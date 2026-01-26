//! Why we prefer the Git CLI here
//!
//! - Safer working-tree semantics: the `git` CLI refuses to clobber uncommitted
//!   tracked changes and untracked files during checkout/merge/rebase unless you
//!   explicitly force it. libgit2 does not enforce those protections by default,
//!   which means callers must re‑implement a lot of safety checks to avoid data loss.
//! - Sparse‑checkout correctness: the CLI natively respects sparse‑checkout.
//!   libgit2 does not yet support sparse‑checkout semantics the same way, which
//!   led to incorrect diffs and staging in our workflows.
//! - Cross‑platform stability: we observed libgit2 corrupt repositories shared
//!   between WSL and Windows in scenarios where the `git` CLI did not. Delegating
//!   working‑tree mutations to the CLI has proven more reliable in practice.
//!
//! Given these reasons, this module centralizes destructive or working‑tree‑
//! touching operations (rebase, merge, checkout, add/commit, etc.) through the
//! `git` CLI, while keeping libgit2 for read‑only graph queries and credentialed
//! network operations when useful.
use std::{
    ffi::{OsStr, OsString},
    io::Write as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use thiserror::Error;
use utils::shell::resolve_executable_path_blocking; // TODO: make GitCli async

use crate::services::{filesystem_watcher::ALWAYS_SKIP_DIRS, git::Commit};

#[derive(Debug, Error)]
pub enum GitCliError {
    #[error("git executable not found or not runnable")]
    NotAvailable,
    #[error("git command failed: {0}")]
    CommandFailed(String),
    #[error("authentication failed: {0}")]
    AuthFailed(String),
    #[error("push rejected: {0}")]
    PushRejected(String),
    #[error("rebase in progress in this worktree")]
    RebaseInProgress,
    #[error("branch or reference not found: {0}")]
    InvalidReference(String),
}

#[derive(Clone, Default)]
pub struct GitCli;

/// Parsed change type from `git diff --name-status` output
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Unmerged,
    Unknown(String),
}

/// One entry from a status diff (name-status + paths)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusDiffEntry {
    pub change: ChangeType,
    pub path: String,
    pub old_path: Option<String>,
}

/// Parsed worktree entry from `git worktree list --porcelain`
#[derive(Debug, Clone)]
pub struct WorktreeEntry {
    pub path: PathBuf,
    pub branch: Option<String>,
    /// True if this is the main repository (not a linked worktree)
    pub is_main: bool,
}

#[derive(Debug, Clone, Default)]
pub struct StatusDiffOptions {
    pub path_filter: Option<Vec<String>>, // pathspecs to limit diff
}

impl GitCli {
    pub fn new() -> Self {
        Self {}
    }
    /// Run `git -C <repo> worktree add <path> <branch>` (optionally creating the branch with -b)
    pub fn worktree_add(
        &self,
        repo_path: &Path,
        worktree_path: &Path,
        branch: &str,
        create_branch: bool,
    ) -> Result<(), GitCliError> {
        self.ensure_available()?;

        let mut args: Vec<OsString> = vec!["worktree".into(), "add".into()];
        if create_branch {
            args.push("-b".into());
            args.push(OsString::from(branch));
        }
        args.push(worktree_path.as_os_str().into());
        args.push(OsString::from(branch));
        self.git(repo_path, args)?;

        // Good practice: reapply sparse-checkout in the new worktree to ensure materialization matches
        // Non-fatal if it fails or not configured.
        let _ = self.git(worktree_path, ["sparse-checkout", "reapply"]);

        Ok(())
    }

    /// Run `git -C <repo> worktree remove <path>`
    pub fn worktree_remove(
        &self,
        repo_path: &Path,
        worktree_path: &Path,
        force: bool,
    ) -> Result<(), GitCliError> {
        self.ensure_available()?;
        let mut args: Vec<OsString> = vec!["worktree".into(), "remove".into()];
        if force {
            args.push("--force".into());
        }
        args.push(worktree_path.as_os_str().into());
        self.git(repo_path, args)?;
        Ok(())
    }

    /// Run `git -C <repo> worktree move <old_path> <new_path>`
    pub fn worktree_move(
        &self,
        repo_path: &Path,
        old_path: &Path,
        new_path: &Path,
    ) -> Result<(), GitCliError> {
        self.ensure_available()?;
        self.git(
            repo_path,
            [
                "worktree",
                "move",
                old_path.to_str().ok_or_else(|| {
                    GitCliError::CommandFailed("Invalid old worktree path".to_string())
                })?,
                new_path.to_str().ok_or_else(|| {
                    GitCliError::CommandFailed("Invalid new worktree path".to_string())
                })?,
            ],
        )?;
        Ok(())
    }

    /// Prune stale worktree metadata
    pub fn worktree_prune(&self, repo_path: &Path) -> Result<(), GitCliError> {
        self.git(repo_path, ["worktree", "prune"])?;
        Ok(())
    }

    /// Return true if there are any changes in the working tree (staged or unstaged).
    pub fn has_changes(&self, worktree_path: &Path) -> Result<bool, GitCliError> {
        let out = self.git(
            worktree_path,
            ["--no-optional-locks", "status", "--porcelain"],
        )?;
        Ok(!out.is_empty())
    }

    /// Diff status vs a base branch using a temporary index (always includes untracked).
    /// Path filter limits the reported paths.
    pub fn diff_status(
        &self,
        worktree_path: &Path,
        base_commit: &Commit,
        opts: StatusDiffOptions,
    ) -> Result<Vec<StatusDiffEntry>, GitCliError> {
        // Create a temp index file
        let tmp_dir = tempfile::TempDir::new()
            .map_err(|e| GitCliError::CommandFailed(format!("temp dir create failed: {e}")))?;
        let tmp_index = tmp_dir.path().join("index");
        let envs = vec![(
            OsString::from("GIT_INDEX_FILE"),
            tmp_index.as_os_str().to_os_string(),
        )];

        // Use a temp index from HEAD to accurately track renames in untracked files
        let _ = self.git_with_env(worktree_path, ["read-tree", "HEAD"], &envs)?;

        // Stage changed and untracked files explicitly, which is faster than `git add -A` for large repos.
        // Use raw paths from `get_worktree_status` to avoid lossy UTF-8 conversions for odd filenames.
        let status = self.get_worktree_status(worktree_path)?;
        let mut paths_to_add: Vec<Vec<u8>> = Vec::new();
        for entry in status.entries {
            paths_to_add.push(entry.path);
            if let Some(orig) = entry.orig_path {
                paths_to_add.push(orig);
            }
        }
        if !paths_to_add.is_empty() {
            paths_to_add.extend(
                Self::get_default_pathspec_excludes()
                    .iter()
                    .map(|s| s.as_encoded_bytes().to_vec()),
            );
            let mut input = Vec::new();
            for p in paths_to_add {
                input.extend_from_slice(&p);
                input.push(0);
            }
            let args = vec![
                OsString::from("add"),
                OsString::from("-A"),
                OsString::from("--pathspec-from-file=-"),
                OsString::from("--pathspec-file-nul"),
            ];
            self.git_with_stdin(worktree_path, args, Some(&envs), &input)?;
        }
        // git diff --cached
        let mut args: Vec<OsString> = vec![
            "-c".into(),
            "core.quotepath=false".into(),
            "diff".into(),
            "--cached".into(),
            "-M".into(),
            "--name-status".into(),
            OsString::from(base_commit.to_string()),
        ];
        args = Self::apply_pathspec_filter(args, opts.path_filter.as_ref());
        let out = self.git_with_env(worktree_path, args, &envs)?;
        Ok(Self::parse_name_status(&out))
    }

    /// Return `git status --porcelain` parsed into a structured summary
    pub fn get_worktree_status(&self, worktree_path: &Path) -> Result<WorktreeStatus, GitCliError> {
        // Using -z for NUL-separated output which correctly handles paths with special chars.
        // Format: XY<space>PATH<NUL>[ORIGPATH<NUL>] where ORIGPATH only present for R/C.
        let args = Self::apply_default_excludes(vec![
            "--no-optional-locks",
            "status",
            "--porcelain",
            "-z",
            "--untracked-files=normal",
        ]);
        let out = self.git_impl(worktree_path, args, None, None)?;
        let mut entries = Vec::new();
        let mut uncommitted_tracked = 0usize;
        let mut untracked = 0usize;
        let mut parts = out.split(|b| *b == 0);
        while let Some(part) = parts.next() {
            if part.is_empty() || part.len() < 4 {
                continue;
            }
            let staged = part[0] as char;
            let unstaged = part[1] as char;
            let path = part[3..].to_vec();

            let mut orig_path = None;
            if (staged == 'R' || unstaged == 'R' || staged == 'C' || unstaged == 'C')
                && let Some(old_path) = parts.next()
                && !old_path.is_empty()
            {
                orig_path = Some(old_path.to_vec());
            }
            if staged == '?' && unstaged == '?' {
                untracked += 1;
                entries.push(StatusEntry {
                    staged,
                    unstaged,
                    path,
                    orig_path,
                    is_untracked: true,
                });
            } else {
                if staged != ' ' || unstaged != ' ' {
                    uncommitted_tracked += 1;
                }
                entries.push(StatusEntry {
                    staged,
                    unstaged,
                    path,
                    orig_path,
                    is_untracked: false,
                });
            }
        }
        Ok(WorktreeStatus {
            uncommitted_tracked,
            untracked,
            entries,
        })
    }

    /// Stage all changes in the working tree (respects sparse-checkout semantics).
    pub fn add_all(&self, worktree_path: &Path) -> Result<(), GitCliError> {
        self.git(
            worktree_path,
            Self::apply_default_excludes(vec!["add", "-A"]),
        )?;
        Ok(())
    }

    /// List all worktrees in the repository.
    ///
    /// The main worktree is identified as the first entry in the list that is not bare.
    /// Secondary worktrees (linked worktrees) appear after it.
    ///
    /// Worktrees in detached HEAD state will have `branch: None`.
    pub fn list_worktrees(&self, repo_path: &Path) -> Result<Vec<WorktreeEntry>, GitCliError> {
        let out = self.git(repo_path, ["worktree", "list", "--porcelain"])?;
        let mut entries = Vec::new();
        let mut current_path: Option<PathBuf> = None;
        let mut current_head: Option<String> = None;
        let mut current_branch: Option<String> = None;
        let mut is_bare = false;

        for line in out.lines() {
            let line = line.trim();

            if line.is_empty() {
                // End of current worktree entry, save it if we have required data
                // Skip bare repositories (they have no working tree)
                if let (Some(path), Some(_head)) = (current_path.take(), current_head.take()) {
                    if !is_bare {
                        // The first non-bare entry is the main worktree
                        let is_main = entries.is_empty();
                        entries.push(WorktreeEntry {
                            path,
                            branch: current_branch.take(),
                            is_main,
                        });
                    }
                }
                current_branch = None;
                is_bare = false;
            } else if let Some(path) = line.strip_prefix("worktree ") {
                current_path = Some(PathBuf::from(path));
            } else if let Some(head) = line.strip_prefix("HEAD ") {
                current_head = Some(head.to_string());
            } else if let Some(branch_ref) = line.strip_prefix("branch ") {
                // Extract branch name from refs/heads/branch-name
                current_branch = branch_ref
                    .strip_prefix("refs/heads/")
                    .map(|name| name.to_string());
            } else if line == "bare" {
                is_bare = true;
            }
            // Note: "detached" line is ignored; we simply have branch: None for detached HEAD
        }

        // Handle the last entry if no trailing empty line
        if let (Some(path), Some(_head)) = (current_path, current_head) {
            if !is_bare {
                let is_main = entries.is_empty();
                entries.push(WorktreeEntry {
                    path,
                    branch: current_branch,
                    is_main,
                });
            }
        }

        Ok(entries)
    }

    /// Discover all worktrees from any path within a git repository.
    ///
    /// This is useful when you're inside a worktree (or the main repo) and want to
    /// find all sibling worktrees. The function first resolves the main repository
    /// path using `git rev-parse --git-common-dir`, then lists all worktrees.
    ///
    /// Returns a list of worktrees with their paths, branches, and whether they're
    /// the main repository.
    pub fn discover_worktrees(&self, any_repo_path: &Path) -> Result<Vec<WorktreeEntry>, GitCliError> {
        // Get the common git directory (the main repo's .git directory)
        // This works from any worktree or the main repo
        let common_dir = self.git(any_repo_path, ["rev-parse", "--git-common-dir"])?;
        let common_dir = common_dir.trim();

        // The common dir is either:
        // - An absolute path to .git
        // - A relative path from any_repo_path to .git (usually just ".git")
        let git_dir = if Path::new(common_dir).is_absolute() {
            PathBuf::from(common_dir)
        } else {
            any_repo_path.join(common_dir)
        };

        // The main repo is the parent of the .git directory
        let main_repo = git_dir
            .parent()
            .ok_or_else(|| GitCliError::CommandFailed("Could not determine main repo path".into()))?;

        self.list_worktrees(main_repo)
    }

    /// Commit staged changes with the given message.
    pub fn commit(&self, worktree_path: &Path, message: &str) -> Result<(), GitCliError> {
        self.git(worktree_path, ["commit", "-m", message])?;
        Ok(())
    }
    /// Fetch a branch to the given remote using native git authentication.
    pub fn fetch_with_refspec(
        &self,
        repo_path: &Path,
        remote_url: &str,
        refspec: &str,
    ) -> Result<(), GitCliError> {
        let envs = vec![(OsString::from("GIT_TERMINAL_PROMPT"), OsString::from("0"))];

        let args = [
            OsString::from("fetch"),
            OsString::from(remote_url),
            OsString::from(refspec),
        ];

        match self.git_with_env(repo_path, args, &envs) {
            Ok(_) => Ok(()),
            Err(GitCliError::CommandFailed(msg)) => Err(self.classify_cli_error(msg)),
            Err(err) => Err(err),
        }
    }

    /// Push a branch to the given remote using native git authentication.
    pub fn push(
        &self,
        repo_path: &Path,
        remote_url: &str,
        branch: &str,
        force: bool,
    ) -> Result<(), GitCliError> {
        let refspec = if force {
            format!("+refs/heads/{branch}:refs/heads/{branch}")
        } else {
            format!("refs/heads/{branch}:refs/heads/{branch}")
        };
        let envs = vec![(OsString::from("GIT_TERMINAL_PROMPT"), OsString::from("0"))];

        let args = [
            OsString::from("push"),
            OsString::from(remote_url),
            OsString::from(refspec),
        ];

        match self.git_with_env(repo_path, args, &envs) {
            Ok(_) => Ok(()),
            Err(GitCliError::CommandFailed(msg)) => Err(self.classify_cli_error(msg)),
            Err(err) => Err(err),
        }
    }

    /// This directly queries the remote without fetching.
    pub fn check_remote_branch_exists(
        &self,
        repo_path: &Path,
        remote_url: &str,
        branch_name: &str,
    ) -> Result<bool, GitCliError> {
        let envs = vec![(OsString::from("GIT_TERMINAL_PROMPT"), OsString::from("0"))];

        let args = [
            OsString::from("ls-remote"),
            OsString::from("--heads"),
            OsString::from(remote_url),
            OsString::from(format!("refs/heads/{branch_name}")),
        ];

        match self.git_with_env(repo_path, args, &envs) {
            Ok(output) => Ok(!output.trim().is_empty()),
            Err(GitCliError::CommandFailed(msg)) => Err(self.classify_cli_error(msg)),
            Err(err) => Err(err),
        }
    }

    // Parse `git diff --name-status` output into structured entries.
    // Handles rename/copy scores like `R100` by matching the first letter.
    fn parse_name_status(output: &str) -> Vec<StatusDiffEntry> {
        let mut out = Vec::new();
        for line in output.lines() {
            let line = line.trim_end();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split('\t');
            let code = parts.next().unwrap_or("");
            let change = match code.chars().next().unwrap_or('?') {
                'A' => ChangeType::Added,
                'M' => ChangeType::Modified,
                'D' => ChangeType::Deleted,
                'R' => ChangeType::Renamed,
                'C' => ChangeType::Copied,
                'T' => ChangeType::TypeChanged,
                'U' => ChangeType::Unmerged,
                other => ChangeType::Unknown(other.to_string()),
            };

            match change {
                ChangeType::Renamed | ChangeType::Copied => {
                    if let (Some(old), Some(newp)) = (parts.next(), parts.next()) {
                        out.push(StatusDiffEntry {
                            change,
                            path: newp.to_string(),
                            old_path: Some(old.to_string()),
                        });
                    }
                }
                _ => {
                    if let Some(p) = parts.next() {
                        out.push(StatusDiffEntry {
                            change,
                            path: p.to_string(),
                            old_path: None,
                        });
                    }
                }
            }
        }
        out
    }

    /// Return the merge base commit sha of two refs in the given worktree.
    /// If `git merge-base --fork-point` fails, falls back to regular `merge-base`.
    fn merge_base(&self, worktree_path: &Path, a: &str, b: &str) -> Result<String, GitCliError> {
        let out = self
            .git(worktree_path, ["merge-base", "--fork-point", a, b])
            .unwrap_or(self.git(worktree_path, ["merge-base", a, b])?);
        Ok(out.trim().to_string())
    }

    /// Perform `git rebase --onto <new_base> <old_base>` on <task_branch> in `worktree_path`.
    pub fn rebase_onto(
        &self,
        worktree_path: &Path,
        new_base: &str,
        old_base: &str,
        task_branch: &str,
    ) -> Result<(), GitCliError> {
        // If a rebase is in progress, refuse to proceed. The caller can
        // choose to abort or continue; we avoid destructive actions here.
        if self.is_rebase_in_progress(worktree_path).unwrap_or(false) {
            return Err(GitCliError::RebaseInProgress);
        }
        // compute the merge base of task_branch from old_base
        let merge_base = self
            .merge_base(worktree_path, old_base, task_branch)
            .unwrap_or(old_base.to_string());

        self.git(
            worktree_path,
            ["rebase", "--onto", new_base, &merge_base, task_branch],
        )?;
        Ok(())
    }

    /// Return true if there is a rebase in progress in this worktree.
    /// We treat this as true when either of Git's rebase state directories exists:
    /// - rebase-merge (interactive rebase)
    /// - rebase-apply (am-based rebase)
    pub fn is_rebase_in_progress(&self, worktree_path: &Path) -> Result<bool, GitCliError> {
        let rebase_merge = self.git(worktree_path, ["rev-parse", "--git-path", "rebase-merge"])?;
        let rebase_apply = self.git(worktree_path, ["rev-parse", "--git-path", "rebase-apply"])?;
        let rm_exists = std::path::Path::new(rebase_merge.trim()).exists();
        let ra_exists = std::path::Path::new(rebase_apply.trim()).exists();
        Ok(rm_exists || ra_exists)
    }

    /// Return true if a merge is in progress (MERGE_HEAD exists).
    pub fn is_merge_in_progress(&self, worktree_path: &Path) -> Result<bool, GitCliError> {
        match self.git(worktree_path, ["rev-parse", "--verify", "MERGE_HEAD"]) {
            Ok(_) => Ok(true),
            Err(GitCliError::CommandFailed(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Return true if a cherry-pick is in progress (CHERRY_PICK_HEAD exists).
    pub fn is_cherry_pick_in_progress(&self, worktree_path: &Path) -> Result<bool, GitCliError> {
        match self.git(worktree_path, ["rev-parse", "--verify", "CHERRY_PICK_HEAD"]) {
            Ok(_) => Ok(true),
            Err(GitCliError::CommandFailed(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Return true if a revert is in progress (REVERT_HEAD exists).
    pub fn is_revert_in_progress(&self, worktree_path: &Path) -> Result<bool, GitCliError> {
        match self.git(worktree_path, ["rev-parse", "--verify", "REVERT_HEAD"]) {
            Ok(_) => Ok(true),
            Err(GitCliError::CommandFailed(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Abort an in-progress rebase in this worktree. If no rebase is in progress,
    /// this is a no-op and returns Ok(()).
    pub fn abort_rebase(&self, worktree_path: &Path) -> Result<(), GitCliError> {
        // If nothing to abort, return success
        if !self.is_rebase_in_progress(worktree_path)? {
            return Ok(());
        }
        // Best-effort: if `git rebase --abort` fails, surface the error message
        self.git(worktree_path, ["rebase", "--abort"]).map(|_| ())
    }

    /// Quit an in-progress rebase (cleanup metadata without modifying commits).
    /// If no rebase is in progress, it's a no-op.
    pub fn quit_rebase(&self, worktree_path: &Path) -> Result<(), GitCliError> {
        if !self.is_rebase_in_progress(worktree_path)? {
            return Ok(());
        }
        self.git(worktree_path, ["rebase", "--quit"]).map(|_| ())
    }

    /// Return true if there are staged changes (index differs from HEAD)
    pub fn has_staged_changes(&self, repo_path: &Path) -> Result<bool, GitCliError> {
        // `git diff --cached --quiet` returns exit code 1 if there are differences
        let out =
            Command::new(resolve_executable_path_blocking("git").ok_or(GitCliError::NotAvailable)?)
                .arg("-C")
                .arg(repo_path)
                .arg("diff")
                .arg("--cached")
                .arg("--quiet")
                .output()
                .map_err(|e| GitCliError::CommandFailed(e.to_string()))?;
        match out.status.code() {
            Some(0) => Ok(false),
            Some(1) => Ok(true),
            _ => Err(GitCliError::CommandFailed(
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            )),
        }
    }

    /// Checkout base branch, squash-merge from_branch, and commit with message. Returns new HEAD sha.
    pub fn merge_squash_commit(
        &self,
        repo_path: &Path,
        base_branch: &str,
        from_branch: &str,
        message: &str,
    ) -> Result<String, GitCliError> {
        self.git(repo_path, ["checkout", base_branch]).map(|_| ())?;
        self.git(repo_path, ["merge", "--squash", "--no-commit", from_branch])
            .map(|_| ())?;
        self.git(repo_path, ["commit", "-m", message]).map(|_| ())?;
        let sha = self
            .git(repo_path, ["rev-parse", "HEAD"])?
            .trim()
            .to_string();
        Ok(sha)
    }

    /// Update a ref to a specific sha in the repo.
    pub fn update_ref(
        &self,
        repo_path: &Path,
        refname: &str,
        sha: &str,
    ) -> Result<(), GitCliError> {
        self.git(repo_path, ["update-ref", refname, sha])
            .map(|_| ())
    }

    pub fn abort_merge(&self, worktree_path: &Path) -> Result<(), GitCliError> {
        if !self.is_merge_in_progress(worktree_path)? {
            return Ok(());
        }
        self.git(worktree_path, ["merge", "--abort"]).map(|_| ())
    }

    pub fn abort_cherry_pick(&self, worktree_path: &Path) -> Result<(), GitCliError> {
        if !self.is_cherry_pick_in_progress(worktree_path)? {
            return Ok(());
        }
        self.git(worktree_path, ["cherry-pick", "--abort"])
            .map(|_| ())
    }

    pub fn abort_revert(&self, worktree_path: &Path) -> Result<(), GitCliError> {
        if !self.is_revert_in_progress(worktree_path)? {
            return Ok(());
        }
        self.git(worktree_path, ["revert", "--abort"]).map(|_| ())
    }

    /// List files currently in a conflicted (unmerged) state in the worktree.
    pub fn get_conflicted_files(&self, worktree_path: &Path) -> Result<Vec<String>, GitCliError> {
        // `--diff-filter=U` lists paths with unresolved conflicts
        let out = self.git(worktree_path, ["diff", "--name-only", "--diff-filter=U"])?;
        let mut files = Vec::new();
        for line in out.lines() {
            let p = line.trim();
            if !p.is_empty() {
                files.push(p.to_string());
            }
        }
        Ok(files)
    }

    /// Create a new branch pointing to HEAD or a base branch.
    pub fn create_branch(
        &self,
        repo_path: &Path,
        branch_name: &str,
        base_branch: Option<&str>,
    ) -> Result<(), GitCliError> {
        self.ensure_available()?;
        if let Some(base) = base_branch {
            self.git(repo_path, ["branch", branch_name, base])?;
        } else {
            self.git(repo_path, ["branch", branch_name])?;
        }
        Ok(())
    }

    /// Check if a branch name is valid according to git-check-ref-format rules.
    pub fn is_branch_name_valid(&self, name: &str) -> bool {
        git2::Branch::name_is_valid(name).unwrap_or(false)
    }

    /// Clone a repository from a URL to a destination directory.
    pub fn clone(&self, url: &str, destination: &Path) -> Result<(), GitCliError> {
        self.ensure_available()?;
        let git = resolve_executable_path_blocking("git").ok_or(GitCliError::NotAvailable)?;

        let mut cmd = Command::new(&git);
        cmd.arg("clone")
            .arg(url)
            .arg(destination)
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        tracing::trace!("Running git clone: {:?}", cmd);

        let out = cmd
            .output()
            .map_err(|e| GitCliError::CommandFailed(e.to_string()))?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let combined = match (stdout.is_empty(), stderr.is_empty()) {
                (true, true) => "Command failed with no output".to_string(),
                (false, false) => format!("--- stderr\n{stderr}\n--- stdout\n{stdout}"),
                (false, true) => format!("--- stderr\n{stdout}"),
                (true, false) => format!("--- stdout\n{stderr}"),
            };
            return Err(self.classify_cli_error(combined));
        }
        Ok(())
    }
}

// Private methods
impl GitCli {
    fn classify_cli_error(&self, msg: String) -> GitCliError {
        let lower = msg.to_ascii_lowercase();
        if lower.contains("authentication failed")
            || lower.contains("could not read username")
            || lower.contains("invalid username or password")
        {
            GitCliError::AuthFailed(msg)
        } else if lower.contains("non-fast-forward")
            || lower.contains("failed to push some refs")
            || lower.contains("fetch first")
            || lower.contains("updates were rejected because the tip")
        {
            GitCliError::PushRejected(msg)
        } else {
            GitCliError::CommandFailed(msg)
        }
    }

    /// Ensure `git` is available on PATH
    fn ensure_available(&self) -> Result<(), GitCliError> {
        let git = resolve_executable_path_blocking("git").ok_or(GitCliError::NotAvailable)?;
        let out = Command::new(&git)
            .arg("--version")
            .output()
            .map_err(|_| GitCliError::NotAvailable)?;
        if out.status.success() {
            Ok(())
        } else {
            Err(GitCliError::NotAvailable)
        }
    }

    /// Run `git -C <repo_path> <args...>` and return stdout bytes on success.
    /// Prefer adding specific helpers (e.g. `get_worktree_status`, `diff_status`)
    /// instead of calling this directly, so all parsing and command choices are
    /// centralized here. This makes it easier to change the underlying commands
    /// without adjusting callers. Use this low-level method directly only in
    /// tests or when no dedicated helper exists yet.
    ///
    /// About `OsStr`/`OsString` usage:
    /// - `Command` and `Path` operate on `OsStr` to support non‑UTF‑8 paths and
    ///   arguments across platforms. Using `String` would force lossy conversion
    ///   or partial failures. This API accepts anything that implements
    ///   `AsRef<OsStr>` so typical call sites can still pass `&str` literals or
    ///   owned `String`s without friction.
    fn git_impl<I, S>(
        &self,
        repo_path: &Path,
        args: I,
        envs: Option<&[(OsString, OsString)]>,
        stdin: Option<&[u8]>,
    ) -> Result<Vec<u8>, GitCliError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.ensure_available()?;
        let git = resolve_executable_path_blocking("git").ok_or(GitCliError::NotAvailable)?;
        let mut cmd = Command::new(&git);
        cmd.arg("-C").arg(repo_path);

        if let Some(envs) = envs {
            for (k, v) in envs {
                cmd.env(k, v);
            }
        }

        for a in args {
            cmd.arg(a);
        }

        if stdin.is_some() {
            cmd.stdin(Stdio::piped());
        } else {
            cmd.stdin(Stdio::null());
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        tracing::trace!(
            stdin = ?stdin.as_ref().map(|s| String::from_utf8_lossy(s)),
            repo = ?repo_path,
            "Running git command: {:?}",
            cmd
        );

        let mut child = cmd
            .spawn()
            .map_err(|e| GitCliError::CommandFailed(e.to_string()))?;

        let stdin_write_result = if let Some(input) = stdin
            && let Some(mut child_stdin) = child.stdin.take()
        {
            Some(child_stdin.write_all(input))
        } else {
            None
        };

        let out = child
            .wait_with_output()
            .map_err(|e| GitCliError::CommandFailed(e.to_string()))?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let combined = match (stdout.is_empty(), stderr.is_empty()) {
                (true, true) => "Command failed with no output".to_string(),
                (false, false) => format!("--- stderr\n{stderr}\n--- stdout\n{stdout}"),
                (false, true) => format!("--- stderr\n{stdout}"),
                (true, false) => format!("--- stdout\n{stderr}"),
            };

            // Detect specific error types for better handling upstream
            if combined.contains("invalid reference:")
                || combined.contains("not a valid ref")
                || combined.contains("unknown revision")
            {
                return Err(GitCliError::InvalidReference(combined));
            }

            return Err(GitCliError::CommandFailed(combined));
        }
        if let Some(Err(e)) = stdin_write_result {
            return Err(GitCliError::CommandFailed(format!(
                "failed to write to git stdin: {e}"
            )));
        }
        Ok(out.stdout)
    }

    pub fn git<I, S>(&self, repo_path: &Path, args: I) -> Result<String, GitCliError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let out = self.git_impl(repo_path, args, None, None)?;
        Ok(String::from_utf8_lossy(&out).to_string())
    }

    fn git_with_env<I, S>(
        &self,
        repo_path: &Path,
        args: I,
        envs: &[(OsString, OsString)],
    ) -> Result<String, GitCliError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let out = self.git_impl(repo_path, args, Some(envs), None)?;
        Ok(String::from_utf8_lossy(&out).to_string())
    }

    fn git_with_stdin<I, S>(
        &self,
        repo_path: &Path,
        args: I,
        envs: Option<&[(OsString, OsString)]>,
        stdin: &[u8],
    ) -> Result<String, GitCliError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let out = self.git_impl(repo_path, args, envs, Some(stdin))?;
        Ok(String::from_utf8_lossy(&out).to_string())
    }

    fn apply_default_excludes<I, S>(args: I) -> Vec<OsString>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        Self::apply_pathspec_filter(args, None)
    }

    fn apply_pathspec_filter<I, S>(args: I, pathspecs: Option<&Vec<String>>) -> Vec<OsString>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let filters = Self::build_pathspec_filter(pathspecs);
        let mut args = args
            .into_iter()
            .map(|s| s.as_ref().to_os_string())
            .collect::<Vec<_>>();
        if !filters.is_empty() {
            args.push("--".into());
            args.extend(filters);
        }
        args
    }

    fn build_pathspec_filter(pathspecs: Option<&Vec<String>>) -> Vec<OsString> {
        let mut filters = Vec::new();
        filters.extend(Self::get_default_pathspec_excludes());
        if let Some(pathspecs) = pathspecs {
            for p in pathspecs {
                if p.trim().is_empty() {
                    continue;
                }
                filters.push(OsString::from(p));
            }
        }
        filters
    }

    fn get_default_pathspec_excludes() -> Vec<OsString> {
        ALWAYS_SKIP_DIRS
            .iter()
            .map(|d| OsString::from(format!(":(glob,exclude)**/{d}/")))
            .collect()
    }
}
/// Parsed entry from `git status --porcelain`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEntry {
    /// Single-letter staged status (column X) or '?' for untracked
    pub staged: char,
    /// Single-letter unstaged status (column Y) or '?' for untracked
    pub unstaged: char,
    /// Current path (raw bytes to avoid lossy UTF-8 conversion)
    pub path: Vec<u8>,
    /// Original path (for renames), raw bytes
    pub orig_path: Option<Vec<u8>>,
    /// True if this entry is untracked ("??")
    pub is_untracked: bool,
}

/// Summary + entries for a working tree status
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeStatus {
    pub uncommitted_tracked: usize,
    pub untracked: usize,
    pub entries: Vec<StatusEntry>,
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

    #[test]
    fn test_list_worktrees_main_only() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();
        init_test_repo_via_cli(repo_path);

        let git_cli = GitCli::new();
        let worktrees = git_cli.list_worktrees(repo_path).unwrap();

        assert_eq!(worktrees.len(), 1, "Should have exactly one worktree");

        let main_wt = &worktrees[0];
        assert!(main_wt.is_main, "First worktree should be the main one");
        assert_eq!(main_wt.branch, Some("main".to_string()));
        assert_eq!(main_wt.path, repo_path);
    }

    #[test]
    fn test_list_worktrees_with_linked_worktree() {
        let temp_dir = TempDir::new().unwrap();
        let main_repo_path = temp_dir.path().join("main");
        fs::create_dir_all(&main_repo_path).unwrap();
        init_test_repo_via_cli(&main_repo_path);

        // Create a linked worktree with a new branch
        let worktree_path = temp_dir.path().join("feature-wt");
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

        let git_cli = GitCli::new();
        let worktrees = git_cli.list_worktrees(&main_repo_path).unwrap();

        assert_eq!(worktrees.len(), 2, "Should have main + 1 linked worktree");

        // First should be main
        let main_wt = &worktrees[0];
        assert!(main_wt.is_main, "First worktree should be the main one");
        assert_eq!(main_wt.branch, Some("main".to_string()));
        assert_eq!(main_wt.path, main_repo_path);

        // Second should be the linked worktree
        let linked_wt = &worktrees[1];
        assert!(!linked_wt.is_main, "Second worktree should not be main");
        assert_eq!(linked_wt.branch, Some("feature".to_string()));
        assert_eq!(linked_wt.path, worktree_path);
    }

    #[test]
    fn test_list_worktrees_detached_head() {
        let temp_dir = TempDir::new().unwrap();
        let main_repo_path = temp_dir.path().join("main");
        fs::create_dir_all(&main_repo_path).unwrap();
        init_test_repo_via_cli(&main_repo_path);

        // Create a linked worktree in detached HEAD state
        let worktree_path = temp_dir.path().join("detached-wt");
        Command::new("git")
            .args([
                "worktree",
                "add",
                "--detach",
                worktree_path.to_str().unwrap(),
                "HEAD",
            ])
            .current_dir(&main_repo_path)
            .output()
            .expect("Failed to create detached worktree");

        let git_cli = GitCli::new();
        let worktrees = git_cli.list_worktrees(&main_repo_path).unwrap();

        assert_eq!(worktrees.len(), 2, "Should have main + 1 detached worktree");

        // The detached worktree should have branch: None
        let detached_wt = &worktrees[1];
        assert!(!detached_wt.is_main);
        assert_eq!(
            detached_wt.branch, None,
            "Detached worktree should have no branch"
        );
        assert_eq!(detached_wt.path, worktree_path);
    }

    #[test]
    fn test_discover_worktrees_from_main() {
        let temp_dir = TempDir::new().unwrap();
        let main_repo_path = temp_dir.path().join("main");
        fs::create_dir_all(&main_repo_path).unwrap();
        init_test_repo_via_cli(&main_repo_path);

        // Create a linked worktree
        let worktree_path = temp_dir.path().join("feature-wt");
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

        let git_cli = GitCli::new();

        // Should discover both worktrees when called from main repo
        let worktrees = git_cli.discover_worktrees(&main_repo_path).unwrap();
        assert_eq!(worktrees.len(), 2);
        assert!(worktrees[0].is_main);
        assert!(!worktrees[1].is_main);
    }

    #[test]
    fn test_discover_worktrees_from_linked() {
        let temp_dir = TempDir::new().unwrap();
        let main_repo_path = temp_dir.path().join("main");
        fs::create_dir_all(&main_repo_path).unwrap();
        init_test_repo_via_cli(&main_repo_path);

        // Create a linked worktree
        let worktree_path = temp_dir.path().join("feature-wt");
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

        let git_cli = GitCli::new();

        // Should discover both worktrees when called FROM the linked worktree
        let worktrees = git_cli.discover_worktrees(&worktree_path).unwrap();
        assert_eq!(
            worktrees.len(),
            2,
            "Should find all worktrees from linked worktree"
        );
        assert!(worktrees[0].is_main, "First should be main");
        assert!(!worktrees[1].is_main, "Second should be linked");
    }
}
