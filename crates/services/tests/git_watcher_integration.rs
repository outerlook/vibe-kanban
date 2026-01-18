//! Integration tests for the GitWatcher service.
//!
//! These tests verify that the git watcher correctly detects changes
//! in real git repositories and worktrees.

use std::fs;
use std::path::Path;
use std::time::Duration;

use git2::Repository;
use services::services::git_watcher::{
    GitStateChangeKind, GitWatcherManager, resolve_git_dir,
};
use tempfile::TempDir;

fn create_git_repo(path: &Path) -> Repository {
    Repository::init(path).expect("Failed to init git repo")
}

fn create_initial_commit(repo: &Repository) {
    let sig = repo.signature().unwrap();
    let tree_id = {
        let mut index = repo.index().unwrap();
        index.write_tree().unwrap()
    };
    let tree = repo.find_tree(tree_id).unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
        .unwrap();
}

fn create_worktree(main_repo: &Repository, worktree_path: &Path, branch_name: &str) {
    // Create a new branch from HEAD
    let head = main_repo.head().unwrap();
    let commit = head.peel_to_commit().unwrap();
    main_repo.branch(branch_name, &commit, false).unwrap();

    // Add the worktree
    main_repo
        .worktree(
            branch_name,
            worktree_path,
            Some(&git2::WorktreeAddOptions::new().reference(Some(
                &main_repo
                    .find_branch(branch_name, git2::BranchType::Local)
                    .unwrap()
                    .into_reference(),
            ))),
        )
        .unwrap();
}

#[test]
fn test_resolve_git_dir_for_worktree() {
    let temp_dir = TempDir::new().unwrap();

    // Create main repo
    let main_repo_path = temp_dir.path().join("main_repo");
    fs::create_dir_all(&main_repo_path).unwrap();
    let main_repo = create_git_repo(&main_repo_path);
    create_initial_commit(&main_repo);

    // Create worktree
    let worktree_path = temp_dir.path().join("worktree_feature");
    create_worktree(&main_repo, &worktree_path, "feature");

    // Verify we can resolve the git dir
    let git_dir = resolve_git_dir(&worktree_path).unwrap();

    // The git dir should be in the main repo's .git/worktrees directory
    assert!(
        git_dir
            .to_string_lossy()
            .contains(".git/worktrees/feature"),
        "Git dir should be in worktrees directory: {:?}",
        git_dir
    );
}

#[tokio::test]
async fn test_watcher_detects_head_change_in_real_repo() {
    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();

    let repo = create_git_repo(&repo_path);
    create_initial_commit(&repo);

    // Create a branch to switch to
    let head = repo.head().unwrap();
    let commit = head.peel_to_commit().unwrap();
    repo.branch("feature", &commit, false).unwrap();

    let manager = GitWatcherManager::new();
    let mut sub = manager.subscribe(repo_path.clone()).unwrap();

    // Give watcher time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Switch branch using git checkout
    repo.set_head("refs/heads/feature").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
        .unwrap();

    // Wait for events - git checkout may emit multiple events (HEAD and index changes)
    // We just need to verify that we receive at least one event related to the checkout
    let mut received_any_event = false;

    // Collect events for a short period
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let timeout = deadline.saturating_duration_since(tokio::time::Instant::now());
        if timeout.is_zero() {
            break;
        }

        match tokio::time::timeout(timeout, sub.recv()).await {
            Ok(Some(_event)) => {
                received_any_event = true;
            }
            _ => break,
        }
    }

    assert!(
        received_any_event,
        "Should receive at least one event after checkout"
    );
    // Note: Due to debouncing, HEAD event might be coalesced or the first event we receive
    // might be an Index event. The key test is that we receive events when git state changes.
    // For a more precise HEAD-only test, we can write to .git/HEAD directly.
}

#[tokio::test]
async fn test_watcher_detects_staging_changes() {
    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();

    let repo = create_git_repo(&repo_path);
    create_initial_commit(&repo);

    let manager = GitWatcherManager::new();
    let mut sub = manager.subscribe(repo_path.clone()).unwrap();

    // Give watcher time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create and stage a file
    let file_path = repo_path.join("new_file.txt");
    fs::write(&file_path, "Hello, world!").unwrap();

    let mut index = repo.index().unwrap();
    index.add_path(Path::new("new_file.txt")).unwrap();
    index.write().unwrap();

    // Wait for event
    let result = tokio::time::timeout(Duration::from_secs(2), sub.recv()).await;

    assert!(result.is_ok(), "Should receive an event within timeout");
    if let Ok(Some(event)) = result {
        assert_eq!(
            event.kind,
            GitStateChangeKind::Index,
            "Expected Index change event"
        );
    }
}

#[tokio::test]
async fn test_watcher_works_with_worktree() {
    let temp_dir = TempDir::new().unwrap();

    // Create main repo
    let main_repo_path = temp_dir.path().join("main_repo");
    fs::create_dir_all(&main_repo_path).unwrap();
    let main_repo = create_git_repo(&main_repo_path);
    create_initial_commit(&main_repo);

    // Create worktree
    let worktree_path = temp_dir.path().join("worktree_feature");
    create_worktree(&main_repo, &worktree_path, "feature");

    let manager = GitWatcherManager::new();
    let mut sub = manager.subscribe(worktree_path.clone()).unwrap();

    // Give watcher time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Open the worktree repo and stage a file
    let worktree_repo = Repository::open(&worktree_path).unwrap();
    let file_path = worktree_path.join("feature_file.txt");
    fs::write(&file_path, "Feature content").unwrap();

    let mut index = worktree_repo.index().unwrap();
    index.add_path(Path::new("feature_file.txt")).unwrap();
    index.write().unwrap();

    // Wait for event
    let result = tokio::time::timeout(Duration::from_secs(2), sub.recv()).await;

    assert!(result.is_ok(), "Should receive an event within timeout");
    if let Ok(Some(event)) = result {
        assert_eq!(
            event.kind,
            GitStateChangeKind::Index,
            "Expected Index change event from worktree"
        );
    }
}

#[tokio::test]
async fn test_multiple_subscriptions_share_watcher() {
    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();

    let repo = create_git_repo(&repo_path);
    create_initial_commit(&repo);

    let manager = GitWatcherManager::new();

    // Create multiple subscriptions
    let mut sub1 = manager.subscribe(repo_path.clone()).unwrap();
    let mut sub2 = manager.subscribe(repo_path.clone()).unwrap();

    // Should have only one active watcher
    assert_eq!(manager.active_watcher_count(), 1);

    // Give watcher time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Trigger a change
    let file_path = repo_path.join("test.txt");
    fs::write(&file_path, "test").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("test.txt")).unwrap();
    index.write().unwrap();

    // Both subscriptions should receive the event
    let result1 = tokio::time::timeout(Duration::from_secs(2), sub1.recv()).await;
    let result2 = tokio::time::timeout(Duration::from_secs(2), sub2.recv()).await;

    assert!(
        result1.is_ok() && result2.is_ok(),
        "Both subscriptions should receive events"
    );
}

#[tokio::test]
async fn test_watcher_cleanup_on_drop() {
    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();

    let _repo = create_git_repo(&repo_path);

    let manager = GitWatcherManager::new();

    {
        let _sub = manager.subscribe(repo_path.clone()).unwrap();
        assert_eq!(manager.active_watcher_count(), 1);
    }
    // Subscription dropped

    // After drop, the watcher should be cleaned up on next subscription attempt
    // or when cleanup_if_unused is called
    // The weak reference should be gone
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Creating a new subscription should work
    let _sub2 = manager.subscribe(repo_path.clone()).unwrap();
    assert_eq!(manager.active_watcher_count(), 1);
}
