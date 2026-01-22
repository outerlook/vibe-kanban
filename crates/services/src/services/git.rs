use std::{collections::HashMap, path::Path};

use chrono::{DateTime, Utc};
use git2::{BranchType, Error as GitError, Reference, Remote, Repository};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;
use utils::diff::{Diff, DiffChangeKind, compute_line_change_counts};

mod cli;

use cli::{ChangeType, StatusDiffEntry, StatusDiffOptions};
pub use cli::{GitCli, GitCliError};

use super::gix_reader::{DiffChangeType, FileStat, GixReader, GixReaderError, TreeDiffEntry};
use crate::services::github::GitHubRepoInfo;

#[derive(Debug, Error)]
pub enum GitServiceError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    GitCLI(#[from] GitCliError),
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error(transparent)]
    GixReader(#[from] GixReaderError),
    #[error("Invalid repository: {0}")]
    InvalidRepository(String),
    #[error("Branch not found: {0}")]
    BranchNotFound(String),
    #[error("Merge conflicts: {0}")]
    MergeConflicts(String),
    #[error("Branches diverged: {0}")]
    BranchesDiverged(String),
    #[error("{0} has uncommitted changes: {1}")]
    WorktreeDirty(String, String),
    #[error("Rebase in progress; resolve or abort it before retrying")]
    RebaseInProgress,
}
/// Service for managing Git operations in task execution workflows
#[derive(Clone)]
pub struct GitService {}

// Max inline diff size for UI (in bytes). Files larger than this will have
// their contents omitted from the diff stream to avoid UI crashes.
const MAX_INLINE_DIFF_BYTES: usize = 2 * 1024 * 1024; // ~2MB

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum ConflictOp {
    Rebase,
    Merge,
    CherryPick,
    Revert,
}

#[derive(Debug, Serialize, TS)]
pub struct GitBranch {
    pub name: String,
    pub is_current: bool,
    pub is_remote: bool,
    #[ts(type = "Date")]
    pub last_commit_date: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct HeadInfo {
    pub branch: String,
    pub oid: String,
}

#[derive(Debug, Clone)]
pub struct Commit(String);

impl Commit {
    pub fn new(id: impl ToString) -> Self {
        Self(id.to_string())
    }
    pub fn as_oid(&self) -> git2::Oid {
        git2::Oid::from_str(&self.0).expect("Commit contains invalid OID")
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Commit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WorktreeResetOptions {
    pub perform_reset: bool,
    pub force_when_dirty: bool,
    pub is_dirty: bool,
    pub log_skip_when_dirty: bool,
}

impl WorktreeResetOptions {
    pub fn new(
        perform_reset: bool,
        force_when_dirty: bool,
        is_dirty: bool,
        log_skip_when_dirty: bool,
    ) -> Self {
        Self {
            perform_reset,
            force_when_dirty,
            is_dirty,
            log_skip_when_dirty,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct WorktreeResetOutcome {
    pub needed: bool,
    pub applied: bool,
}

/// Target for diff generation
pub enum DiffTarget<'p> {
    /// Work-in-progress branch checked out in this worktree
    Worktree {
        worktree_path: &'p Path,
        base_commit: &'p Commit,
    },
    /// Fully committed branch vs base branch
    Branch {
        repo_path: &'p Path,
        branch_name: &'p str,
        base_branch: &'p str,
    },
    /// Specific commit vs base branch
    Commit {
        repo_path: &'p Path,
        commit_sha: &'p str,
    },
}

impl Default for GitService {
    fn default() -> Self {
        Self::new()
    }
}

impl GitService {
    /// Create a new GitService for the given repository path
    pub fn new() -> Self {
        Self {}
    }

    pub fn is_branch_name_valid(&self, name: &str) -> bool {
        git2::Branch::name_is_valid(name).unwrap_or(false)
    }

    /// Create a new local branch from HEAD or a base branch.
    pub fn create_branch(
        &self,
        repo_path: &Path,
        branch_name: &str,
        base_branch: Option<&str>,
    ) -> Result<(), GitServiceError> {
        let git = GitCli::new();
        git.create_branch(repo_path, branch_name, base_branch)?;
        Ok(())
    }

    /// Open the repository
    fn open_repo(&self, repo_path: &Path) -> Result<Repository, GitServiceError> {
        Repository::open(repo_path).map_err(GitServiceError::from)
    }

    /// Ensure local (repo-scoped) identity exists for CLI commits.
    /// Sets user.name/email only if missing in the repo config.
    fn ensure_cli_commit_identity(&self, repo_path: &Path) -> Result<(), GitServiceError> {
        let repo = self.open_repo(repo_path)?;
        let cfg = repo.config()?;
        let has_name = cfg.get_string("user.name").is_ok();
        let has_email = cfg.get_string("user.email").is_ok();
        if !(has_name && has_email) {
            let mut cfg = repo.config()?;
            cfg.set_str("user.name", "Vibe Kanban")?;
            cfg.set_str("user.email", "noreply@vibekanban.com")?;
        }
        Ok(())
    }

    /// Get a signature for libgit2 commits with a safe fallback identity.
    fn signature_with_fallback<'a>(
        &self,
        repo: &'a Repository,
    ) -> Result<git2::Signature<'a>, GitServiceError> {
        match repo.signature() {
            Ok(sig) => Ok(sig),
            Err(_) => git2::Signature::now("Vibe Kanban", "noreply@vibekanban.com")
                .map_err(GitServiceError::from),
        }
    }

    pub fn default_remote_name(&self, repo: &Repository) -> String {
        if let Ok(repos) = repo.remotes() {
            repos
                .iter()
                .flatten()
                .next()
                .map(|r| r.to_owned())
                .unwrap_or_else(|| "origin".to_string())
        } else {
            "origin".to_string()
        }
    }

    /// Initialize a new git repository with a main branch and initial commit
    pub fn initialize_repo_with_main_branch(
        &self,
        repo_path: &Path,
    ) -> Result<(), GitServiceError> {
        // Create directory if it doesn't exist
        if !repo_path.exists() {
            std::fs::create_dir_all(repo_path)?;
        }

        // Initialize git repository with main branch
        let repo = Repository::init_opts(
            repo_path,
            git2::RepositoryInitOptions::new()
                .initial_head("main")
                .mkdir(true),
        )?;

        // Create initial commit
        self.create_initial_commit(&repo)?;

        Ok(())
    }

    /// Ensure an existing repository has a main branch (for empty repos)
    pub fn ensure_main_branch_exists(&self, repo_path: &Path) -> Result<(), GitServiceError> {
        let repo = self.open_repo(repo_path)?;

        match repo.branches(None) {
            Ok(branches) => {
                if branches.count() == 0 {
                    // No branches exist - create initial commit on main branch
                    self.create_initial_commit(&repo)?;
                }
            }
            Err(e) => {
                return Err(GitServiceError::InvalidRepository(format!(
                    "Failed to list branches: {e}"
                )));
            }
        }
        Ok(())
    }

    pub fn create_initial_commit(&self, repo: &Repository) -> Result<(), GitServiceError> {
        let signature = self.signature_with_fallback(repo)?;

        let tree_id = {
            let tree_builder = repo.treebuilder(None)?;
            tree_builder.write()?
        };
        let tree = repo.find_tree(tree_id)?;

        // Create initial commit on main branch
        let _commit_id = repo.commit(
            Some("refs/heads/main"),
            &signature,
            &signature,
            "Initial commit",
            &tree,
            &[],
        )?;

        // Set HEAD to point to main branch
        repo.set_head("refs/heads/main")?;

        Ok(())
    }

    pub fn commit(&self, path: &Path, message: &str) -> Result<bool, GitServiceError> {
        // Use Git CLI to respect sparse-checkout semantics for staging and commit
        let git = GitCli::new();
        let has_changes = git
            .has_changes(path)
            .map_err(|e| GitServiceError::InvalidRepository(format!("git status failed: {e}")))?;
        if !has_changes {
            tracing::debug!("No changes to commit!");
            return Ok(false);
        }

        git.add_all(path)
            .map_err(|e| GitServiceError::InvalidRepository(format!("git add failed: {e}")))?;
        // Only ensure identity once we know we're about to commit
        self.ensure_cli_commit_identity(path)?;
        git.commit(path, message)
            .map_err(|e| GitServiceError::InvalidRepository(format!("git commit failed: {e}")))?;
        Ok(true)
    }

    /// Get diffs between branches or worktree changes
    pub fn get_diffs(
        &self,
        target: DiffTarget,
        path_filter: Option<&[&str]>,
    ) -> Result<Vec<Diff>, GitServiceError> {
        match target {
            DiffTarget::Worktree {
                worktree_path,
                base_commit,
            } => {
                // Use Git CLI to compute diff vs base to avoid sparse false deletions
                let repo = Repository::open(worktree_path)?;
                let base_tree = repo
                    .find_commit(base_commit.as_oid())?
                    .tree()
                    .map_err(|e| {
                        GitServiceError::InvalidRepository(format!(
                            "Failed to find base commit tree: {e}"
                        ))
                    })?;

                let git = GitCli::new();
                let cli_opts = StatusDiffOptions {
                    path_filter: path_filter.map(|fs| fs.iter().map(|s| s.to_string()).collect()),
                };
                let entries = git
                    .diff_status(worktree_path, base_commit, cli_opts)
                    .map_err(|e| {
                        GitServiceError::InvalidRepository(format!("git diff failed: {e}"))
                    })?;
                Ok(entries
                    .into_iter()
                    .map(|e| Self::status_entry_to_diff(&repo, &base_tree, e))
                    .collect())
            }
            DiffTarget::Branch {
                repo_path,
                branch_name,
                base_branch,
            } => {
                // Use gix for tree-to-tree diff
                let gix_repo = GixReader::open(repo_path)?;

                // Resolve branch references to tree OIDs using git2 for reference resolution
                // (gix reference resolution requires more setup)
                let git2_repo = self.open_repo(repo_path)?;
                let base_tree_oid = Self::find_branch(&git2_repo, base_branch)?
                    .get()
                    .peel_to_commit()?
                    .tree()?
                    .id();
                let branch_tree_oid = Self::find_branch(&git2_repo, branch_name)?
                    .get()
                    .peel_to_commit()?
                    .tree()?
                    .id();

                // Convert git2 OIDs to gix OIDs
                let base_gix_oid = gix::ObjectId::from_bytes_or_panic(base_tree_oid.as_bytes());
                let branch_gix_oid = gix::ObjectId::from_bytes_or_panic(branch_tree_oid.as_bytes());

                // Perform diff using gix
                let mut entries = GixReader::diff_trees(&gix_repo, base_gix_oid, branch_gix_oid)?;

                // Apply path filter if specified
                if let Some(paths) = path_filter {
                    entries.retain(|e| {
                        let check_path = e.new_path.as_deref().or(e.old_path.as_deref());
                        if let Some(p) = check_path {
                            paths.iter().any(|filter| p.starts_with(*filter))
                        } else {
                            false
                        }
                    });
                }

                Self::convert_gix_diff_entries(&gix_repo, entries)
            }
            DiffTarget::Commit {
                repo_path,
                commit_sha,
            } => {
                // Use gix for tree-to-tree diff
                let gix_repo = GixReader::open(repo_path)?;

                // Resolve commit and parent using git2 for reference resolution
                let git2_repo = self.open_repo(repo_path)?;
                let commit_oid = git2::Oid::from_str(commit_sha).map_err(|_| {
                    GitServiceError::InvalidRepository(format!("Invalid commit SHA: {commit_sha}"))
                })?;
                let commit = git2_repo.find_commit(commit_oid)?;
                let parent = commit.parent(0).map_err(|_| {
                    GitServiceError::InvalidRepository(
                        "Commit has no parent; cannot diff a squash merge without a baseline"
                            .into(),
                    )
                })?;

                let parent_tree_oid = parent.tree()?.id();
                let commit_tree_oid = commit.tree()?.id();

                // Convert git2 OIDs to gix OIDs
                let parent_gix_oid = gix::ObjectId::from_bytes_or_panic(parent_tree_oid.as_bytes());
                let commit_gix_oid = gix::ObjectId::from_bytes_or_panic(commit_tree_oid.as_bytes());

                // Perform diff using gix
                let mut entries = GixReader::diff_trees(&gix_repo, parent_gix_oid, commit_gix_oid)?;

                // Apply path filter if specified
                if let Some(paths) = path_filter {
                    entries.retain(|e| {
                        let check_path = e.new_path.as_deref().or(e.old_path.as_deref());
                        if let Some(p) = check_path {
                            paths.iter().any(|filter| p.starts_with(*filter))
                        } else {
                            false
                        }
                    });
                }

                Self::convert_gix_diff_entries(&gix_repo, entries)
            }
        }
    }
    /// Convert gix TreeDiffEntry results to our Diff structs using gix for blob reading.
    ///
    /// This is the gix-based replacement for `convert_diff_to_file_diffs`.
    fn convert_gix_diff_entries(
        gix_repo: &gix::Repository,
        entries: Vec<TreeDiffEntry>,
    ) -> Result<Vec<Diff>, GitServiceError> {
        let mut file_diffs = Vec::new();

        for entry in entries {
            // Convert change type
            let mut change = match entry.change_type {
                DiffChangeType::Added => DiffChangeKind::Added,
                DiffChangeType::Deleted => DiffChangeKind::Deleted,
                DiffChangeType::Modified => DiffChangeKind::Modified,
                DiffChangeType::Renamed => DiffChangeKind::Renamed,
                DiffChangeType::Copied => DiffChangeKind::Copied,
            };

            // Check blob sizes to decide if content should be omitted
            let mut content_omitted = false;

            // Check old blob size
            if let Some(old_oid) = entry.old_oid {
                if let Ok(size) = GixReader::blob_size(gix_repo, old_oid) {
                    if size > MAX_INLINE_DIFF_BYTES {
                        content_omitted = true;
                    }
                }
            }

            // Check new blob size
            if let Some(new_oid) = entry.new_oid {
                if let Ok(size) = GixReader::blob_size(gix_repo, new_oid) {
                    if size > MAX_INLINE_DIFF_BYTES {
                        content_omitted = true;
                    }
                }
            }

            // Get old content if not omitted
            let old_content = if content_omitted {
                None
            } else if let Some(old_oid) = entry.old_oid {
                GixReader::read_blob(gix_repo, old_oid).ok().flatten()
            } else {
                None
            };

            // Get new content if not omitted
            let new_content = if content_omitted {
                None
            } else if let Some(new_oid) = entry.new_oid {
                GixReader::read_blob(gix_repo, new_oid).ok().flatten()
            } else {
                None
            };

            // Detect pure permission changes (content identical, mode differs)
            if matches!(change, DiffChangeKind::Modified) {
                let mode_changed = entry.old_mode != entry.new_mode;
                if mode_changed
                    && old_content.is_some()
                    && new_content.is_some()
                    && old_content == new_content
                {
                    change = DiffChangeKind::PermissionChange;
                }
            }

            // Compute line stats if we have content
            let (additions, deletions) = match (&old_content, &new_content) {
                (Some(old), Some(new)) => {
                    let (adds, dels) = compute_line_change_counts(old, new);
                    (Some(adds), Some(dels))
                }
                (None, Some(new)) => {
                    // All lines are additions
                    let adds = new.lines().count();
                    (Some(adds), Some(0))
                }
                (Some(old), None) => {
                    // All lines are deletions
                    let dels = old.lines().count();
                    (Some(0), Some(dels))
                }
                (None, None) => (None, None),
            };

            file_diffs.push(Diff {
                change,
                old_path: entry.old_path,
                new_path: entry.new_path,
                old_content,
                new_content,
                content_omitted,
                additions,
                deletions,
            });
        }

        Ok(file_diffs)
    }

    /// Extract file path from a Diff (for indexing and ConversationPatch)
    pub fn diff_path(diff: &Diff) -> String {
        diff.new_path
            .clone()
            .or_else(|| diff.old_path.clone())
            .unwrap_or_default()
    }

    /// Helper function to convert blob to string content
    fn blob_to_string(blob: &git2::Blob) -> Option<String> {
        if blob.is_binary() {
            None // Skip binary files
        } else {
            std::str::from_utf8(blob.content())
                .ok()
                .map(|s| s.to_string())
        }
    }

    /// Helper function to read file content from filesystem with safety guards
    fn read_file_to_string(repo: &Repository, rel_path: &Path) -> Option<String> {
        let workdir = repo.workdir()?;
        let abs_path = workdir.join(rel_path);

        // Read file from filesystem
        let bytes = match std::fs::read(&abs_path) {
            Ok(bytes) => bytes,
            Err(e) => {
                tracing::debug!("Failed to read file from filesystem: {:?}: {}", abs_path, e);
                return None;
            }
        };

        // Size guard - skip files larger than UI inline threshold
        if bytes.len() > MAX_INLINE_DIFF_BYTES {
            tracing::debug!(
                "Skipping large file ({}KB): {:?}",
                bytes.len() / 1024,
                abs_path
            );
            return None;
        }

        // Binary guard - skip files containing null bytes
        if bytes.contains(&0) {
            tracing::debug!("Skipping binary file: {:?}", abs_path);
            return None;
        }

        // UTF-8 validation
        match String::from_utf8(bytes) {
            Ok(content) => Some(content),
            Err(e) => {
                tracing::debug!("File is not valid UTF-8: {:?}: {}", abs_path, e);
                None
            }
        }
    }

    /// Create Diff entries from git_cli::StatusDiffEntry
    /// New Diff format is flattened with change kind, paths, and optional contents.
    fn status_entry_to_diff(repo: &Repository, base_tree: &git2::Tree, e: StatusDiffEntry) -> Diff {
        // Map ChangeType to DiffChangeKind
        let mut change = match e.change {
            ChangeType::Added => DiffChangeKind::Added,
            ChangeType::Deleted => DiffChangeKind::Deleted,
            ChangeType::Modified => DiffChangeKind::Modified,
            ChangeType::Renamed => DiffChangeKind::Renamed,
            ChangeType::Copied => DiffChangeKind::Copied,
            // Treat type changes and unmerged as modified for now
            ChangeType::TypeChanged | ChangeType::Unmerged => DiffChangeKind::Modified,
            ChangeType::Unknown(_) => DiffChangeKind::Modified,
        };

        // Determine old/new paths based on change
        let (old_path_opt, new_path_opt): (Option<String>, Option<String>) = match e.change {
            ChangeType::Added => (None, Some(e.path.clone())),
            ChangeType::Deleted => (Some(e.old_path.unwrap_or(e.path.clone())), None),
            ChangeType::Modified | ChangeType::TypeChanged | ChangeType::Unmerged => (
                Some(e.old_path.unwrap_or(e.path.clone())),
                Some(e.path.clone()),
            ),
            ChangeType::Renamed | ChangeType::Copied => (e.old_path.clone(), Some(e.path.clone())),
            ChangeType::Unknown(_) => (e.old_path.clone(), Some(e.path.clone())),
        };

        // Decide if we should omit content by size (either side)
        let mut content_omitted = false;
        // Old side (from base tree)
        if let Some(ref oldp) = old_path_opt {
            let rel = std::path::Path::new(oldp);
            if let Ok(entry) = base_tree.get_path(rel)
                && entry.kind() == Some(git2::ObjectType::Blob)
                && let Ok(blob) = repo.find_blob(entry.id())
                && !blob.is_binary()
                && blob.size() > MAX_INLINE_DIFF_BYTES
            {
                content_omitted = true;
            }
        }
        // New side (from filesystem)
        if let Some(ref newp) = new_path_opt
            && let Some(workdir) = repo.workdir()
        {
            let abs = workdir.join(newp);
            if let Ok(md) = std::fs::metadata(&abs)
                && (md.len() as usize) > MAX_INLINE_DIFF_BYTES
            {
                content_omitted = true;
            }
        }

        // Load contents only if not omitted
        let (old_content, new_content) = if content_omitted {
            (None, None)
        } else {
            // Load old content from base tree if possible
            let old_content = if let Some(ref oldp) = old_path_opt {
                let rel = std::path::Path::new(oldp);
                match base_tree.get_path(rel) {
                    Ok(entry) if entry.kind() == Some(git2::ObjectType::Blob) => repo
                        .find_blob(entry.id())
                        .ok()
                        .and_then(|b| Self::blob_to_string(&b)),
                    _ => None,
                }
            } else {
                None
            };

            // Load new content from filesystem (worktree) when available
            let new_content = if let Some(ref newp) = new_path_opt {
                let rel = std::path::Path::new(newp);
                Self::read_file_to_string(repo, rel)
            } else {
                None
            };
            (old_content, new_content)
        };

        // If reported as Modified but content is identical, treat as a permission-only change
        if matches!(change, DiffChangeKind::Modified)
            && old_content.is_some()
            && new_content.is_some()
            && old_content == new_content
        {
            change = DiffChangeKind::PermissionChange;
        }

        // Compute line stats from available content
        let (additions, deletions) = match (&old_content, &new_content) {
            (Some(old), Some(new)) => {
                let (adds, dels) = compute_line_change_counts(old, new);
                (Some(adds), Some(dels))
            }
            (Some(old), None) => {
                // File deleted - all lines are deletions
                (Some(0), Some(old.lines().count()))
            }
            (None, Some(new)) => {
                // File added - all lines are additions
                (Some(new.lines().count()), Some(0))
            }
            (None, None) => (None, None),
        };

        Diff {
            change,
            old_path: old_path_opt,
            new_path: new_path_opt,
            old_content,
            new_content,
            content_omitted,
            additions,
            deletions,
        }
    }

    /// Find where a branch is currently checked out
    pub fn find_checkout_path_for_branch(
        &self,
        repo_path: &Path,
        branch_name: &str,
    ) -> Result<Option<std::path::PathBuf>, GitServiceError> {
        let git_cli = GitCli::new();
        let worktrees = git_cli.list_worktrees(repo_path).map_err(|e| {
            GitServiceError::InvalidRepository(format!("git worktree list failed: {e}"))
        })?;

        for worktree in worktrees {
            if let Some(ref branch) = worktree.branch
                && branch == branch_name
            {
                return Ok(Some(std::path::PathBuf::from(worktree.path)));
            }
        }
        Ok(None)
    }

    /// Merge changes from a task branch into the base branch.
    pub fn merge_changes(
        &self,
        base_worktree_path: &Path,
        task_worktree_path: &Path,
        task_branch_name: &str,
        base_branch_name: &str,
        commit_message: &str,
    ) -> Result<String, GitServiceError> {
        // Open the repositories
        let task_repo = self.open_repo(task_worktree_path)?;
        let base_repo = self.open_repo(base_worktree_path)?;

        // Check if base branch is ahead of task branch - this indicates the base has moved
        // ahead since the task was created, which should block the merge
        let (_, task_behind) =
            self.get_branch_status(base_worktree_path, task_branch_name, base_branch_name)?;

        if task_behind > 0 {
            return Err(GitServiceError::BranchesDiverged(format!(
                "Cannot merge: base branch '{base_branch_name}' is {task_behind} commits ahead of task branch '{task_branch_name}'. The base branch has moved forward since the task was created.",
            )));
        }

        // Check where base branch is checked out (if anywhere)
        match self.find_checkout_path_for_branch(base_worktree_path, base_branch_name)? {
            Some(base_checkout_path) => {
                // base branch is checked out somewhere - use CLI merge
                let git_cli = GitCli::new();

                // Safety check: base branch has no staged changes
                if git_cli
                    .has_staged_changes(&base_checkout_path)
                    .map_err(|e| {
                        GitServiceError::InvalidRepository(format!("git diff --cached failed: {e}"))
                    })?
                {
                    return Err(GitServiceError::WorktreeDirty(
                        base_branch_name.to_string(),
                        "staged changes present".to_string(),
                    ));
                }

                // Use CLI merge in base context
                self.ensure_cli_commit_identity(&base_checkout_path)?;
                let sha = git_cli
                    .merge_squash_commit(
                        &base_checkout_path,
                        base_branch_name,
                        task_branch_name,
                        commit_message,
                    )
                    .map_err(|e| {
                        GitServiceError::InvalidRepository(format!("CLI merge failed: {e}"))
                    })?;

                // Update task branch ref for continuity
                let task_refname = format!("refs/heads/{task_branch_name}");
                git_cli
                    .update_ref(base_worktree_path, &task_refname, &sha)
                    .map_err(|e| {
                        GitServiceError::InvalidRepository(format!("git update-ref failed: {e}"))
                    })?;

                Ok(sha)
            }
            None => {
                // base branch not checked out anywhere - use libgit2 pure ref operations
                let task_branch = Self::find_branch(&task_repo, task_branch_name)?;
                let base_branch = Self::find_branch(&task_repo, base_branch_name)?;

                // Resolve commits
                let base_commit = base_branch.get().peel_to_commit()?;
                let task_commit = task_branch.get().peel_to_commit()?;

                // Create the squash commit in-memory (no checkout) and update the base branch ref
                let signature = self.signature_with_fallback(&task_repo)?;
                let squash_commit_id = self.perform_squash_merge(
                    &task_repo,
                    &base_commit,
                    &task_commit,
                    &signature,
                    commit_message,
                    base_branch_name,
                )?;

                // Update the task branch to the new squash commit so follow-up
                // work can continue from the merged state without conflicts.
                let task_refname = format!("refs/heads/{task_branch_name}");
                base_repo.reference(
                    &task_refname,
                    squash_commit_id,
                    true,
                    "Reset task branch after squash merge",
                )?;

                Ok(squash_commit_id.to_string())
            }
        }
    }
    /// Compute ahead/behind between two OIDs using gix.
    fn ahead_behind_by_oid_gix(
        repo_path: &Path,
        local_oid: &str,
        remote_oid: &str,
    ) -> Result<(usize, usize), GitServiceError> {
        let gix_repo = GixReader::open(repo_path)?;
        let (ahead, behind) = GixReader::ahead_behind_by_oid(&gix_repo, local_oid, remote_oid)?;
        Ok((ahead, behind))
    }

    fn get_branch_status_inner(
        &self,
        repo_path: &Path,
        branch_ref: &Reference,
        base_branch_ref: &Reference,
    ) -> Result<(usize, usize), GitServiceError> {
        let local_oid = branch_ref
            .target()
            .ok_or(GitServiceError::BranchNotFound(
                "Branch not found".to_string(),
            ))?
            .to_string();
        let remote_oid = base_branch_ref
            .target()
            .ok_or(GitServiceError::BranchNotFound(
                "Base branch not found".to_string(),
            ))?
            .to_string();
        Self::ahead_behind_by_oid_gix(repo_path, &local_oid, &remote_oid)
    }

    pub fn get_branch_status(
        &self,
        repo_path: &Path,
        branch_name: &str,
        base_branch_name: &str,
    ) -> Result<(usize, usize), GitServiceError> {
        let repo = Repository::open(repo_path)?;
        let branch = Self::find_branch(&repo, branch_name)?;
        let base_branch = Self::find_branch(&repo, base_branch_name)?;
        self.get_branch_status_inner(
            repo_path,
            &branch.into_reference(),
            &base_branch.into_reference(),
        )
    }

    pub fn get_base_commit(
        &self,
        repo_path: &Path,
        branch_name: &str,
        base_branch_name: &str,
    ) -> Result<Commit, GitServiceError> {
        // Resolve branch refs to OIDs using git2
        let repo = Repository::open(repo_path)?;
        let branch_oid = Self::find_branch(&repo, branch_name)?
            .get()
            .peel_to_commit()?
            .id()
            .to_string();
        let base_branch_oid = Self::find_branch(&repo, base_branch_name)?
            .get()
            .peel_to_commit()?
            .id()
            .to_string();

        // Use gix for merge_base calculation
        let gix_repo = GixReader::open(repo_path)?;
        let merge_base = GixReader::merge_base_by_oid(&gix_repo, &branch_oid, &base_branch_oid)?;
        Ok(Commit::new(merge_base))
    }

    pub fn get_remote_branch_status(
        &self,
        repo_path: &Path,
        branch_name: &str,
        base_branch_name: Option<&str>,
    ) -> Result<(usize, usize), GitServiceError> {
        let repo = Repository::open(repo_path)?;
        let branch_ref = Self::find_branch(&repo, branch_name)?.into_reference();
        // base branch is either given or upstream of branch_name
        let base_branch_ref = if let Some(bn) = base_branch_name {
            Self::find_branch(&repo, bn)?
        } else {
            repo.find_branch(branch_name, BranchType::Local)?
                .upstream()?
        }
        .into_reference();
        let remote = self.get_remote_from_branch_ref(&repo, &base_branch_ref)?;
        self.fetch_all_from_remote(&repo, &remote)?;
        self.get_branch_status_inner(repo_path, &branch_ref, &base_branch_ref)
    }

    pub fn is_worktree_clean(&self, worktree_path: &Path) -> Result<bool, GitServiceError> {
        match self.check_worktree_clean(worktree_path) {
            Ok(()) => Ok(true),
            Err(GitServiceError::WorktreeDirty(_, _)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Check if the worktree is clean (no uncommitted changes to tracked files)
    fn check_worktree_clean(&self, worktree_path: &Path) -> Result<(), GitServiceError> {
        let dirty_files = GixReader::get_dirty_files(worktree_path).map_err(|e| {
            GitServiceError::InvalidRepository(format!("gix dirty check failed: {e}"))
        })?;

        if !dirty_files.is_empty() {
            // Get branch name for error message
            let branch_name = self
                .get_head_info(worktree_path)
                .map(|h| h.branch)
                .unwrap_or_else(|_| "unknown branch".to_string());
            return Err(GitServiceError::WorktreeDirty(
                branch_name,
                dirty_files.join(", "),
            ));
        }

        Ok(())
    }

    /// Get current HEAD information including branch name and commit OID
    pub fn get_head_info(&self, repo_path: &Path) -> Result<HeadInfo, GitServiceError> {
        let gix_info = GixReader::head_info(repo_path).map_err(|e| {
            GitServiceError::InvalidRepository(format!("Failed to get HEAD info: {}", e))
        })?;

        Ok(HeadInfo {
            branch: gix_info.branch,
            oid: gix_info.oid,
        })
    }

    pub fn get_current_branch(&self, repo_path: &Path) -> Result<String, git2::Error> {
        // Thin wrapper for backward compatibility
        match self.get_head_info(repo_path) {
            Ok(head_info) => Ok(head_info.branch),
            Err(GitServiceError::Git(git_err)) => Err(git_err),
            Err(_) => Err(git2::Error::from_str("Failed to get head info")),
        }
    }

    /// Get the commit OID (as hex string) for a given branch without modifying HEAD
    pub fn get_branch_oid(
        &self,
        repo_path: &Path,
        branch_name: &str,
    ) -> Result<String, GitServiceError> {
        GixReader::branch_oid(repo_path, branch_name)
            .map_err(|_| GitServiceError::BranchNotFound(branch_name.to_string()))
    }

    /// Get the subject/summary line for a given commit OID
    pub fn get_commit_subject(
        &self,
        repo_path: &Path,
        commit_sha: &str,
    ) -> Result<String, GitServiceError> {
        let gix_repo = GixReader::open(repo_path)?;
        let message = GixReader::commit_message_by_oid(&gix_repo, commit_sha)?;
        // Extract subject line (first line before any newlines)
        let subject = message.lines().next().unwrap_or("(no subject)").to_string();
        Ok(subject)
    }

    /// Compare two OIDs and return (ahead, behind) counts: how many commits
    /// `from_oid` is ahead of and behind `to_oid`.
    pub fn ahead_behind_commits_by_oid(
        &self,
        repo_path: &Path,
        from_oid: &str,
        to_oid: &str,
    ) -> Result<(usize, usize), GitServiceError> {
        Self::ahead_behind_by_oid_gix(repo_path, from_oid, to_oid)
    }

    /// Return (uncommitted_tracked_changes, untracked_files) counts in worktree
    pub fn get_worktree_change_counts(
        &self,
        worktree_path: &Path,
    ) -> Result<(usize, usize), GitServiceError> {
        let summary = GixReader::get_worktree_status(worktree_path).map_err(|e| {
            GitServiceError::InvalidRepository(format!("gix status failed: {e}"))
        })?;
        Ok((summary.uncommitted_tracked, summary.untracked))
    }

    /// Evaluate whether any action is needed to reset to `target_commit_oid` and
    /// optionally perform the actions.
    pub fn reconcile_worktree_to_commit(
        &self,
        worktree_path: &Path,
        target_commit_oid: &str,
        options: WorktreeResetOptions,
    ) -> WorktreeResetOutcome {
        let WorktreeResetOptions {
            perform_reset,
            force_when_dirty,
            is_dirty,
            log_skip_when_dirty,
        } = options;

        let head_oid = self.get_head_info(worktree_path).ok().map(|h| h.oid);
        let mut outcome = WorktreeResetOutcome::default();

        if head_oid.as_deref() != Some(target_commit_oid) || is_dirty {
            outcome.needed = true;

            if perform_reset {
                if is_dirty && !force_when_dirty {
                    if log_skip_when_dirty {
                        tracing::warn!("Worktree dirty; skipping reset as not forced");
                    }
                } else if let Err(e) = self.reset_worktree_to_commit(
                    worktree_path,
                    target_commit_oid,
                    force_when_dirty,
                ) {
                    tracing::error!("Failed to reset worktree: {}", e);
                } else {
                    outcome.applied = true;
                }
            }
        }

        outcome
    }

    /// Reset the given worktree to the specified commit SHA.
    /// If `force` is false and the worktree is dirty, returns WorktreeDirty error.
    pub fn reset_worktree_to_commit(
        &self,
        worktree_path: &Path,
        commit_sha: &str,
        force: bool,
    ) -> Result<(), GitServiceError> {
        if !force {
            // Avoid clobbering uncommitted changes unless explicitly forced
            self.check_worktree_clean(worktree_path)?;
        }
        let cli = GitCli::new();
        cli.git(worktree_path, ["reset", "--hard", commit_sha])
            .map_err(|e| {
                GitServiceError::InvalidRepository(format!("git reset --hard failed: {e}"))
            })?;
        // Reapply sparse-checkout if configured (non-fatal)
        let _ = cli.git(worktree_path, ["sparse-checkout", "reapply"]);
        Ok(())
    }

    /// Add a worktree for a branch, optionally creating the branch
    pub fn add_worktree(
        &self,
        repo_path: &Path,
        worktree_path: &Path,
        branch: &str,
        create_branch: bool,
    ) -> Result<(), GitServiceError> {
        let git = GitCli::new();
        git.worktree_add(repo_path, worktree_path, branch, create_branch)
            .map_err(|e| match e {
                GitCliError::InvalidReference(_) => {
                    GitServiceError::BranchNotFound(branch.to_string())
                }
                _ => GitServiceError::InvalidRepository(e.to_string()),
            })?;
        Ok(())
    }

    /// Remove a worktree
    pub fn remove_worktree(
        &self,
        repo_path: &Path,
        worktree_path: &Path,
        force: bool,
    ) -> Result<(), GitServiceError> {
        let git = GitCli::new();
        git.worktree_remove(repo_path, worktree_path, force)
            .map_err(|e| GitServiceError::InvalidRepository(e.to_string()))?;
        Ok(())
    }

    /// Move a worktree to a new location
    pub fn move_worktree(
        &self,
        repo_path: &Path,
        old_path: &Path,
        new_path: &Path,
    ) -> Result<(), GitServiceError> {
        let git = GitCli::new();
        git.worktree_move(repo_path, old_path, new_path)
            .map_err(|e| GitServiceError::InvalidRepository(e.to_string()))?;
        Ok(())
    }

    pub fn prune_worktrees(&self, repo_path: &Path) -> Result<(), GitServiceError> {
        let git = GitCli::new();
        git.worktree_prune(repo_path)
            .map_err(|e| GitServiceError::InvalidRepository(e.to_string()))?;
        Ok(())
    }

    pub fn get_all_branches(&self, repo_path: &Path) -> Result<Vec<GitBranch>, git2::Error> {
        let repo = Repository::open(repo_path)?;
        let current_branch = self.get_current_branch(repo_path).unwrap_or_default();
        let mut branches = Vec::new();

        // Helper function to get last commit date for a branch
        let get_last_commit_date = |branch: &git2::Branch| -> Result<DateTime<Utc>, git2::Error> {
            if let Some(target) = branch.get().target()
                && let Ok(commit) = repo.find_commit(target)
            {
                let timestamp = commit.time().seconds();
                return Ok(DateTime::from_timestamp(timestamp, 0).unwrap_or_else(Utc::now));
            }
            Ok(Utc::now()) // Default to now if we can't get the commit date
        };

        // Get local branches
        let local_branches = repo.branches(Some(BranchType::Local))?;
        for branch_result in local_branches {
            let (branch, _) = branch_result?;
            if let Some(name) = branch.name()? {
                let last_commit_date = get_last_commit_date(&branch)?;
                branches.push(GitBranch {
                    name: name.to_string(),
                    is_current: name == current_branch,
                    is_remote: false,
                    last_commit_date,
                });
            }
        }

        // Get remote branches
        let remote_branches = repo.branches(Some(BranchType::Remote))?;
        for branch_result in remote_branches {
            let (branch, _) = branch_result?;
            if let Some(name) = branch.name()? {
                // Skip remote HEAD references
                if !name.ends_with("/HEAD") {
                    let last_commit_date = get_last_commit_date(&branch)?;
                    branches.push(GitBranch {
                        name: name.to_string(),
                        is_current: false,
                        is_remote: true,
                        last_commit_date,
                    });
                }
            }
        }

        // Sort branches: current first, then by most recent commit date
        branches.sort_by(|a, b| {
            if a.is_current && !b.is_current {
                std::cmp::Ordering::Less
            } else if !a.is_current && b.is_current {
                std::cmp::Ordering::Greater
            } else {
                // Sort by most recent commit date (newest first)
                b.last_commit_date.cmp(&a.last_commit_date)
            }
        });

        Ok(branches)
    }

    /// Perform a squash merge of task branch into base branch, but fail on conflicts
    fn perform_squash_merge(
        &self,
        repo: &Repository,
        base_commit: &git2::Commit,
        task_commit: &git2::Commit,
        signature: &git2::Signature,
        commit_message: &str,
        base_branch_name: &str,
    ) -> Result<git2::Oid, GitServiceError> {
        // In-memory merge to detect conflicts without touching the working tree
        let mut merge_opts = git2::MergeOptions::new();
        // Safety and correctness options
        merge_opts.find_renames(true); // improve rename handling
        merge_opts.fail_on_conflict(true); // bail out instead of generating conflicted index
        let mut index = repo.merge_commits(base_commit, task_commit, Some(&merge_opts))?;

        // If there are conflicts, return an error
        if index.has_conflicts() {
            return Err(GitServiceError::MergeConflicts(
                "Merge failed due to conflicts. Please resolve conflicts manually.".to_string(),
            ));
        }

        // Write the merged tree back to the repository
        let tree_id = index.write_tree_to(repo)?;
        let tree = repo.find_tree(tree_id)?;

        // Create a squash commit: use merged tree with base_commit as sole parent
        let squash_commit_id = repo.commit(
            None,           // Don't update any reference yet
            signature,      // Author
            signature,      // Committer
            commit_message, // Custom message
            &tree,          // Merged tree content
            &[base_commit], // Single parent: base branch commit
        )?;

        // Update the base branch reference to point to the new commit
        let refname = format!("refs/heads/{base_branch_name}");
        repo.reference(&refname, squash_commit_id, true, "Squash merge")?;

        Ok(squash_commit_id)
    }

    /// Rebase a worktree branch onto a new base
    pub fn rebase_branch(
        &self,
        repo_path: &Path,
        worktree_path: &Path,
        new_base_branch: &str,
        old_base_branch: &str,
        task_branch: &str,
    ) -> Result<String, GitServiceError> {
        let worktree_repo = Repository::open(worktree_path)?;
        let main_repo = self.open_repo(repo_path)?;

        // Safety guard: never operate on a dirty worktree. This preserves any
        // uncommitted changes to tracked files by failing fast instead of
        // resetting or cherry-picking over them. Untracked files are allowed.
        self.check_worktree_clean(worktree_path)?;

        // If a rebase is already in progress, refuse to proceed instead of
        // aborting (which might destroy user changes mid-rebase).
        let git = GitCli::new();
        if git.is_rebase_in_progress(worktree_path).unwrap_or(false) {
            return Err(GitServiceError::RebaseInProgress);
        }

        // Get the target base branch reference
        let nbr = Self::find_branch(&main_repo, new_base_branch)?.into_reference();
        // If the target base is remote, update it first so CLI sees latest
        if nbr.is_remote() {
            self.fetch_branch_from_remote(&main_repo, &nbr)?;
        }

        // Ensure identity for any commits produced by rebase
        self.ensure_cli_commit_identity(worktree_path)?;
        // Use git CLI rebase to carry out the operation safely
        match git.rebase_onto(worktree_path, new_base_branch, old_base_branch, task_branch) {
            Ok(()) => {}
            Err(GitCliError::RebaseInProgress) => {
                return Err(GitServiceError::RebaseInProgress);
            }
            Err(GitCliError::CommandFailed(stderr)) => {
                // If the CLI indicates conflicts, return a concise, actionable error.
                let looks_like_conflict = stderr.contains("could not apply")
                    || stderr.contains("CONFLICT")
                    || stderr.to_lowercase().contains("resolve all conflicts");
                if looks_like_conflict {
                    // Determine current attempt branch name for clarity
                    let attempt_branch = worktree_repo
                        .head()
                        .ok()
                        .and_then(|h| h.shorthand().map(|s| s.to_string()))
                        .unwrap_or_else(|| "(unknown)".to_string());
                    // List conflicted files (best-effort)
                    let conflicts = git.get_conflicted_files(worktree_path).unwrap_or_default();
                    let files_part = if conflicts.is_empty() {
                        "".to_string()
                    } else {
                        let mut sample = conflicts.clone();
                        let total = sample.len();
                        sample.truncate(10);
                        let list = sample.join(", ");
                        if total > sample.len() {
                            format!(
                                " Conflicted files (showing {} of {}): {}.",
                                sample.len(),
                                total,
                                list
                            )
                        } else {
                            format!(" Conflicted files: {list}.")
                        }
                    };
                    let msg = format!(
                        "Rebase encountered merge conflicts while rebasing '{attempt_branch}' onto '{new_base_branch}'.{files_part} Resolve conflicts and then continue or abort."
                    );
                    return Err(GitServiceError::MergeConflicts(msg));
                }
                return Err(GitServiceError::InvalidRepository(format!(
                    "Rebase failed: {}",
                    stderr.lines().next().unwrap_or("")
                )));
            }
            Err(e) => {
                return Err(GitServiceError::InvalidRepository(format!(
                    "git rebase failed: {e}"
                )));
            }
        }

        // Return resulting HEAD commit
        let final_commit = worktree_repo.head()?.peel_to_commit()?;
        Ok(final_commit.id().to_string())
    }

    pub fn find_branch_type(
        &self,
        repo_path: &Path,
        branch_name: &str,
    ) -> Result<BranchType, GitServiceError> {
        let gix_branch_type = GixReader::branch_type(repo_path, branch_name)
            .map_err(|_| GitServiceError::BranchNotFound(branch_name.to_string()))?;

        // Map gix_reader::BranchType to git2::BranchType
        Ok(match gix_branch_type {
            super::gix_reader::BranchType::Local => BranchType::Local,
            super::gix_reader::BranchType::Remote => BranchType::Remote,
        })
    }

    pub fn check_branch_exists(
        &self,
        repo_path: &Path,
        branch_name: &str,
    ) -> Result<bool, GitServiceError> {
        Ok(GixReader::find_branch(repo_path, branch_name).is_ok())
    }

    pub fn check_remote_branch_exists(
        &self,
        repo_path: &Path,
        branch_name: &str,
    ) -> Result<bool, GitServiceError> {
        let repo = self.open_repo(repo_path)?;
        let default_remote_name = self.default_remote_name(&repo);
        let stripped_branch_name = match self.find_branch_type(repo_path, branch_name) {
            Ok(BranchType::Remote) => {
                // strip remote prefix if present
                Ok(branch_name
                    .strip_prefix(&format!("{default_remote_name}/"))
                    .unwrap_or(branch_name))
            }
            Ok(BranchType::Local) => Ok(branch_name),
            Err(e) => Err(e),
        }?;
        let remote = repo.find_remote(&default_remote_name)?;
        let remote_url = remote
            .url()
            .ok_or_else(|| GitServiceError::InvalidRepository("Remote has no URL".to_string()))?;

        let git_cli = GitCli::new();
        git_cli
            .check_remote_branch_exists(repo_path, remote_url, stripped_branch_name)
            .map_err(|e| e.into())
    }

    pub fn rename_local_branch(
        &self,
        worktree_path: &Path,
        old_branch_name: &str,
        new_branch_name: &str,
    ) -> Result<(), GitServiceError> {
        let repo = self.open_repo(worktree_path)?;

        let mut branch = repo
            .find_branch(old_branch_name, BranchType::Local)
            .map_err(|_| GitServiceError::BranchNotFound(old_branch_name.to_string()))?;

        branch.rename(new_branch_name, false)?;

        repo.set_head(&format!("refs/heads/{new_branch_name}"))?;

        Ok(())
    }

    /// Return true if a rebase is currently in progress in this worktree.
    pub fn is_rebase_in_progress(&self, worktree_path: &Path) -> Result<bool, GitServiceError> {
        let git = GitCli::new();
        git.is_rebase_in_progress(worktree_path).map_err(|e| {
            GitServiceError::InvalidRepository(format!("git rebase state check failed: {e}"))
        })
    }

    pub fn detect_conflict_op(
        &self,
        worktree_path: &Path,
    ) -> Result<Option<ConflictOp>, GitServiceError> {
        let git = GitCli::new();
        if git.is_rebase_in_progress(worktree_path).unwrap_or(false) {
            return Ok(Some(ConflictOp::Rebase));
        }
        if git.is_merge_in_progress(worktree_path).unwrap_or(false) {
            return Ok(Some(ConflictOp::Merge));
        }
        if git
            .is_cherry_pick_in_progress(worktree_path)
            .unwrap_or(false)
        {
            return Ok(Some(ConflictOp::CherryPick));
        }
        if git.is_revert_in_progress(worktree_path).unwrap_or(false) {
            return Ok(Some(ConflictOp::Revert));
        }
        Ok(None)
    }

    /// List conflicted (unmerged) files in the worktree.
    pub fn get_conflicted_files(
        &self,
        worktree_path: &Path,
    ) -> Result<Vec<String>, GitServiceError> {
        let git = GitCli::new();
        git.get_conflicted_files(worktree_path).map_err(|e| {
            GitServiceError::InvalidRepository(format!("git diff for conflicts failed: {e}"))
        })
    }

    /// Abort an in-progress rebase in this worktree (no-op if none).
    pub fn abort_rebase(&self, worktree_path: &Path) -> Result<(), GitServiceError> {
        let git = GitCli::new();
        git.abort_rebase(worktree_path).map_err(|e| {
            GitServiceError::InvalidRepository(format!("git rebase --abort failed: {e}"))
        })
    }

    pub fn abort_conflicts(&self, worktree_path: &Path) -> Result<(), GitServiceError> {
        let git = GitCli::new();
        if git.is_rebase_in_progress(worktree_path).unwrap_or(false) {
            // If there are no conflicted files, prefer `git rebase --quit` to clean up metadata
            let has_conflicts = !self
                .get_conflicted_files(worktree_path)
                .unwrap_or_default()
                .is_empty();
            if has_conflicts {
                return self.abort_rebase(worktree_path);
            } else {
                return git.quit_rebase(worktree_path).map_err(|e| {
                    GitServiceError::InvalidRepository(format!("git rebase --quit failed: {e}"))
                });
            }
        }
        if git.is_merge_in_progress(worktree_path).unwrap_or(false) {
            return git.abort_merge(worktree_path).map_err(|e| {
                GitServiceError::InvalidRepository(format!("git merge --abort failed: {e}"))
            });
        }
        if git
            .is_cherry_pick_in_progress(worktree_path)
            .unwrap_or(false)
        {
            return git.abort_cherry_pick(worktree_path).map_err(|e| {
                GitServiceError::InvalidRepository(format!("git cherry-pick --abort failed: {e}"))
            });
        }
        if git.is_revert_in_progress(worktree_path).unwrap_or(false) {
            return git.abort_revert(worktree_path).map_err(|e| {
                GitServiceError::InvalidRepository(format!("git revert --abort failed: {e}"))
            });
        }
        Ok(())
    }

    pub fn find_branch<'a>(
        repo: &'a Repository,
        branch_name: &str,
    ) -> Result<git2::Branch<'a>, GitServiceError> {
        // Try to find the branch as a local branch first
        match repo.find_branch(branch_name, BranchType::Local) {
            Ok(branch) => Ok(branch),
            Err(_) => {
                // If not found, try to find it as a remote branch
                match repo.find_branch(branch_name, BranchType::Remote) {
                    Ok(branch) => Ok(branch),
                    Err(_) => Err(GitServiceError::BranchNotFound(branch_name.to_string())),
                }
            }
        }
    }

    /// Extract GitHub owner and repo name from git repo path
    pub fn get_github_repo_info(
        &self,
        repo_path: &Path,
    ) -> Result<GitHubRepoInfo, GitServiceError> {
        let repo = self.open_repo(repo_path)?;
        let remote_name = self.default_remote_name(&repo);
        let remote = repo.find_remote(&remote_name).map_err(|_| {
            GitServiceError::InvalidRepository(format!("No '{remote_name}' remote found"))
        })?;

        let url = remote
            .url()
            .ok_or_else(|| GitServiceError::InvalidRepository("Remote has no URL".to_string()))?;
        GitHubRepoInfo::from_remote_url(url).map_err(|e| {
            GitServiceError::InvalidRepository(format!("Failed to parse remote URL: {e}"))
        })
    }

    pub fn get_remote_name_from_branch_name(
        &self,
        repo_path: &Path,
        branch_name: &str,
    ) -> Result<String, GitServiceError> {
        let repo = Repository::open(repo_path)?;
        let branch_ref = Self::find_branch(&repo, branch_name)?.into_reference();
        let default_remote = self.default_remote_name(&repo);
        self.get_remote_from_branch_ref(&repo, &branch_ref)
            .map(|r| r.name().unwrap_or(&default_remote).to_string())
    }

    fn get_remote_from_branch_ref<'a>(
        &self,
        repo: &'a Repository,
        branch_ref: &Reference,
    ) -> Result<Remote<'a>, GitServiceError> {
        let branch_name = branch_ref
            .name()
            .map(|name| name.to_string())
            .ok_or_else(|| GitServiceError::InvalidRepository("Invalid branch ref".into()))?;
        let remote_name_buf = repo.branch_remote_name(&branch_name)?;

        let remote_name = str::from_utf8(&remote_name_buf)
            .map_err(|e| {
                GitServiceError::InvalidRepository(format!(
                    "Invalid remote name for branch {branch_name}: {e}"
                ))
            })?
            .to_string();
        repo.find_remote(&remote_name).map_err(|_| {
            GitServiceError::InvalidRepository(format!(
                "Remote '{remote_name}' for branch '{branch_name}' not found"
            ))
        })
    }

    pub fn push_to_github(
        &self,
        worktree_path: &Path,
        branch_name: &str,
        force: bool,
    ) -> Result<(), GitServiceError> {
        self.check_worktree_clean(worktree_path)?;
        let repo = Repository::open(worktree_path)?;

        // Get the remote
        let remote_name = self.default_remote_name(&repo);
        let remote = repo.find_remote(&remote_name)?;

        let remote_url = remote
            .url()
            .ok_or_else(|| GitServiceError::InvalidRepository("Remote has no URL".to_string()))?;
        let git_cli = GitCli::new();
        if let Err(e) = git_cli.push(worktree_path, remote_url, branch_name, force) {
            tracing::error!("Push to GitHub failed: {}", e);
            return Err(e.into());
        }

        let mut branch = Self::find_branch(&repo, branch_name)?;
        if !branch.get().is_remote() {
            if let Some(branch_target) = branch.get().target() {
                let remote_ref = format!("refs/remotes/{remote_name}/{branch_name}");
                repo.reference(
                    &remote_ref,
                    branch_target,
                    true,
                    "update remote tracking branch",
                )?;
            }
            branch.set_upstream(Some(&format!("{remote_name}/{branch_name}")))?;
        }

        Ok(())
    }

    /// Fetch from remote repository using gix (gitoxide) for performance.
    ///
    /// This uses gix's native fetch implementation which handles authentication
    /// via git credential helpers (SSH keys, credential managers, etc.).
    /// Falls back to git CLI if gix fetch fails.
    fn fetch_from_remote(
        &self,
        repo: &Repository,
        remote: &Remote,
        refspec: &str,
    ) -> Result<(), GitServiceError> {
        let remote_name = remote.name();
        let remote_url = remote
            .url()
            .ok_or_else(|| GitServiceError::InvalidRepository("Remote has no URL".to_string()))?;

        // Get repo workdir path for gix
        let repo_path = repo
            .workdir()
            .ok_or_else(|| GitServiceError::InvalidRepository("Bare repository".to_string()))?;

        // Try gix fetch first (faster, 2-5x improvement)
        let gix_repo = match GixReader::open(repo_path) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("Failed to open repo with gix, falling back to CLI: {}", e);
                return self.fetch_from_remote_cli(repo_path, remote_url, refspec);
            }
        };

        // Use remote name if available, otherwise use URL directly
        let fetch_result = if let Some(name) = remote_name {
            GixReader::fetch(&gix_repo, name, refspec)
        } else {
            GixReader::fetch_url(&gix_repo, remote_url, refspec)
        };

        match fetch_result {
            Ok(()) => {
                tracing::debug!("Fetch completed via gix");
                Ok(())
            }
            Err(e) => {
                // Fall back to CLI for complex auth scenarios (e.g., interactive prompts)
                tracing::warn!("gix fetch failed, falling back to CLI: {}", e);
                self.fetch_from_remote_cli(repo_path, remote_url, refspec)
            }
        }
    }

    /// Fallback fetch using git CLI for complex auth scenarios.
    fn fetch_from_remote_cli(
        &self,
        repo_path: &Path,
        remote_url: &str,
        refspec: &str,
    ) -> Result<(), GitServiceError> {
        let git_cli = GitCli::new();
        if let Err(e) = git_cli.fetch_with_refspec(repo_path, remote_url, refspec) {
            tracing::error!("Fetch from remote failed: {}", e);
            return Err(e.into());
        }
        Ok(())
    }

    /// Fetch from remote repository using native git authentication
    fn fetch_branch_from_remote(
        &self,
        repo: &Repository,
        branch: &Reference,
    ) -> Result<(), GitServiceError> {
        let remote = self.get_remote_from_branch_ref(repo, branch)?;
        let default_remote_name = self.default_remote_name(repo);
        let remote_name = remote.name().unwrap_or(&default_remote_name);
        let dest_ref = branch
            .name()
            .ok_or_else(|| GitServiceError::InvalidRepository("Invalid branch ref".into()))?;
        let remote_prefix = format!("refs/remotes/{remote_name}/");
        let src_ref = dest_ref.replacen(&remote_prefix, "refs/heads/", 1);
        let refspec = format!("+{src_ref}:{dest_ref}");
        self.fetch_from_remote(repo, &remote, &refspec)
    }

    /// Fetch from remote repository using native git authentication
    fn fetch_all_from_remote(
        &self,
        repo: &Repository,
        remote: &Remote,
    ) -> Result<(), GitServiceError> {
        let default_remote_name = self.default_remote_name(repo);
        let remote_name = remote.name().unwrap_or(&default_remote_name);
        let refspec = format!("+refs/heads/*:refs/remotes/{remote_name}/*");
        self.fetch_from_remote(repo, remote, &refspec)
    }

    /// Clone a repository to the specified directory
    #[cfg(feature = "cloud")]
    pub fn clone_repository(
        clone_url: &str,
        target_path: &Path,
        token: Option<&str>,
    ) -> Result<Repository, GitServiceError> {
        use git2::{Cred, FetchOptions, RemoteCallbacks};

        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Set up callbacks for authentication if token is provided
        let mut callbacks = RemoteCallbacks::new();
        if let Some(token) = token {
            callbacks.credentials(|_url, username_from_url, _allowed_types| {
                Cred::userpass_plaintext(username_from_url.unwrap_or("git"), token)
            });
        } else {
            // Fallback to SSH agent and key file authentication
            callbacks.credentials(|_url, username_from_url, _| {
                // Try SSH agent first
                if let Some(username) = username_from_url
                    && let Ok(cred) = Cred::ssh_key_from_agent(username)
                {
                    return Ok(cred);
                }

                // Fallback to key file (~/.ssh/id_rsa)
                let home = dirs::home_dir()
                    .ok_or_else(|| git2::Error::from_str("Could not find home directory"))?;
                let key_path = home.join(".ssh").join("id_rsa");
                Cred::ssh_key(username_from_url.unwrap_or("git"), None, &key_path, None)
            });
        }

        // Set up fetch options with our callbacks
        let mut fetch_opts = FetchOptions::new();
        fetch_opts.remote_callbacks(callbacks);

        // Create a repository builder with fetch options
        let mut builder = git2::build::RepoBuilder::new();
        builder.fetch_options(fetch_opts);

        let repo = builder.clone(clone_url, target_path)?;

        tracing::info!(
            "Successfully cloned repository from {} to {}",
            clone_url,
            target_path.display()
        );

        Ok(repo)
    }

    /// Check if a branch is an ancestor of HEAD.
    /// Returns Ok(true) if the branch is an ancestor, Ok(false) if not.
    /// Returns an error if the branch doesn't exist or other git errors occur.
    pub fn is_branch_ancestor_of_head(
        &self,
        repo_path: &Path,
        branch_name: &str,
    ) -> Result<bool, GitServiceError> {
        // Get branch OID using git2 (for branch lookup)
        let repo = self.open_repo(repo_path)?;
        let head_oid = repo.head()?.peel_to_commit()?.id().to_string();
        let branch = Self::find_branch(&repo, branch_name)?;
        let branch_oid = branch.get().peel_to_commit()?.id().to_string();

        // Use gix for ancestry check
        let gix_repo = GixReader::open(repo_path)?;
        let is_ancestor = GixReader::is_ancestor_by_oid(&gix_repo, &branch_oid, &head_oid)?;
        Ok(is_ancestor)
    }

    /// Check if a branch has been merged into a target branch.
    ///
    /// Returns:
    /// - `Ok(None)` if the source branch doesn't exist
    /// - `Ok(Some(true))` if the source branch is merged into the target
    /// - `Ok(Some(false))` if the source branch exists but is not merged
    /// - `Err(BranchNotFound)` if the target branch doesn't exist
    pub fn is_branch_merged_into(
        &self,
        repo_path: &Path,
        branch_name: &str,
        target_branch: &str,
    ) -> Result<Option<bool>, GitServiceError> {
        let repo = self.open_repo(repo_path)?;

        // Check if source branch exists - return None if not found
        let source_branch = match Self::find_branch(&repo, branch_name) {
            Ok(branch) => branch,
            Err(GitServiceError::BranchNotFound(_)) => return Ok(None),
            Err(e) => return Err(e),
        };

        // Check if target branch exists - return error if not found
        let target_branch_ref = Self::find_branch(&repo, target_branch)?;

        // Get OIDs for both branches
        let source_oid = source_branch.get().peel_to_commit()?.id().to_string();
        let target_oid = target_branch_ref.get().peel_to_commit()?.id().to_string();

        // Use gix to check if source is ancestor of target (i.e., merged into target)
        let gix_repo = GixReader::open(repo_path)?;
        let is_merged = GixReader::is_ancestor_by_oid(&gix_repo, &source_oid, &target_oid)?;
        Ok(Some(is_merged))
    }

    /// Collect file statistics from recent commits for ranking purposes
    pub fn collect_recent_file_stats(
        &self,
        repo_path: &Path,
        commit_limit: usize,
    ) -> Result<HashMap<String, FileStat>, GitServiceError> {
        let mut gix_repo = GixReader::open(repo_path)?;
        GixReader::recent_file_stats(&mut gix_repo, commit_limit).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, process::Command};

    use tempfile::TempDir;

    use super::*;

    fn init_test_repo_via_cli(dir: &Path) {
        Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(dir)
            .output()
            .expect("Failed to init repo");

        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir)
            .output()
            .expect("Failed to set email");

        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .output()
            .expect("Failed to set name");

        // Create empty initial commit
        Command::new("git")
            .args(["commit", "--allow-empty", "-m", "Initial commit"])
            .current_dir(dir)
            .output()
            .expect("Failed to create initial commit");
    }

    fn git_rev_parse(repo_path: &Path, rev: &str) -> String {
        let output = Command::new("git")
            .args(["rev-parse", rev])
            .current_dir(repo_path)
            .output()
            .expect("Failed to run git rev-parse");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn git_rev_list_count(repo_path: &Path, range: &str) -> usize {
        let output = Command::new("git")
            .args(["rev-list", "--count", range])
            .current_dir(repo_path)
            .output()
            .expect("Failed to run git rev-list");
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .unwrap_or(0)
    }

    fn git_status_counts(repo_path: &Path) -> (usize, usize) {
        let output = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(repo_path)
            .output()
            .expect("Failed to run git status");
        let status_output = String::from_utf8_lossy(&output.stdout);

        let mut uncommitted = 0;
        let mut untracked = 0;

        for line in status_output.lines() {
            if line.starts_with("??") {
                untracked += 1;
            } else if !line.is_empty() {
                uncommitted += 1;
            }
        }

        (uncommitted, untracked)
    }

    /// Integration test: verify GitService.get_head_info() matches git CLI
    #[test]
    fn test_git_service_get_head_info() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();
        init_test_repo_via_cli(repo_path);

        let git_service = GitService::new();
        let head_info = git_service.get_head_info(repo_path).unwrap();

        let git_oid = git_rev_parse(repo_path, "HEAD");
        let git_branch = {
            let output = Command::new("git")
                .args(["branch", "--show-current"])
                .current_dir(repo_path)
                .output()
                .expect("Failed to get branch");
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        };

        assert_eq!(head_info.oid, git_oid, "HEAD OID should match git CLI");
        assert_eq!(
            head_info.branch, git_branch,
            "Branch name should match git CLI"
        );
    }

    /// Integration test: verify GitService.get_worktree_change_counts() matches git CLI
    #[test]
    fn test_git_service_worktree_change_counts() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();
        init_test_repo_via_cli(repo_path);

        // Create a tracked file and commit it
        fs::write(repo_path.join("tracked.txt"), "initial").unwrap();
        Command::new("git")
            .args(["add", "tracked.txt"])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Add tracked file"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Create mixed state: modified + staged + untracked
        fs::write(repo_path.join("tracked.txt"), "modified").unwrap();
        fs::write(repo_path.join("untracked.txt"), "untracked").unwrap();
        fs::write(repo_path.join("staged.txt"), "staged").unwrap();
        Command::new("git")
            .args(["add", "staged.txt"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let git_service = GitService::new();
        let (uncommitted, untracked) = git_service.get_worktree_change_counts(repo_path).unwrap();
        let (git_uncommitted, git_untracked) = git_status_counts(repo_path);

        assert_eq!(
            uncommitted, git_uncommitted,
            "Uncommitted count should match git CLI"
        );
        assert_eq!(
            untracked, git_untracked,
            "Untracked count should match git CLI"
        );
    }

    /// Integration test: verify GitService.get_branch_status() matches git CLI
    #[test]
    fn test_git_service_branch_status() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();
        init_test_repo_via_cli(repo_path);

        // Create a feature branch with some commits
        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(repo_path)
            .output()
            .expect("Failed to create feature branch");

        for i in 1..=3 {
            fs::write(
                repo_path.join(format!("feature{}.txt", i)),
                format!("content{}", i),
            )
            .unwrap();
            Command::new("git")
                .args(["add", "."])
                .current_dir(repo_path)
                .output()
                .unwrap();
            Command::new("git")
                .args(["commit", "-m", &format!("Feature commit {}", i)])
                .current_dir(repo_path)
                .output()
                .unwrap();
        }

        // Switch to main and add commits
        Command::new("git")
            .args(["checkout", "main"])
            .current_dir(repo_path)
            .output()
            .expect("Failed to checkout main");

        for i in 1..=2 {
            fs::write(repo_path.join(format!("main{}.txt", i)), format!("main{}", i)).unwrap();
            Command::new("git")
                .args(["add", "."])
                .current_dir(repo_path)
                .output()
                .unwrap();
            Command::new("git")
                .args(["commit", "-m", &format!("Main commit {}", i)])
                .current_dir(repo_path)
                .output()
                .unwrap();
        }

        // Get ahead/behind using GitService
        let git_service = GitService::new();
        let (ahead, behind) = git_service
            .get_branch_status(repo_path, "feature", "main")
            .unwrap();

        // Get expected values from git CLI
        let feature_oid = git_rev_parse(repo_path, "feature");
        let main_oid = git_rev_parse(repo_path, "main");
        let expected_ahead =
            git_rev_list_count(repo_path, &format!("{}..{}", main_oid, feature_oid));
        let expected_behind =
            git_rev_list_count(repo_path, &format!("{}..{}", feature_oid, main_oid));

        assert_eq!(
            ahead, expected_ahead,
            "Ahead count should match git CLI (expected 3)"
        );
        assert_eq!(
            behind, expected_behind,
            "Behind count should match git CLI (expected 2)"
        );
    }

    /// Integration test: verify GitService works with worktrees
    #[test]
    fn test_git_service_with_worktree() {
        let temp_dir = TempDir::new().unwrap();
        let main_repo_path = temp_dir.path().join("main");
        fs::create_dir_all(&main_repo_path).unwrap();
        init_test_repo_via_cli(&main_repo_path);

        // Create a worktree
        let worktree_path = temp_dir.path().join("worktree");
        Command::new("git")
            .args([
                "worktree",
                "add",
                "-b",
                "feature",
                worktree_path.to_str().unwrap(),
            ])
            .current_dir(&main_repo_path)
            .output()
            .expect("Failed to create worktree");

        // Add files in worktree
        fs::write(worktree_path.join("new-file.txt"), "content").unwrap();

        let git_service = GitService::new();

        // Test get_head_info in worktree
        let head_info = git_service.get_head_info(&worktree_path).unwrap();
        assert_eq!(
            head_info.branch, "feature",
            "Worktree should be on feature branch"
        );

        // Test get_worktree_change_counts in worktree
        let (uncommitted, untracked) = git_service
            .get_worktree_change_counts(&worktree_path)
            .unwrap();
        let (git_uncommitted, git_untracked) = git_status_counts(&worktree_path);
        assert_eq!(uncommitted, git_uncommitted);
        assert_eq!(untracked, git_untracked);
    }

    /// Test is_branch_merged_into with a merged branch
    #[test]
    fn test_is_branch_merged_into_merged() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();
        init_test_repo_via_cli(repo_path);

        // Create a feature branch
        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(repo_path)
            .output()
            .expect("Failed to create feature branch");

        // Add a commit on feature
        fs::write(repo_path.join("feature.txt"), "feature content").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Feature commit"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Go back to main and merge feature
        Command::new("git")
            .args(["checkout", "main"])
            .current_dir(repo_path)
            .output()
            .expect("Failed to checkout main");

        Command::new("git")
            .args(["merge", "feature", "--no-ff", "-m", "Merge feature"])
            .current_dir(repo_path)
            .output()
            .expect("Failed to merge feature");

        let git_service = GitService::new();
        let result = git_service
            .is_branch_merged_into(repo_path, "feature", "main")
            .unwrap();

        assert_eq!(
            result,
            Some(true),
            "Feature branch should be merged into main"
        );
    }

    /// Test is_branch_merged_into with a non-merged branch
    #[test]
    fn test_is_branch_merged_into_not_merged() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();
        init_test_repo_via_cli(repo_path);

        // Create a feature branch with a commit
        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(repo_path)
            .output()
            .expect("Failed to create feature branch");

        fs::write(repo_path.join("feature.txt"), "feature content").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Feature commit"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Go back to main (don't merge)
        Command::new("git")
            .args(["checkout", "main"])
            .current_dir(repo_path)
            .output()
            .expect("Failed to checkout main");

        let git_service = GitService::new();
        let result = git_service
            .is_branch_merged_into(repo_path, "feature", "main")
            .unwrap();

        assert_eq!(
            result,
            Some(false),
            "Feature branch should NOT be merged into main"
        );
    }

    /// Test is_branch_merged_into with a non-existent source branch
    #[test]
    fn test_is_branch_merged_into_nonexistent_source() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();
        init_test_repo_via_cli(repo_path);

        let git_service = GitService::new();
        let result = git_service
            .is_branch_merged_into(repo_path, "nonexistent-branch", "main")
            .unwrap();

        assert_eq!(
            result, None,
            "Should return None for non-existent source branch"
        );
    }

    /// Test is_branch_merged_into with a non-existent target branch
    #[test]
    fn test_is_branch_merged_into_nonexistent_target() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();
        init_test_repo_via_cli(repo_path);

        let git_service = GitService::new();
        let result = git_service.is_branch_merged_into(repo_path, "main", "nonexistent-target");

        assert!(
            matches!(result, Err(GitServiceError::BranchNotFound(_))),
            "Should return BranchNotFound error for non-existent target branch"
        );
    }
}
