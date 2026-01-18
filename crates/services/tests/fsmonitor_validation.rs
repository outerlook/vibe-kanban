use std::path::Path;

use git2::Repository;
use services::services::{
    git::GitService,
    worktree_manager::{WorktreeError, WorktreeManager},
};
use tempfile::TempDir;

fn configure_user(repo: &Repository) {
    let mut cfg = repo.config().unwrap();
    cfg.set_str("user.name", "Test User").unwrap();
    cfg.set_str("user.email", "test@example.com").unwrap();
}

fn set_fsmonitor(repo_path: &Path, enabled: bool) {
    let repo = Repository::open(repo_path).unwrap();
    let mut cfg = repo.config().unwrap();
    cfg.set_bool("core.fsmonitor", enabled).unwrap();
}

fn set_fsmonitor_string(repo_path: &Path, value: &str) {
    let repo = Repository::open(repo_path).unwrap();
    let mut cfg = repo.config().unwrap();
    cfg.set_str("core.fsmonitor", value).unwrap();
}

fn create_test_repo(root: &TempDir) -> std::path::PathBuf {
    let repo_path = root.path().join("repo");

    let service = GitService::new();
    service
        .initialize_repo_with_main_branch(&repo_path)
        .expect("init repo");

    let repo = Repository::open(&repo_path).unwrap();
    configure_user(&repo);

    repo_path
}

#[test]
fn test_fsmonitor_validation_fails_when_disabled() {
    let root = TempDir::new().unwrap();
    let repo_path = create_test_repo(&root);

    // By default, fsmonitor is not set
    let result = WorktreeManager::validate_fsmonitor(&repo_path);
    assert!(result.is_err());

    match result {
        Err(WorktreeError::FsmonitorRequired(name)) => {
            assert_eq!(name, "repo");
        }
        _ => panic!("Expected FsmonitorRequired error"),
    }
}

#[test]
fn test_fsmonitor_validation_fails_when_explicitly_false() {
    let root = TempDir::new().unwrap();
    let repo_path = create_test_repo(&root);

    set_fsmonitor(&repo_path, false);

    let result = WorktreeManager::validate_fsmonitor(&repo_path);
    assert!(result.is_err());

    match result {
        Err(WorktreeError::FsmonitorRequired(name)) => {
            assert_eq!(name, "repo");
        }
        _ => panic!("Expected FsmonitorRequired error"),
    }
}

#[test]
fn test_fsmonitor_validation_succeeds_when_enabled() {
    let root = TempDir::new().unwrap();
    let repo_path = create_test_repo(&root);

    set_fsmonitor(&repo_path, true);

    let result = WorktreeManager::validate_fsmonitor(&repo_path);
    assert!(result.is_ok());
}

#[test]
fn test_fsmonitor_validation_succeeds_with_hook_path() {
    let root = TempDir::new().unwrap();
    let repo_path = create_test_repo(&root);

    // fsmonitor can also be a path to a hook script
    set_fsmonitor_string(&repo_path, "/path/to/fsmonitor-hook");

    let result = WorktreeManager::validate_fsmonitor(&repo_path);
    assert!(result.is_ok());
}

#[test]
fn test_fsmonitor_validation_fails_with_empty_string() {
    let root = TempDir::new().unwrap();
    let repo_path = create_test_repo(&root);

    set_fsmonitor_string(&repo_path, "");

    let result = WorktreeManager::validate_fsmonitor(&repo_path);
    assert!(result.is_err());

    match result {
        Err(WorktreeError::FsmonitorRequired(_)) => {}
        _ => panic!("Expected FsmonitorRequired error"),
    }
}

#[test]
fn test_fsmonitor_error_message_contains_fix_instruction() {
    let root = TempDir::new().unwrap();
    let repo_path = create_test_repo(&root);

    let result = WorktreeManager::validate_fsmonitor(&repo_path);
    let err = result.unwrap_err();
    let error_msg = err.to_string();

    assert!(
        error_msg.contains("git config core.fsmonitor true"),
        "Error message should contain fix instruction, got: {}",
        error_msg
    );
}

#[tokio::test]
async fn test_fsmonitor_validation_async_works() {
    let root = TempDir::new().unwrap();
    let repo_path = create_test_repo(&root);

    // Test with fsmonitor disabled
    let result = WorktreeManager::validate_fsmonitor_async(&repo_path).await;
    assert!(result.is_err());

    // Enable fsmonitor
    set_fsmonitor(&repo_path, true);

    // Test with fsmonitor enabled
    let result = WorktreeManager::validate_fsmonitor_async(&repo_path).await;
    assert!(result.is_ok());
}
