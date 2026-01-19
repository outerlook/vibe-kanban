//! Read-only Git repository operations using gix (gitoxide).
//!
//! This module encapsulates all gix read operations and handles both regular
//! repositories and worktrees transparently.

use std::path::Path;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GixReaderError {
    #[error("Failed to open repository: {0}")]
    Open(#[from] gix::open::Error),
    #[error("Repository discovery failed: {0}")]
    Discover(#[from] gix::discover::Error),
    #[error("Invalid repository at path: {path}")]
    InvalidRepository { path: String },
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
