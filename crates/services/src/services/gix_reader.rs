//! Read-only Git repository operations using gix (gitoxide).
//!
//! This module encapsulates all gix read operations and handles both regular
//! repositories and worktrees transparently.

use std::path::Path;

use gix::bstr::BStr;
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
    #[error("Failed to peel reference: {0}")]
    PeelError(#[from] gix::reference::peel::Error),
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
        use gix::bstr::ByteSlice;
        use gix::object::tree::diff::ChangeDetached;
        use gix::object::tree::EntryKind;

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
            let dominated_by_blob =
                matches!(entry.new_mode, Some(EntryKind::Blob | EntryKind::BlobExecutable))
                    || matches!(
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
        let tree_id = commit.tree_id().map_err(|e| {
            GixReaderError::InvalidObject(format!("commit has no tree: {e}"))
        })?;

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
    pub fn blob_size(repo: &gix::Repository, blob_id: gix::ObjectId) -> Result<usize, GixReaderError> {
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
    pub fn is_blob_binary(repo: &gix::Repository, blob_id: gix::ObjectId) -> Result<bool, GixReaderError> {
        let blob = repo
            .find_object(blob_id)
            .map_err(|e| GixReaderError::ObjectNotFound(format!("blob {blob_id}: {e}")))?;

        Ok(blob.data.as_slice().contains(&0))
    }
}

/// Convert BStr to String, handling non-UTF8 paths gracefully
fn bstr_to_string(bstr: &BStr) -> String {
    String::from_utf8_lossy(bstr).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

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
}
