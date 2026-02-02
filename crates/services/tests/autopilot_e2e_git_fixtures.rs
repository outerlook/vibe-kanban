//! Test fixtures for autopilot E2E tests.
//!
//! Provides `TestRepo` for creating isolated git repositories with worktree support.

use std::path::{Path, PathBuf};

use git2::Repository;
use tempfile::TempDir;

/// Configures git user.name and user.email for a repository.
pub fn configure_git_user(repo_path: &Path) {
    let repo = Repository::open(repo_path).expect("Failed to open repository");
    let mut config = repo.config().expect("Failed to get repo config");
    config
        .set_str("user.name", "Test User")
        .expect("Failed to set user.name");
    config
        .set_str("user.email", "test@example.com")
        .expect("Failed to set user.email");
}

/// A test git repository with worktree support.
///
/// Creates an isolated git repository in a temporary directory with an initial commit.
/// The repository is automatically cleaned up when dropped.
pub struct TestRepo {
    pub path: PathBuf,
    pub name: String,
    _dir: TempDir,
}

impl TestRepo {
    /// Creates a new test repository with the given name.
    ///
    /// The repository is initialized with:
    /// - A configured git user (for commits to work)
    /// - An initial commit with a README.md file
    /// - The default branch set to "main"
    pub fn new(name: &str) -> Self {
        let dir = TempDir::new().expect("Failed to create temp dir");
        let path = dir.path().join(name);

        // Initialize repository
        let repo = Repository::init(&path).expect("Failed to init git repo");

        // Configure git user
        {
            let mut config = repo.config().expect("Failed to get repo config");
            config
                .set_str("user.name", "Test User")
                .expect("Failed to set user.name");
            config
                .set_str("user.email", "test@example.com")
                .expect("Failed to set user.email");
        }

        // Create initial commit
        let sig = repo.signature().expect("Failed to get signature");

        // Create README.md
        let readme_path = path.join("README.md");
        std::fs::write(&readme_path, "# Test Repository\n").expect("Failed to write README.md");

        // Stage and commit
        let mut index = repo.index().expect("Failed to get index");
        index
            .add_path(Path::new("README.md"))
            .expect("Failed to add README.md to index");
        index.write().expect("Failed to write index");

        let tree_id = index.write_tree().expect("Failed to write tree");
        let tree = repo.find_tree(tree_id).expect("Failed to find tree");

        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .expect("Failed to create initial commit");

        // Set default branch to main
        repo.set_head("refs/heads/master")
            .expect("Failed to set HEAD");

        // Rename master to main
        let mut master = repo
            .find_branch("master", git2::BranchType::Local)
            .expect("Failed to find master branch");
        master.rename("main", false).expect("Failed to rename to main");

        repo.set_head("refs/heads/main")
            .expect("Failed to set HEAD to main");

        Self {
            path,
            name: name.to_string(),
            _dir: dir,
        }
    }

    /// Creates a new branch at HEAD.
    pub fn create_branch(&self, branch: &str) {
        let repo = Repository::open(&self.path).expect("Failed to open repository");
        let head = repo
            .head()
            .expect("Failed to get HEAD")
            .peel_to_commit()
            .expect("Failed to peel to commit");
        repo.branch(branch, &head, false)
            .expect("Failed to create branch");
    }

    /// Creates a git worktree for the given branch.
    ///
    /// Returns the path to the worktree directory.
    pub fn create_worktree(&self, branch: &str) -> PathBuf {
        let repo = Repository::open(&self.path).expect("Failed to open repository");

        // Ensure branch exists
        if repo.find_branch(branch, git2::BranchType::Local).is_err() {
            self.create_branch(branch);
        }

        let branch_ref = repo
            .find_branch(branch, git2::BranchType::Local)
            .expect("Failed to find branch")
            .into_reference();

        // Create worktree path (sibling to main repo)
        let worktree_path = self
            .path
            .parent()
            .expect("Repo should have parent")
            .join(format!("{}-worktree-{}", self.name, branch));

        repo.worktree(
            branch,
            &worktree_path,
            Some(&git2::WorktreeAddOptions::new().reference(Some(&branch_ref))),
        )
        .expect("Failed to create worktree");

        worktree_path
    }
}

#[test]
fn test_repo_creates_valid_git_repo() {
    let repo = TestRepo::new("test-repo");

    // Verify .git exists
    assert!(
        repo.path.join(".git").exists(),
        ".git directory should exist"
    );

    // Verify initial commit is present
    let git_repo = Repository::open(&repo.path).expect("Failed to open repository");
    let head = git_repo.head().expect("Failed to get HEAD");
    let commit = head.peel_to_commit().expect("Failed to peel to commit");

    assert_eq!(commit.message(), Some("Initial commit"));

    // Verify branch is main
    let branch_name = head.shorthand().expect("Failed to get branch name");
    assert_eq!(branch_name, "main");

    // Verify README.md exists
    assert!(
        repo.path.join("README.md").exists(),
        "README.md should exist"
    );
}

#[test]
fn test_repo_creates_worktree() {
    let repo = TestRepo::new("worktree-test");

    // Create a worktree for a feature branch
    let worktree_path = repo.create_worktree("feature-branch");

    // Verify worktree exists
    assert!(worktree_path.exists(), "Worktree path should exist");

    // Verify it's a valid git worktree (has .git file pointing to main repo)
    let git_file = worktree_path.join(".git");
    assert!(git_file.exists(), ".git file should exist in worktree");

    // The .git should be a file (not directory) for worktrees
    assert!(git_file.is_file(), ".git should be a file in worktree");

    // Verify we can open it as a repository
    let worktree_repo = Repository::open(&worktree_path).expect("Failed to open worktree");

    // Verify it's on the correct branch
    let head = worktree_repo.head().expect("Failed to get HEAD");
    let branch_name = head.shorthand().expect("Failed to get branch name");
    assert_eq!(branch_name, "feature-branch");
}
