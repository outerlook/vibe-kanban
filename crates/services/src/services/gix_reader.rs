//! Git repository operations using gix (gitoxide).
//!
//! This module encapsulates gix operations and handles both regular
//! repositories and worktrees transparently.

use std::{collections::HashMap, path::Path, sync::atomic::AtomicBool};

use chrono::{DateTime, Utc};
use gix::bstr::BStr;
use gix::remote::Direction;
use gix::status::index_worktree;
use thiserror::Error;

/// Statistics for a single file based on git history
#[derive(Clone, Debug)]
pub struct FileStat {
    /// Index in the commit history (0 = HEAD, 1 = parent of HEAD, ...)
    pub last_index: usize,
    /// Number of times this file was changed in recent commits
    pub commit_count: u32,
    /// Timestamp of the most recent change
    pub last_time: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum GixReaderError {
    #[error("Failed to open repository: {0}")]
    Open(#[from] gix::open::Error),
    #[error("Reference not found: {0}")]
    ReferenceNotFound(String),
    #[error("HEAD has no target commit")]
    DetachedHeadNoTarget,
    #[error("Reference error: {0}")]
    Reference(#[from] gix::reference::find::existing::Error),
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
    /// Handles both regular repositories (with a `.git` directory) and worktrees
    /// (with a `.git` file pointing to the main repository).
    pub fn open(path: &Path) -> Result<gix::Repository, GixReaderError> {
        Ok(gix::open(path)?)
    }

    /// Parse a hex OID string into a gix ObjectId.
    fn parse_oid(oid_str: &str) -> Result<gix::ObjectId, GixReaderError> {
        gix::ObjectId::from_hex(oid_str.as_bytes())
            .map_err(|_| GixReaderError::InvalidObjectId(oid_str.to_string()))
    }

    /// Calculate (ahead, behind) commit counts between local and remote.
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
    pub fn ahead_behind_by_oid(
        repo: &gix::Repository,
        local_oid: &str,
        remote_oid: &str,
    ) -> Result<(usize, usize), GixReaderError> {
        Self::ahead_behind(repo, Self::parse_oid(local_oid)?, Self::parse_oid(remote_oid)?)
    }

    /// Compute a tree-to-tree diff with rename detection.
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

    /// Read blob content as UTF-8 text, or None if binary.
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

    /// Get blob size without reading full content.
    pub fn blob_size(
        repo: &gix::Repository,
        blob_id: gix::ObjectId,
    ) -> Result<usize, GixReaderError> {
        let header = repo
            .find_header(blob_id)
            .map_err(|e| GixReaderError::ObjectNotFound(format!("blob {blob_id}: {e}")))?;
        Ok(header.size() as usize)
    }

    /// Get HEAD information: branch name and commit OID.
    ///
    /// Returns the branch name (or "HEAD" if detached) and the commit OID.
    pub fn head_info(path: &Path) -> Result<HeadInfo, GixReaderError> {
        let repo = Self::open(path)?;

        let branch = repo
            .head_name()?
            .map(|name| name.shorten().to_string())
            .unwrap_or_else(|| "HEAD".to_string());

        let oid = repo
            .head_id()
            .map_err(|_| GixReaderError::DetachedHeadNoTarget)?
            .to_string();

        Ok(HeadInfo { branch, oid })
    }

    /// Find a branch by name, trying local then remote.
    ///
    /// Returns (full reference name, branch type).
    pub fn find_branch(
        path: &Path,
        branch_name: &str,
    ) -> Result<(String, BranchType), GixReaderError> {
        let repo = Self::open(path)?;

        let local_ref = format!("refs/heads/{branch_name}");
        if repo.find_reference(&local_ref).is_ok() {
            return Ok((local_ref, BranchType::Local));
        }

        let remote_ref = format!("refs/remotes/{branch_name}");
        if repo.find_reference(&remote_ref).is_ok() {
            return Ok((remote_ref, BranchType::Remote));
        }

        Err(GixReaderError::ReferenceNotFound(branch_name.to_string()))
    }

    /// Get the branch type (local or remote) for a branch name.
    pub fn branch_type(path: &Path, branch_name: &str) -> Result<BranchType, GixReaderError> {
        Ok(Self::find_branch(path, branch_name)?.1)
    }

    /// Get the commit OID for a branch as a hex string.
    pub fn branch_oid(path: &Path, branch_name: &str) -> Result<String, GixReaderError> {
        let repo = Self::open(path)?;
        let (ref_name, _) = Self::find_branch(path, branch_name)?;

        let reference = repo.find_reference(&ref_name)?;
        let oid = reference
            .into_fully_peeled_id()
            .map_err(|e| GixReaderError::ReferenceNotFound(format!("Failed to peel: {}", e)))?;

        Ok(oid.to_string())
    }

    /// Fetch from a remote by name with a specific refspec.
    ///
    /// Uses gix's native fetch with automatic credential helper support.
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

    fn fetch_with_remote<'repo>(
        remote: gix::Remote<'repo>,
        refspec: &str,
    ) -> Result<(), GixReaderError> {
        let remote = remote
            .with_refspecs(Some(refspec), Direction::Fetch)
            .map_err(|e| GixReaderError::InvalidRefspec(e.to_string()))?;

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

    /// Get worktree status: counts of uncommitted tracked changes and untracked files.
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

    pub fn commit_message(
        repo: &gix::Repository,
        oid: gix::ObjectId,
    ) -> Result<String, GixReaderError> {
        let commit = repo
            .find_commit(oid)
            .map_err(|e| GixReaderError::ObjectNotFound(format!("Commit {oid}: {e}")))?;
        let message = commit
            .message_raw()
            .map_err(|e| GixReaderError::Diff(format!("Failed to decode commit message: {e}")))?
            .to_string();
        Ok(message)
    }

    /// Get the commit message by hex OID string.
    pub fn commit_message_by_oid(
        repo: &gix::Repository,
        oid_str: &str,
    ) -> Result<String, GixReaderError> {
        Self::commit_message(repo, Self::parse_oid(oid_str)?)
    }

    /// Check if `ancestor` is reachable from `descendant`.
    pub fn is_ancestor(
        repo: &gix::Repository,
        ancestor: gix::ObjectId,
        descendant: gix::ObjectId,
    ) -> Result<bool, GixReaderError> {
        // If they're the same, ancestor is trivially reachable
        if ancestor == descendant {
            return Ok(true);
        }

        // Walk from descendant looking for ancestor
        let walk = repo.rev_walk([descendant]);
        for info_result in walk.all()? {
            let info = info_result?;
            if info.id == ancestor {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Check ancestry by hex OID strings.
    pub fn is_ancestor_by_oid(
        repo: &gix::Repository,
        ancestor_oid: &str,
        descendant_oid: &str,
    ) -> Result<bool, GixReaderError> {
        Self::is_ancestor(repo, Self::parse_oid(ancestor_oid)?, Self::parse_oid(descendant_oid)?)
    }

    /// Find the merge base (common ancestor) of two commits.
    pub fn merge_base(
        repo: &gix::Repository,
        a: gix::ObjectId,
        b: gix::ObjectId,
    ) -> Result<gix::ObjectId, GixReaderError> {
        let base: gix::ObjectId = repo.merge_base(a, b)?.into();
        Ok(base)
    }

    /// Find merge base by hex OID strings.
    pub fn merge_base_by_oid(
        repo: &gix::Repository,
        a_oid: &str,
        b_oid: &str,
    ) -> Result<gix::ObjectId, GixReaderError> {
        Self::merge_base(repo, Self::parse_oid(a_oid)?, Self::parse_oid(b_oid)?)
    }

    /// Collect file statistics from recent commits (change count and last modified time).
    pub fn recent_file_stats(
        repo: &mut gix::Repository,
        commit_limit: usize,
    ) -> Result<HashMap<String, FileStat>, GixReaderError> {
        let mut stats: HashMap<String, FileStat> = HashMap::new();

        // Get HEAD reference and commit OID
        let head_oid = {
            let mut head = repo
                .head()
                .map_err(|e| GixReaderError::ReferenceNotFound(format!("HEAD: {e}")))?;

            let head_commit = head
                .peel_to_commit()
                .map_err(|e| GixReaderError::ReferenceNotFound(format!("HEAD commit: {e}")))?;
            head_commit.id
        };

        // Set up cache size for tree diffs
        if let Ok(index) = repo.index_or_empty() {
            let cache_size = repo.compute_object_cache_size_for_tree_diffs(&index);
            repo.object_cache_size_if_unset(cache_size);
        }

        // Walk commits from HEAD
        let walk = repo.rev_walk([head_oid]);
        let mut commit_index = 0;

        for info_result in walk.all()? {
            if commit_index >= commit_limit {
                break;
            }

            let info = info_result?;
            let commit = info.object().map_err(|e| {
                GixReaderError::Diff(format!("Failed to get commit object: {e}"))
            })?;

            // Get commit timestamp
            let commit_time = {
                let time = commit.time().map_err(|e| {
                    GixReaderError::Diff(format!("Failed to get commit time: {e}"))
                })?;
                DateTime::from_timestamp(time.seconds, 0).unwrap_or_else(Utc::now)
            };

            // Get the commit tree
            let commit_tree = commit.tree().map_err(|e| {
                GixReaderError::Diff(format!("Failed to get commit tree: {e}"))
            })?;

            // Get parent tree (or empty for root commits)
            let parent_ids: Vec<_> = commit.parent_ids().collect();
            let parent_tree = if parent_ids.is_empty() {
                None
            } else {
                // Use first parent for simplicity (following linear history)
                let parent_id = parent_ids[0];
                let parent_commit = repo.find_commit(parent_id).map_err(|e| {
                    GixReaderError::Diff(format!("Failed to find parent commit: {e}"))
                })?;
                Some(parent_commit.tree().map_err(|e| {
                    GixReaderError::Diff(format!("Failed to get parent tree: {e}"))
                })?)
            };

            // Diff trees to find changed files
            let changes = repo
                .diff_tree_to_tree(parent_tree.as_ref(), Some(&commit_tree), None)
                .map_err(|e| GixReaderError::Diff(format!("Tree diff failed: {e}")))?;

            // Process each change
            for change in changes {
                let path_str = match &change {
                    gix::diff::tree_with_rewrites::Change::Addition { location, .. } => {
                        location.to_string()
                    }
                    gix::diff::tree_with_rewrites::Change::Deletion { location, .. } => {
                        location.to_string()
                    }
                    gix::diff::tree_with_rewrites::Change::Modification { location, .. } => {
                        location.to_string()
                    }
                    gix::diff::tree_with_rewrites::Change::Rewrite { location, .. } => {
                        location.to_string()
                    }
                };

                // Update or insert file stats
                let stat = stats.entry(path_str).or_insert(FileStat {
                    last_index: commit_index,
                    commit_count: 0,
                    last_time: commit_time,
                });

                // Increment commit count
                stat.commit_count += 1;

                // Keep the most recent change (smallest index)
                if commit_index < stat.last_index {
                    stat.last_index = commit_index;
                    stat.last_time = commit_time;
                }
            }

            commit_index += 1;
        }

        Ok(stats)
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

    // ========== Gix Migration Verification Tests ==========
    // These tests compare gix output against git CLI to verify the migration is correct

    /// Helper to get git rev-parse output for comparing OIDs
    fn git_rev_parse(repo_path: &Path, rev: &str) -> String {
        let output = Command::new("git")
            .args(["rev-parse", rev])
            .current_dir(repo_path)
            .output()
            .expect("Failed to run git rev-parse");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// Helper to get git rev-list --count for comparing ahead/behind
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

    /// Helper to get git status --porcelain counts for comparing worktree status
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

    #[test]
    fn test_gix_head_info_matches_git_cli() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();
        init_test_repo_via_cli(repo_path);

        // Get results from both gix and git CLI
        let gix_info = GixReader::head_info(repo_path).unwrap();
        let git_oid = git_rev_parse(repo_path, "HEAD");
        let git_branch = {
            let output = Command::new("git")
                .args(["branch", "--show-current"])
                .current_dir(repo_path)
                .output()
                .expect("Failed to get branch");
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        };

        assert_eq!(
            gix_info.oid, git_oid,
            "Gix OID {} should match git CLI OID {}",
            gix_info.oid, git_oid
        );
        assert_eq!(
            gix_info.branch, git_branch,
            "Gix branch {} should match git CLI branch {}",
            gix_info.branch, git_branch
        );
    }

    #[test]
    fn test_gix_ahead_behind_matches_git_cli() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();
        init_test_repo_via_cli(repo_path);

        // Create a branch with some commits ahead of main
        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(repo_path)
            .output()
            .expect("Failed to create feature branch");

        // Add 3 commits on feature
        for i in 1..=3 {
            fs::write(repo_path.join(format!("feature{}.txt", i)), format!("content{}", i))
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

        // Switch to main and add 2 commits
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

        // Get OIDs for both branches
        let feature_oid = git_rev_parse(repo_path, "feature");
        let main_oid = git_rev_parse(repo_path, "main");

        // Get ahead/behind from git CLI
        let git_ahead = git_rev_list_count(repo_path, &format!("{}..{}", main_oid, feature_oid));
        let git_behind = git_rev_list_count(repo_path, &format!("{}..{}", feature_oid, main_oid));

        // Get ahead/behind from gix
        let repo = GixReader::open(repo_path).unwrap();
        let (gix_ahead, gix_behind) =
            GixReader::ahead_behind_by_oid(&repo, &feature_oid, &main_oid).unwrap();

        assert_eq!(
            gix_ahead, git_ahead,
            "Gix ahead {} should match git CLI ahead {}",
            gix_ahead, git_ahead
        );
        assert_eq!(
            gix_behind, git_behind,
            "Gix behind {} should match git CLI behind {}",
            gix_behind, git_behind
        );

        // Verify the expected values (3 ahead, 2 behind)
        assert_eq!(gix_ahead, 3, "Feature should be 3 commits ahead of main");
        assert_eq!(gix_behind, 2, "Feature should be 2 commits behind main");
    }

    #[test]
    fn test_gix_ahead_behind_same_commit() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();
        init_test_repo_via_cli(repo_path);

        let oid = git_rev_parse(repo_path, "HEAD");
        let repo = GixReader::open(repo_path).unwrap();
        let (ahead, behind) = GixReader::ahead_behind_by_oid(&repo, &oid, &oid).unwrap();

        assert_eq!(ahead, 0);
        assert_eq!(behind, 0);
    }

    #[test]
    fn test_gix_worktree_status_matches_git_cli() {
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

        // Now create a mixed state:
        // - Modify tracked file (1 uncommitted)
        // - Add untracked file (1 untracked)
        // - Stage a new file (1 uncommitted)
        fs::write(repo_path.join("tracked.txt"), "modified").unwrap();
        fs::write(repo_path.join("untracked.txt"), "untracked").unwrap();
        fs::write(repo_path.join("staged.txt"), "staged").unwrap();
        Command::new("git")
            .args(["add", "staged.txt"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Get counts from both gix and git CLI
        let gix_summary = GixReader::get_worktree_status(repo_path).unwrap();
        let (git_uncommitted, git_untracked) = git_status_counts(repo_path);

        assert_eq!(
            gix_summary.uncommitted_tracked, git_uncommitted,
            "Gix uncommitted {} should match git CLI uncommitted {}",
            gix_summary.uncommitted_tracked, git_uncommitted
        );
        assert_eq!(
            gix_summary.untracked, git_untracked,
            "Gix untracked {} should match git CLI untracked {}",
            gix_summary.untracked, git_untracked
        );

        // Verify expected values (2 uncommitted: modified + staged, 1 untracked)
        assert_eq!(
            gix_summary.uncommitted_tracked, 2,
            "Should have 2 uncommitted (modified + staged)"
        );
        assert_eq!(gix_summary.untracked, 1, "Should have 1 untracked");
    }

    #[test]
    fn test_gix_worktree_status_in_worktree() {
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

        // Get counts from both gix and git CLI
        let gix_summary = GixReader::get_worktree_status(&worktree_path).unwrap();
        let (git_uncommitted, git_untracked) = git_status_counts(&worktree_path);

        assert_eq!(
            gix_summary.uncommitted_tracked, git_uncommitted,
            "Gix uncommitted in worktree should match git CLI"
        );
        assert_eq!(
            gix_summary.untracked, git_untracked,
            "Gix untracked in worktree should match git CLI"
        );
    }

    #[test]
    fn test_gix_branch_oid_matches_git_cli() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();
        init_test_repo_via_cli(repo_path);

        // Create additional branch
        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(repo_path)
            .output()
            .unwrap();
        fs::write(repo_path.join("file.txt"), "content").unwrap();
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

        // Compare gix vs git CLI for both branches
        let gix_main_oid = GixReader::branch_oid(repo_path, "main").unwrap();
        let git_main_oid = git_rev_parse(repo_path, "main");
        assert_eq!(gix_main_oid, git_main_oid, "Main branch OID should match");

        let gix_feature_oid = GixReader::branch_oid(repo_path, "feature").unwrap();
        let git_feature_oid = git_rev_parse(repo_path, "feature");
        assert_eq!(
            gix_feature_oid, git_feature_oid,
            "Feature branch OID should match"
        );
    }

    /// Benchmark: measure gix performance vs git CLI for branchStatus operations
    /// This test demonstrates gix is faster than spawning git CLI processes.
    /// Run with: cargo nextest run --package services test_gix_performance_vs_cli --no-capture
    #[test]
    fn test_gix_performance_vs_cli() {
        use std::time::Instant;

        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();
        init_test_repo_via_cli(repo_path);

        // Create a repo with some history
        for i in 1..=10 {
            fs::write(repo_path.join(format!("file{}.txt", i)), format!("content{}", i)).unwrap();
            Command::new("git")
                .args(["add", "."])
                .current_dir(repo_path)
                .output()
                .unwrap();
            Command::new("git")
                .args(["commit", "-m", &format!("Commit {}", i)])
                .current_dir(repo_path)
                .output()
                .unwrap();
        }

        // Create a diverged branch
        Command::new("git")
            .args(["checkout", "-b", "feature", "HEAD~5"])
            .current_dir(repo_path)
            .output()
            .unwrap();
        for i in 1..=3 {
            fs::write(
                repo_path.join(format!("feature{}.txt", i)),
                format!("feature{}", i),
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

        // Add some untracked and modified files
        fs::write(repo_path.join("untracked1.txt"), "untracked").unwrap();
        fs::write(repo_path.join("untracked2.txt"), "untracked").unwrap();
        fs::write(repo_path.join("file1.txt"), "modified").unwrap();

        let iterations = 100;

        // Benchmark git CLI - typical branchStatus operations
        let cli_start = Instant::now();
        for _ in 0..iterations {
            // git rev-parse HEAD
            let _ = Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(repo_path)
                .output()
                .unwrap();
            // git branch --show-current
            let _ = Command::new("git")
                .args(["branch", "--show-current"])
                .current_dir(repo_path)
                .output()
                .unwrap();
            // git status --porcelain
            let _ = Command::new("git")
                .args(["status", "--porcelain"])
                .current_dir(repo_path)
                .output()
                .unwrap();
            // git rev-list --count main..feature
            let _ = Command::new("git")
                .args(["rev-list", "--count", "main..feature"])
                .current_dir(repo_path)
                .output()
                .unwrap();
        }
        let cli_duration = cli_start.elapsed();
        let cli_per_op = cli_duration / iterations;

        // Benchmark gix - same operations
        let gix_start = Instant::now();
        for _ in 0..iterations {
            // head_info (combines rev-parse HEAD + branch --show-current)
            let _ = GixReader::head_info(repo_path).unwrap();
            // get_worktree_status (combines status --porcelain counts)
            let _ = GixReader::get_worktree_status(repo_path).unwrap();
            // ahead_behind (combines rev-list --count in both directions)
            let repo = GixReader::open(repo_path).unwrap();
            let main_oid = GixReader::branch_oid(repo_path, "main").unwrap();
            let feature_oid = GixReader::branch_oid(repo_path, "feature").unwrap();
            let _ = GixReader::ahead_behind_by_oid(&repo, &feature_oid, &main_oid).unwrap();
        }
        let gix_duration = gix_start.elapsed();
        let gix_per_op = gix_duration / iterations;

        // Calculate speedup
        let speedup = cli_duration.as_secs_f64() / gix_duration.as_secs_f64();

        println!("\n=== Gix vs Git CLI Performance Comparison ===");
        println!("Operations per iteration: head_info + worktree_status + ahead_behind");
        println!("Iterations: {}", iterations);
        println!("Git CLI total: {:?} ({:?} per iteration)", cli_duration, cli_per_op);
        println!("Gix total:     {:?} ({:?} per iteration)", gix_duration, gix_per_op);
        println!("Speedup: {:.2}x faster", speedup);
        println!("==============================================\n");

        // Gix should be faster (conservative threshold to avoid flaky tests)
        // Measured: ~1.9x faster on typical hardware (3.6ms vs 6.8ms per op)
        assert!(
            speedup > 1.5,
            "Gix should be at least 1.5x faster than git CLI, but was only {:.2}x",
            speedup
        );
    }
}
