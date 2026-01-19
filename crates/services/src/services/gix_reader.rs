//! Git repository operations using gix (gitoxide).
//!
//! This module encapsulates gix operations and handles both regular
//! repositories and worktrees transparently.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use gix::bstr::BStr;
use gix::remote::Direction;
use gix::status::index_worktree;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GixReaderError {
    #[error("Failed to open repository: {0}")]
    Open(#[from] gix::open::Error),
    #[error("Repository discovery failed: {0}")]
    Discover(#[from] gix::discover::Error),
    #[error("Invalid repository at path: {path}")]
    InvalidRepository { path: String },
    #[error("Reference not found: {0}")]
    ReferenceNotFound(String),
    #[error("HEAD has no target commit")]
    DetachedHeadNoTarget,
    #[error("Reference error: {0}")]
    Reference(#[from] gix::reference::find::existing::Error),
    #[error("Failed to peel reference: {0}")]
    PeelError(#[from] gix::reference::peel::Error),
    #[error("Failed to peel HEAD: {0}")]
    PeelHead(#[from] gix::head::peel::to_commit::Error),
    #[error("Merge base error: {0}")]
    MergeBase(#[from] gix::repository::merge_base::Error),
    #[error("Revision walk error: {0}")]
    RevWalk(#[from] gix::revision::walk::Error),
    #[error("Revision walk iteration error: {0}")]
    RevWalkIter(#[from] gix::revision::walk::iter::Error),
    #[error("Invalid object ID: {0}")]
    InvalidObjectId(String),
    #[error("Object not found: {0}")]
    ObjectNotFound(String),
    #[error("Diff error: {0}")]
    Diff(String),
    #[error("Invalid object: {0}")]
    InvalidObject(String),
    #[error("Remote not found: {0}")]
    RemoteNotFound(String),
    #[error("Remote connection failed: {0}")]
    RemoteConnect(#[from] gix::remote::connect::Error),
    #[error("Fetch preparation failed: {0}")]
    FetchPrepare(#[from] gix::remote::fetch::prepare::Error),
    #[error("Fetch failed: {0}")]
    Fetch(#[from] gix::remote::fetch::Error),
    #[error("Invalid refspec: {0}")]
    InvalidRefspec(String),
    #[error("Status operation failed: {0}")]
    Status(String),
}

/// Change type for a file in a tree-to-tree diff
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffChangeType {
    Added,
    Deleted,
    Modified,
    Renamed,
    Copied,
}

/// A single file change from a tree-to-tree diff
#[derive(Debug, Clone)]
pub struct TreeDiffEntry {
    /// Type of change
    pub change_type: DiffChangeType,
    /// Old path (for deletions, modifications, renames)
    pub old_path: Option<String>,
    /// New path (for additions, modifications, renames)
    pub new_path: Option<String>,
    /// Old blob OID (for deletions, modifications)
    pub old_oid: Option<gix::ObjectId>,
    /// New blob OID (for additions, modifications)
    pub new_oid: Option<gix::ObjectId>,
    /// Old file mode
    pub old_mode: Option<gix::object::tree::EntryKind>,
    /// New file mode
    pub new_mode: Option<gix::object::tree::EntryKind>,
}

/// Branch type enumeration matching git2::BranchType semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchType {
    Local,
    Remote,
}

/// HEAD information: branch name (or "HEAD" if detached) and commit OID.
#[derive(Debug, Clone)]
pub struct HeadInfo {
    pub branch: String,
    pub oid: String,
}

/// Summary of worktree status for display and decision making.
#[derive(Debug, Clone, Default)]
pub struct WorktreeStatusSummary {
    /// Count of uncommitted changes to tracked files (staged or unstaged).
    pub uncommitted_tracked: usize,
    /// Count of untracked files.
    pub untracked: usize,
}

/// Read-only interface for Git repository operations using gix.
///
/// `GixReader` provides a unified way to open and read from both regular Git
/// repositories and worktrees. It handles the special `.git` file format used
/// by worktrees automatically.
#[derive(Debug)]
pub struct GixReader;

impl GixReader {
    /// Open a Git repository at the given path.
    ///
    /// This method handles both regular repositories (with a `.git` directory)
    /// and worktrees (with a `.git` file pointing to the main repository).
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the repository root or worktree directory
    ///
    /// # Returns
    ///
    /// A `gix::Repository` instance configured for read operations.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use services::services::gix_reader::GixReader;
    /// use std::path::Path;
    ///
    /// let repo = GixReader::open(Path::new("/path/to/repo")).unwrap();
    /// ```
    pub fn open(path: &Path) -> Result<gix::Repository, GixReaderError> {
        // gix::open() handles both regular repos and worktrees automatically.
        // For worktrees, it reads the .git file and follows the gitdir pointer.
        let repo = gix::open(path)?;
        Ok(repo)
    }

    /// Open a Git repository with isolated configuration.
    ///
    /// This opens the repository without loading global or system configuration,
    /// making it faster but potentially missing some settings.
    ///
    /// Use this for performance-critical read operations where complete
    /// configuration is not required.
    pub fn open_isolated(path: &Path) -> Result<gix::Repository, GixReaderError> {
        let repo = gix::open_opts(path, gix::open::Options::isolated())?;
        Ok(repo)
    }

    /// Open a worktree path, following the gitdir pointer if present.
    ///
    /// Worktrees have a `.git` file (not directory) containing:
    /// ```text
    /// gitdir: /path/to/main/repo/.git/worktrees/{name}
    /// ```
    ///
    /// This method explicitly handles this case and is functionally equivalent
    /// to [`open`](Self::open), but more clearly documents the intent.
    ///
    /// # Arguments
    ///
    /// * `worktree_path` - Path to the worktree directory
    ///
    /// # Returns
    ///
    /// A `gix::Repository` instance for the worktree.
    pub fn open_worktree(worktree_path: &Path) -> Result<gix::Repository, GixReaderError> {
        // gix handles .git files automatically - it reads the gitdir pointer
        // and opens the correct repository. This method exists for API clarity.
        Self::open(worktree_path)
    }

    /// Discover and open a Git repository starting from any directory.
    ///
    /// This walks up the directory tree to find the repository root,
    /// similar to how `git` commands work from subdirectories.
    ///
    /// # Arguments
    ///
    /// * `directory` - Any directory within or at the root of a repository
    ///
    /// # Returns
    ///
    /// A `gix::Repository` instance for the discovered repository.
    pub fn discover(directory: &Path) -> Result<gix::Repository, GixReaderError> {
        let repo = gix::discover(directory)?;
        Ok(repo)
    }

    /// Calculate how many commits `local` is ahead of and behind `remote`.
    ///
    /// This finds the merge base between the two commits, then counts
    /// commits from each side to the base.
    ///
    /// # Arguments
    ///
    /// * `repo` - An open gix repository
    /// * `local` - The local commit ObjectId
    /// * `remote` - The remote commit ObjectId
    ///
    /// # Returns
    ///
    /// A tuple `(ahead, behind)` where:
    /// - `ahead`: commits in `local` not in `remote`
    /// - `behind`: commits in `remote` not in `local`
    pub fn ahead_behind(
        repo: &gix::Repository,
        local: gix::ObjectId,
        remote: gix::ObjectId,
    ) -> Result<(usize, usize), GixReaderError> {
        // Fast path: same commit
        if local == remote {
            return Ok((0, 0));
        }

        // Find the merge base
        let base: gix::ObjectId = repo.merge_base(local, remote)?.into();

        // Count commits from local to base (ahead count)
        let ahead = Self::count_commits_to_base(repo, local, base)?;

        // Count commits from remote to base (behind count)
        let behind = Self::count_commits_to_base(repo, remote, base)?;

        Ok((ahead, behind))
    }

    /// Count commits from `start` to `base` (exclusive of base).
    fn count_commits_to_base(
        repo: &gix::Repository,
        start: gix::ObjectId,
        base: gix::ObjectId,
    ) -> Result<usize, GixReaderError> {
        // Fast path: start is the base
        if start == base {
            return Ok(0);
        }

        let mut count = 0;
        let walk = repo.rev_walk([start]);

        for info_result in walk.all()? {
            let info = info_result?;
            if info.id == base {
                break;
            }
            count += 1;
        }

        Ok(count)
    }

    /// Calculate ahead/behind between two commits by their hex OID strings.
    ///
    /// This is a convenience wrapper around [`ahead_behind`](Self::ahead_behind)
    /// that parses hex OID strings.
    pub fn ahead_behind_by_oid(
        repo: &gix::Repository,
        local_oid: &str,
        remote_oid: &str,
    ) -> Result<(usize, usize), GixReaderError> {
        let local = gix::ObjectId::from_hex(local_oid.as_bytes())
            .map_err(|_| GixReaderError::InvalidObjectId(local_oid.to_string()))?;
        let remote = gix::ObjectId::from_hex(remote_oid.as_bytes())
            .map_err(|_| GixReaderError::InvalidObjectId(remote_oid.to_string()))?;
        Self::ahead_behind(repo, local, remote)
    }

    /// Compute a tree-to-tree diff between two tree OIDs.
    ///
    /// Returns a list of file changes with their paths and blob OIDs.
    /// Supports rename detection.
    ///
    /// # Arguments
    ///
    /// * `repo` - The gix repository
    /// * `old_tree_id` - OID of the old (base) tree
    /// * `new_tree_id` - OID of the new tree
    ///
    /// # Returns
    ///
    /// A vector of `TreeDiffEntry` describing each changed file.
    pub fn diff_trees(
        repo: &gix::Repository,
        old_tree_id: gix::ObjectId,
        new_tree_id: gix::ObjectId,
    ) -> Result<Vec<TreeDiffEntry>, GixReaderError> {
        use gix::{
            bstr::ByteSlice,
            object::tree::{diff::ChangeDetached, EntryKind},
        };

        let mut entries = Vec::new();

        // Get Tree objects
        let old_tree = repo
            .find_object(old_tree_id)
            .map_err(|e| GixReaderError::ObjectNotFound(format!("old tree: {e}")))?
            .try_into_tree()
            .map_err(|e| GixReaderError::InvalidObject(format!("old tree not a tree: {e}")))?;

        let new_tree = repo
            .find_object(new_tree_id)
            .map_err(|e| GixReaderError::ObjectNotFound(format!("new tree: {e}")))?
            .try_into_tree()
            .map_err(|e| GixReaderError::InvalidObject(format!("new tree not a tree: {e}")))?;

        // Perform the diff - uses repository configuration for options (including rename detection)
        let changes = repo
            .diff_tree_to_tree(&old_tree, &new_tree, None)
            .map_err(|e| GixReaderError::Diff(format!("tree diff failed: {e}")))?;

        // Process changes
        for change in changes {
            let entry = match change {
                ChangeDetached::Addition {
                    location,
                    entry_mode,
                    id,
                    ..
                } => TreeDiffEntry {
                    change_type: DiffChangeType::Added,
                    old_path: None,
                    new_path: Some(bstr_to_string(location.as_bstr())),
                    old_oid: None,
                    new_oid: Some(id),
                    old_mode: None,
                    new_mode: Some(entry_mode.kind()),
                },
                ChangeDetached::Deletion {
                    location,
                    entry_mode,
                    id,
                    ..
                } => TreeDiffEntry {
                    change_type: DiffChangeType::Deleted,
                    old_path: Some(bstr_to_string(location.as_bstr())),
                    new_path: None,
                    old_oid: Some(id),
                    new_oid: None,
                    old_mode: Some(entry_mode.kind()),
                    new_mode: None,
                },
                ChangeDetached::Modification {
                    location,
                    previous_entry_mode,
                    previous_id,
                    entry_mode,
                    id,
                } => TreeDiffEntry {
                    change_type: DiffChangeType::Modified,
                    old_path: Some(bstr_to_string(location.as_bstr())),
                    new_path: Some(bstr_to_string(location.as_bstr())),
                    old_oid: Some(previous_id),
                    new_oid: Some(id),
                    old_mode: Some(previous_entry_mode.kind()),
                    new_mode: Some(entry_mode.kind()),
                },
                ChangeDetached::Rewrite {
                    source_location,
                    source_id,
                    location,
                    id,
                    copy,
                    entry_mode,
                    source_entry_mode,
                    ..
                } => TreeDiffEntry {
                    change_type: if copy {
                        DiffChangeType::Copied
                    } else {
                        DiffChangeType::Renamed
                    },
                    old_path: Some(bstr_to_string(source_location.as_bstr())),
                    new_path: Some(bstr_to_string(location.as_bstr())),
                    old_oid: Some(source_id),
                    new_oid: Some(id),
                    old_mode: Some(source_entry_mode.kind()),
                    new_mode: Some(entry_mode.kind()),
                },
            };

            // Only include blob entries (files), skip trees/submodules
            let dominated_by_blob = matches!(
                entry.new_mode,
                Some(EntryKind::Blob | EntryKind::BlobExecutable)
            ) || matches!(
                entry.old_mode,
                Some(EntryKind::Blob | EntryKind::BlobExecutable)
            );

            if dominated_by_blob {
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    /// Read the contents of a blob by OID.
    ///
    /// Returns the blob content as bytes, or None if it's binary.
    ///
    /// # Arguments
    ///
    /// * `repo` - The gix repository
    /// * `blob_id` - OID of the blob to read
    ///
    /// # Returns
    ///
    /// The blob content as a String if it's valid UTF-8 text, None otherwise.
    pub fn read_blob(
        repo: &gix::Repository,
        blob_id: gix::ObjectId,
    ) -> Result<Option<String>, GixReaderError> {
        let blob = repo
            .find_object(blob_id)
            .map_err(|e| GixReaderError::ObjectNotFound(format!("blob {blob_id}: {e}")))?;

        let data = blob.data.as_slice();

        // Check for binary content (null bytes)
        if data.contains(&0) {
            return Ok(None);
        }

        // Try to convert to UTF-8
        match std::str::from_utf8(data) {
            Ok(s) => Ok(Some(s.to_string())),
            Err(_) => Ok(None), // Not valid UTF-8, treat as binary
        }
    }

    /// Read blob contents at a specific commit for a given path.
    ///
    /// # Arguments
    ///
    /// * `repo` - The gix repository
    /// * `commit_id` - The commit OID
    /// * `path` - The file path relative to repository root
    ///
    /// # Returns
    ///
    /// The file content as a String if found and is valid UTF-8 text, None otherwise.
    pub fn file_contents_at(
        repo: &gix::Repository,
        commit_id: gix::ObjectId,
        path: &str,
    ) -> Result<Option<String>, GixReaderError> {
        // Find the commit
        let commit = repo
            .find_object(commit_id)
            .map_err(|e| GixReaderError::ObjectNotFound(format!("commit {commit_id}: {e}")))?
            .try_into_commit()
            .map_err(|e| GixReaderError::InvalidObject(format!("not a commit: {e}")))?;

        // Get the tree
        let tree_id = commit
            .tree_id()
            .map_err(|e| GixReaderError::InvalidObject(format!("commit has no tree: {e}")))?;

        let tree = repo
            .find_object(tree_id)
            .map_err(|e| GixReaderError::ObjectNotFound(format!("tree: {e}")))?
            .try_into_tree()
            .map_err(|e| GixReaderError::InvalidObject(format!("not a tree: {e}")))?;

        // Look up the path in the tree
        let entry = match tree.lookup_entry_by_path(path) {
            Ok(Some(entry)) => entry,
            Ok(None) => return Ok(None), // Path not found
            Err(e) => {
                return Err(GixReaderError::Diff(format!(
                    "failed to lookup path {path}: {e}"
                )))
            }
        };

        // Get the blob
        let blob_id = entry.object_id();
        Self::read_blob(repo, blob_id)
    }

    /// Get blob size without reading full content.
    ///
    /// # Arguments
    ///
    /// * `repo` - The gix repository
    /// * `blob_id` - OID of the blob
    ///
    /// # Returns
    ///
    /// The size of the blob in bytes.
    pub fn blob_size(
        repo: &gix::Repository,
        blob_id: gix::ObjectId,
    ) -> Result<usize, GixReaderError> {
        let header = repo
            .find_header(blob_id)
            .map_err(|e| GixReaderError::ObjectNotFound(format!("blob {blob_id}: {e}")))?;
        Ok(header.size() as usize)
    }

    /// Check if a blob is binary (contains null bytes).
    ///
    /// # Arguments
    ///
    /// * `repo` - The gix repository
    /// * `blob_id` - OID of the blob
    ///
    /// # Returns
    ///
    /// True if the blob contains binary content.
    pub fn is_blob_binary(
        repo: &gix::Repository,
        blob_id: gix::ObjectId,
    ) -> Result<bool, GixReaderError> {
        let blob = repo
            .find_object(blob_id)
            .map_err(|e| GixReaderError::ObjectNotFound(format!("blob {blob_id}: {e}")))?;

        Ok(blob.data.as_slice().contains(&0))
    }

    /// Get HEAD information: branch name and commit OID.
    ///
    /// Returns the current branch name (or "HEAD" if detached) and the
    /// commit OID that HEAD points to.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the repository root or worktree directory
    ///
    /// # Returns
    ///
    /// [`HeadInfo`] containing the branch name and OID as strings.
    pub fn head_info(path: &Path) -> Result<HeadInfo, GixReaderError> {
        let repo = Self::open(path)?;

        // Get branch name from HEAD (None if detached)
        let branch = match repo.head_name()? {
            Some(name) => {
                // Extract short name from full ref (e.g., "refs/heads/main" -> "main")
                name.shorten().to_string()
            }
            None => "HEAD".to_string(),
        };

        // Get the OID that HEAD points to
        let head_id = repo
            .head_id()
            .map_err(|_| GixReaderError::DetachedHeadNoTarget)?;
        let oid = head_id.to_string();

        Ok(HeadInfo { branch, oid })
    }

    /// Find a branch by name (local or remote).
    ///
    /// Tries local branches first (`refs/heads/{name}`), then remote branches
    /// (`refs/remotes/{name}`).
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the repository
    /// * `branch_name` - Branch name (e.g., "main" or "origin/main")
    ///
    /// # Returns
    ///
    /// A tuple of (full reference name, branch type).
    pub fn find_branch(
        path: &Path,
        branch_name: &str,
    ) -> Result<(String, BranchType), GixReaderError> {
        let repo = Self::open(path)?;

        // Try local branch first
        let local_ref = format!("refs/heads/{}", branch_name);
        if repo.find_reference(&local_ref).is_ok() {
            return Ok((local_ref, BranchType::Local));
        }

        // Try remote branch
        let remote_ref = format!("refs/remotes/{}", branch_name);
        if repo.find_reference(&remote_ref).is_ok() {
            return Ok((remote_ref, BranchType::Remote));
        }

        Err(GixReaderError::ReferenceNotFound(branch_name.to_string()))
    }

    /// Get the branch type (local or remote) for a given branch name.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the repository
    /// * `branch_name` - Branch name to look up
    ///
    /// # Returns
    ///
    /// [`BranchType::Local`] or [`BranchType::Remote`].
    pub fn branch_type(path: &Path, branch_name: &str) -> Result<BranchType, GixReaderError> {
        let (_, branch_type) = Self::find_branch(path, branch_name)?;
        Ok(branch_type)
    }

    /// Get the commit OID for a branch.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the repository
    /// * `branch_name` - Branch name (e.g., "main" or "origin/main")
    ///
    /// # Returns
    ///
    /// The commit OID as a hex string.
    pub fn branch_oid(path: &Path, branch_name: &str) -> Result<String, GixReaderError> {
        let repo = Self::open(path)?;
        let (ref_name, _) = Self::find_branch(path, branch_name)?;

        let reference = repo.find_reference(&ref_name)?;
        let oid = reference
            .into_fully_peeled_id()
            .map_err(|e| GixReaderError::ReferenceNotFound(format!("Failed to peel: {}", e)))?;

        Ok(oid.to_string())
    }

    /// Fetch from a remote repository using a specific refspec.
    ///
    /// This uses gix's native fetch implementation which handles authentication
    /// via git credential helpers automatically (SSH keys, credential managers, etc.).
    ///
    /// # Arguments
    ///
    /// * `repo` - An open gix repository
    /// * `remote_name` - Name of the remote (e.g., "origin")
    /// * `refspec` - The refspec to fetch (e.g., "+refs/heads/*:refs/remotes/origin/*")
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use services::services::gix_reader::GixReader;
    /// use std::path::Path;
    ///
    /// let repo = GixReader::open(Path::new("/path/to/repo")).unwrap();
    /// // Fetch all branches from origin
    /// GixReader::fetch(&repo, "origin", "+refs/heads/*:refs/remotes/origin/*").unwrap();
    /// ```
    pub fn fetch(
        repo: &gix::Repository,
        remote_name: &str,
        refspec: &str,
    ) -> Result<(), GixReaderError> {
        let remote = repo
            .find_remote(remote_name)
            .map_err(|_| GixReaderError::RemoteNotFound(remote_name.to_string()))?;

        Self::fetch_with_remote(remote, refspec)
    }

    /// Fetch from a remote URL with a specific refspec.
    ///
    /// Use this when you have a URL instead of a configured remote name.
    ///
    /// # Arguments
    ///
    /// * `repo` - An open gix repository
    /// * `url` - The remote URL (e.g., "https://github.com/org/repo.git")
    /// * `refspec` - The refspec to fetch
    pub fn fetch_url(
        repo: &gix::Repository,
        url: &str,
        refspec: &str,
    ) -> Result<(), GixReaderError> {
        let remote = repo
            .remote_at(url)
            .map_err(|_| GixReaderError::RemoteNotFound(url.to_string()))?;

        Self::fetch_with_remote(remote, refspec)
    }

    /// Internal helper to perform fetch with a gix Remote.
    fn fetch_with_remote<'repo>(
        remote: gix::Remote<'repo>,
        refspec: &str,
    ) -> Result<(), GixReaderError> {
        // Override the remote's refspecs with our specific one
        let remote = remote
            .with_refspecs(Some(refspec), Direction::Fetch)
            .map_err(|e| GixReaderError::InvalidRefspec(e.to_string()))?;

        // Connect and fetch
        let outcome = remote
            .connect(Direction::Fetch)?
            .prepare_fetch(gix::progress::Discard, Default::default())?
            .receive(gix::progress::Discard, &AtomicBool::new(false))?;

        tracing::debug!(
            "Fetch complete: {} ref updates",
            outcome.ref_map.mappings.len()
        );

        Ok(())
    }

    /// Get a summary of the worktree status: counts of uncommitted tracked changes
    /// and untracked files.
    ///
    /// This uses gix's status API which is faster than shelling out to `git status`.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the repository or worktree
    ///
    /// # Returns
    ///
    /// A `WorktreeStatusSummary` with counts.
    pub fn get_worktree_status(path: &Path) -> Result<WorktreeStatusSummary, GixReaderError> {
        let repo = Self::open(path)?;
        Self::get_worktree_status_from_repo(&repo)
    }

    /// Get worktree status from an already-opened repository.
    pub fn get_worktree_status_from_repo(
        repo: &gix::Repository,
    ) -> Result<WorktreeStatusSummary, GixReaderError> {
        use gix::{
            dir::walk::EmissionMode,
            status::{plumbing::index_as_worktree::EntryStatus, tree_index::TrackRenames},
        };

        let status_iter = repo
            .status(gix::progress::Discard)
            .map_err(|e| GixReaderError::Status(e.to_string()))?
            // Disable rename tracking for speed - we just need counts
            .tree_index_track_renames(TrackRenames::Disabled)
            .index_worktree_rewrites(None)
            .index_worktree_options_mut(|opts| {
                if let Some(dirwalk) = opts.dirwalk_options.as_mut() {
                    dirwalk
                        .set_emit_ignored(None)
                        .set_emit_pruned(false)
                        .set_emit_tracked(false)
                        .set_emit_untracked(EmissionMode::Matching)
                        .set_emit_collapsed(None);
                }
            })
            .into_iter(None)
            .map_err(|e| GixReaderError::Status(e.to_string()))?;

        let mut summary = WorktreeStatusSummary::default();

        for item_result in status_iter {
            let item = match item_result {
                Ok(item) => item,
                Err(_) => continue, // Skip errors, count what we can
            };

            match item {
                // Staged changes (index differs from HEAD tree)
                gix::status::Item::TreeIndex(_) => {
                    summary.uncommitted_tracked += 1;
                }
                // Unstaged changes (worktree differs from index)
                gix::status::Item::IndexWorktree(iw_item) => match &iw_item {
                    index_worktree::Item::Modification { status, .. } => {
                        // Check if it's a real modification vs just needs-update
                        if !matches!(status, EntryStatus::NeedsUpdate(_)) {
                            summary.uncommitted_tracked += 1;
                        }
                    }
                    index_worktree::Item::DirectoryContents { entry, .. } => {
                        // Untracked files come through here
                        if matches!(entry.status, gix::dir::entry::Status::Untracked) {
                            summary.untracked += 1;
                        }
                    }
                    index_worktree::Item::Rewrite { .. } => {
                        // Rewrites are disabled, but handle just in case
                        summary.uncommitted_tracked += 1;
                    }
                },
            }
        }

        Ok(summary)
    }

    /// Check if the worktree has any uncommitted changes to tracked files.
    ///
    /// This is faster than `get_worktree_status()` when you only need a boolean check,
    /// as it can short-circuit on the first change found.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the repository or worktree
    ///
    /// # Returns
    ///
    /// `true` if there are uncommitted changes to tracked files, `false` if clean.
    pub fn is_dirty(path: &Path) -> Result<bool, GixReaderError> {
        let repo = Self::open(path)?;
        Self::is_dirty_from_repo(&repo)
    }

    /// Check if repository is dirty from an already-opened repository.
    pub fn is_dirty_from_repo(repo: &gix::Repository) -> Result<bool, GixReaderError> {
        use gix::status::{plumbing::index_as_worktree::EntryStatus, tree_index::TrackRenames};

        let status_iter = repo
            .status(gix::progress::Discard)
            .map_err(|e| GixReaderError::Status(e.to_string()))?
            .tree_index_track_renames(TrackRenames::Disabled)
            .index_worktree_rewrites(None)
            // Skip untracked files for dirty check - we only care about tracked changes
            // Setting dirwalk_options to None disables directory walk entirely
            .index_worktree_options_mut(|opts| {
                opts.dirwalk_options = None;
            })
            .into_iter(None)
            .map_err(|e| GixReaderError::Status(e.to_string()))?;

        for item_result in status_iter {
            let item = match item_result {
                Ok(item) => item,
                Err(_) => continue,
            };

            match item {
                // Any staged change means dirty
                gix::status::Item::TreeIndex(_) => {
                    return Ok(true);
                }
                // Any worktree modification (not just NeedsUpdate) means dirty
                gix::status::Item::IndexWorktree(iw_item) => {
                    if let index_worktree::Item::Modification { status, .. } = &iw_item {
                        if !matches!(status, EntryStatus::NeedsUpdate(_)) {
                            return Ok(true);
                        }
                    }
                    // Rewrites also count as dirty
                    if matches!(iw_item, index_worktree::Item::Rewrite { .. }) {
                        return Ok(true);
                    }
                }
            }
        }

        Ok(false)
    }

    /// Get list of dirty file paths (for error messages).
    ///
    /// Returns paths of files with uncommitted changes to tracked files.
    pub fn get_dirty_files(path: &Path) -> Result<Vec<String>, GixReaderError> {
        let repo = Self::open(path)?;
        Self::get_dirty_files_from_repo(&repo)
    }

    /// Get dirty files from an already-opened repository.
    pub fn get_dirty_files_from_repo(
        repo: &gix::Repository,
    ) -> Result<Vec<String>, GixReaderError> {
        use gix::{
            bstr::ByteSlice,
            status::{plumbing::index_as_worktree::EntryStatus, tree_index::TrackRenames},
        };

        let status_iter = repo
            .status(gix::progress::Discard)
            .map_err(|e| GixReaderError::Status(e.to_string()))?
            .tree_index_track_renames(TrackRenames::Disabled)
            .index_worktree_rewrites(None)
            // Disable directory walk to skip untracked files
            .index_worktree_options_mut(|opts| {
                opts.dirwalk_options = None;
            })
            .into_iter(None)
            .map_err(|e| GixReaderError::Status(e.to_string()))?;

        let mut files = Vec::new();

        for item_result in status_iter {
            let item = match item_result {
                Ok(item) => item,
                Err(_) => continue,
            };

            match item {
                gix::status::Item::TreeIndex(change) => {
                    let path = match &change {
                        gix::diff::index::Change::Addition { location, .. }
                        | gix::diff::index::Change::Deletion { location, .. }
                        | gix::diff::index::Change::Modification { location, .. } => {
                            location.to_str_lossy().to_string()
                        }
                        gix::diff::index::Change::Rewrite {
                            source_location, ..
                        } => source_location.to_str_lossy().to_string(),
                    };
                    files.push(path);
                }
                gix::status::Item::IndexWorktree(iw_item) => {
                    if let index_worktree::Item::Modification {
                        rela_path, status, ..
                    } = &iw_item
                    {
                        if !matches!(status, EntryStatus::NeedsUpdate(_)) {
                            files.push(rela_path.to_str_lossy().to_string());
                        }
                    }
                    // Rewrites have source: RewriteSource, get path from dirwalk_entry
                    if let index_worktree::Item::Rewrite { dirwalk_entry, .. } = &iw_item {
                        files.push(dirwalk_entry.rela_path.to_str_lossy().to_string());
                    }
                }
            }
        }

        Ok(files)
    }
}

/// Convert BStr to String, handling non-UTF8 paths gracefully
fn bstr_to_string(bstr: &BStr) -> String {
    String::from_utf8_lossy(bstr).to_string()
}

#[cfg(test)]
mod tests {
    use std::{fs, process::Command};

    use tempfile::TempDir;

    use super::*;

    fn init_test_repo_via_cli(dir: &Path) {
        // Use git CLI for repo initialization - simpler and more reliable
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
    fn test_open_regular_repo() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        init_test_repo_via_cli(repo_path);

        let result = GixReader::open(repo_path);
        assert!(result.is_ok());

        let repo = result.unwrap();
        assert_eq!(repo.workdir(), Some(repo_path));
    }

    #[test]
    fn test_open_isolated() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        init_test_repo_via_cli(repo_path);

        let result = GixReader::open_isolated(repo_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_open_worktree() {
        let temp_dir = TempDir::new().unwrap();
        let main_repo_path = temp_dir.path().join("main");
        fs::create_dir_all(&main_repo_path).unwrap();

        // Initialize main repository
        init_test_repo_via_cli(&main_repo_path);

        // Create a worktree
        let worktree_path = temp_dir.path().join("worktree");

        let output = Command::new("git")
            .args([
                "worktree",
                "add",
                "-b",
                "test-branch",
                worktree_path.to_str().unwrap(),
            ])
            .current_dir(&main_repo_path)
            .output()
            .expect("Failed to create worktree");

        if !output.status.success() {
            panic!(
                "git worktree add failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // Verify worktree has .git file (not directory)
        let git_path = worktree_path.join(".git");
        assert!(git_path.exists());
        assert!(git_path.is_file(), "Worktree should have .git file");

        // Test opening the worktree
        let result = GixReader::open_worktree(&worktree_path);
        assert!(result.is_ok());

        let repo = result.unwrap();
        assert_eq!(repo.workdir(), Some(worktree_path.as_path()));
    }

    #[test]
    fn test_discover_from_subdirectory() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        init_test_repo_via_cli(repo_path);

        // Create a subdirectory
        let subdir = repo_path.join("src").join("nested");
        fs::create_dir_all(&subdir).unwrap();

        // Discover repo from subdirectory
        let result = GixReader::discover(&subdir);
        assert!(result.is_ok());

        let repo = result.unwrap();
        assert_eq!(repo.workdir(), Some(repo_path));
    }

    #[test]
    fn test_open_nonexistent_path() {
        let result = GixReader::open(Path::new("/nonexistent/path/to/repo"));
        assert!(result.is_err());
    }

    #[test]
    fn test_head_info() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        init_test_repo_via_cli(repo_path);

        let head_info = GixReader::head_info(repo_path).unwrap();
        assert_eq!(head_info.branch, "main");
        // OID should be a 40-character hex string
        assert_eq!(head_info.oid.len(), 40);
        assert!(head_info.oid.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_head_info_worktree() {
        let temp_dir = TempDir::new().unwrap();
        let main_repo_path = temp_dir.path().join("main");
        fs::create_dir_all(&main_repo_path).unwrap();

        init_test_repo_via_cli(&main_repo_path);

        // Create a worktree on a different branch
        let worktree_path = temp_dir.path().join("worktree");
        Command::new("git")
            .args([
                "worktree",
                "add",
                "-b",
                "feature-branch",
                worktree_path.to_str().unwrap(),
            ])
            .current_dir(&main_repo_path)
            .output()
            .expect("Failed to create worktree");

        let head_info = GixReader::head_info(&worktree_path).unwrap();
        assert_eq!(head_info.branch, "feature-branch");
    }

    #[test]
    fn test_find_branch_local() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        init_test_repo_via_cli(repo_path);

        let (ref_name, branch_type) = GixReader::find_branch(repo_path, "main").unwrap();
        assert_eq!(ref_name, "refs/heads/main");
        assert_eq!(branch_type, BranchType::Local);
    }

    #[test]
    fn test_find_branch_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        init_test_repo_via_cli(repo_path);

        let result = GixReader::find_branch(repo_path, "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_branch_type() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        init_test_repo_via_cli(repo_path);

        let branch_type = GixReader::branch_type(repo_path, "main").unwrap();
        assert_eq!(branch_type, BranchType::Local);
    }

    #[test]
    fn test_branch_oid() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        init_test_repo_via_cli(repo_path);

        let oid = GixReader::branch_oid(repo_path, "main").unwrap();
        // OID should be a 40-character hex string
        assert_eq!(oid.len(), 40);
        assert!(oid.chars().all(|c| c.is_ascii_hexdigit()));

        // Should match HEAD since we're on main
        let head_info = GixReader::head_info(repo_path).unwrap();
        assert_eq!(oid, head_info.oid);
    }

    #[test]
    fn test_fetch_from_local_remote() {
        // Create a "remote" repository
        let remote_dir = TempDir::new().unwrap();
        let remote_path = remote_dir.path();
        init_test_repo_via_cli(remote_path);

        // Add another commit to the remote
        fs::write(remote_path.join("file.txt"), "content").unwrap();
        Command::new("git")
            .args(["add", "file.txt"])
            .current_dir(remote_path)
            .output()
            .expect("Failed to add file");
        Command::new("git")
            .args(["commit", "-m", "Second commit"])
            .current_dir(remote_path)
            .output()
            .expect("Failed to commit");

        // Clone the remote to create a local repo
        let local_dir = TempDir::new().unwrap();
        let local_path = local_dir.path().join("repo");
        Command::new("git")
            .args([
                "clone",
                remote_path.to_str().unwrap(),
                local_path.to_str().unwrap(),
            ])
            .output()
            .expect("Failed to clone");

        // Add yet another commit to remote (so local is behind)
        fs::write(remote_path.join("file2.txt"), "more content").unwrap();
        Command::new("git")
            .args(["add", "file2.txt"])
            .current_dir(remote_path)
            .output()
            .expect("Failed to add second file");
        Command::new("git")
            .args(["commit", "-m", "Third commit"])
            .current_dir(remote_path)
            .output()
            .expect("Failed to create third commit");

        // Now test fetch using gix
        let repo = GixReader::open(&local_path).expect("Failed to open local repo");
        let result = GixReader::fetch(&repo, "origin", "+refs/heads/*:refs/remotes/origin/*");

        assert!(result.is_ok(), "Fetch failed: {:?}", result.err());

        // Verify the remote ref was updated by checking we can see the new commit
        let output = Command::new("git")
            .args(["log", "--oneline", "origin/main"])
            .current_dir(&local_path)
            .output()
            .expect("Failed to run git log");

        let log_output = String::from_utf8_lossy(&output.stdout);
        assert!(
            log_output.contains("Third commit"),
            "Expected to see 'Third commit' after fetch, got: {}",
            log_output
        );
    }

    #[test]
    fn test_fetch_url_from_local_remote() {
        // Create a "remote" repository
        let remote_dir = TempDir::new().unwrap();
        let remote_path = remote_dir.path();
        init_test_repo_via_cli(remote_path);

        // Clone the remote to create a local repo
        let local_dir = TempDir::new().unwrap();
        let local_path = local_dir.path().join("repo");
        Command::new("git")
            .args([
                "clone",
                remote_path.to_str().unwrap(),
                local_path.to_str().unwrap(),
            ])
            .output()
            .expect("Failed to clone");

        // Test fetch_url with file:// URL
        let repo = GixReader::open(&local_path).expect("Failed to open local repo");
        let file_url = format!("file://{}", remote_path.display());
        let result = GixReader::fetch_url(&repo, &file_url, "+refs/heads/*:refs/remotes/origin/*");

        assert!(result.is_ok(), "Fetch URL failed: {:?}", result.err());
    }

    #[test]
    fn test_status_clean_repo() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        init_test_repo_via_cli(repo_path);

        // Clean repo should have no changes
        let summary = GixReader::get_worktree_status(repo_path).unwrap();
        assert_eq!(summary.uncommitted_tracked, 0);
        assert_eq!(summary.untracked, 0);

        // is_dirty should return false
        assert!(!GixReader::is_dirty(repo_path).unwrap());

        // get_dirty_files should return empty
        let dirty_files = GixReader::get_dirty_files(repo_path).unwrap();
        assert!(dirty_files.is_empty());
    }

    #[test]
    fn test_status_untracked_file() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        init_test_repo_via_cli(repo_path);

        // Create untracked file
        fs::write(repo_path.join("untracked.txt"), "hello").unwrap();

        let summary = GixReader::get_worktree_status(repo_path).unwrap();
        assert_eq!(summary.uncommitted_tracked, 0);
        assert_eq!(summary.untracked, 1);

        // Untracked files don't make repo "dirty" (only tracked changes do)
        assert!(!GixReader::is_dirty(repo_path).unwrap());
    }

    #[test]
    fn test_status_modified_tracked_file() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        init_test_repo_via_cli(repo_path);

        // Create and commit a file
        let file_path = repo_path.join("tracked.txt");
        fs::write(&file_path, "initial").unwrap();
        Command::new("git")
            .args(["add", "tracked.txt"])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "add file"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Modify the tracked file
        fs::write(&file_path, "modified").unwrap();

        let summary = GixReader::get_worktree_status(repo_path).unwrap();
        assert_eq!(summary.uncommitted_tracked, 1);
        assert_eq!(summary.untracked, 0);

        // Modified tracked file makes repo dirty
        assert!(GixReader::is_dirty(repo_path).unwrap());

        // Dirty files should include the modified file
        let dirty_files = GixReader::get_dirty_files(repo_path).unwrap();
        assert_eq!(dirty_files.len(), 1);
        assert!(dirty_files[0].contains("tracked.txt"));
    }

    #[test]
    fn test_status_staged_file() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        init_test_repo_via_cli(repo_path);

        // Create and stage a new file (but don't commit)
        let file_path = repo_path.join("staged.txt");
        fs::write(&file_path, "staged content").unwrap();
        Command::new("git")
            .args(["add", "staged.txt"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let summary = GixReader::get_worktree_status(repo_path).unwrap();
        assert_eq!(summary.uncommitted_tracked, 1); // Staged = uncommitted tracked
        assert_eq!(summary.untracked, 0);

        assert!(GixReader::is_dirty(repo_path).unwrap());
    }
}
