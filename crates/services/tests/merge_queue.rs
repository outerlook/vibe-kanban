//! Integration tests for the merge queue feature.
//!
//! Tests verify:
//! - FIFO processing order
//! - Conflict handling and status updates
//! - Queue cancellation
//! - End-to-end merge flow with Merge record creation

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

use db::models::{
    merge_queue::{MergeQueue, MergeQueueStatus},
    task::{Task, TaskStatus},
};
use git2::Repository;
use services::services::{
    git::GitService,
    merge_queue_processor::{MergeQueueError, MergeQueueProcessor},
};
use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};
use tempfile::TempDir;
use uuid::Uuid;

/// Creates an in-memory SQLite database and runs all migrations.
async fn create_test_db() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("Failed to create in-memory database");

    sqlx::migrate!("../db/migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    pool
}

/// Creates a test project in the database.
async fn create_test_project(pool: &SqlitePool, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO projects (id, name) VALUES (?, ?)")
        .bind(id)
        .bind(name)
        .execute(pool)
        .await
        .expect("Failed to create project");
    id
}

/// Creates a test task in the database.
async fn create_test_task(pool: &SqlitePool, project_id: Uuid, title: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO tasks (id, project_id, title, status) VALUES (?, ?, ?, 'inprogress')")
        .bind(id)
        .bind(project_id)
        .bind(title)
        .execute(pool)
        .await
        .expect("Failed to create task");
    id
}

/// Creates a test repo in the database.
async fn create_test_repo(pool: &SqlitePool, path: &Path) -> Uuid {
    let id = Uuid::new_v4();
    let path_str = path.to_string_lossy().to_string();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "test-repo".to_string());
    sqlx::query("INSERT INTO repos (id, path, name, display_name) VALUES (?, ?, ?, ?)")
        .bind(id)
        .bind(&path_str)
        .bind(&name)
        .bind(&name)
        .execute(pool)
        .await
        .expect("Failed to create repo");
    id
}

/// Creates a test workspace in the database.
async fn create_test_workspace(
    pool: &SqlitePool,
    task_id: Uuid,
    branch: &str,
    container_ref: Option<&str>,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id, task_id, branch, container_ref) VALUES (?, ?, ?, ?)")
        .bind(id)
        .bind(task_id)
        .bind(branch)
        .bind(container_ref)
        .execute(pool)
        .await
        .expect("Failed to create workspace");
    id
}

/// Creates a workspace_repo association.
async fn create_workspace_repo(
    pool: &SqlitePool,
    workspace_id: Uuid,
    repo_id: Uuid,
    target_branch: &str,
) {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspace_repos (id, workspace_id, repo_id, target_branch) VALUES (?, ?, ?, ?)",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(repo_id)
    .bind(target_branch)
    .execute(pool)
    .await
    .expect("Failed to create workspace_repo");
}

fn write_file<P: AsRef<Path>>(base: P, rel: &str, content: &str) {
    let path = base.as_ref().join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mut f = fs::File::create(&path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

fn configure_git_user(repo_path: &Path, name: &str, email: &str) {
    let repo = Repository::open(repo_path).unwrap();
    let mut cfg = repo.config().unwrap();
    cfg.set_str("user.name", name).unwrap();
    cfg.set_str("user.email", email).unwrap();
}

fn init_repo_main(root: &TempDir) -> PathBuf {
    let path = root.path().join("repo");
    let s = GitService::new();
    s.initialize_repo_with_main_branch(&path).unwrap();
    configure_git_user(&path, "Test User", "test@example.com");
    checkout_branch(&path, "main");
    path
}

fn checkout_branch(repo_path: &Path, name: &str) {
    let repo = Repository::open(repo_path).unwrap();
    repo.set_head(&format!("refs/heads/{name}")).unwrap();
    let mut co = git2::build::CheckoutBuilder::new();
    co.force();
    repo.checkout_head(Some(&mut co)).unwrap();
}

fn create_branch(repo_path: &Path, name: &str) {
    let repo = Repository::open(repo_path).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let _ = repo.branch(name, &head, true).unwrap();
}

// ============================================================================
// MergeQueue Database Operation Tests
// ============================================================================

#[tokio::test]
async fn test_merge_queue_create_and_find() {
    let pool = create_test_db().await;

    // Setup: Create project, task, repo, workspace
    let project_id = create_test_project(&pool, "Test Project").await;
    let task_id = create_test_task(&pool, project_id, "Test Task").await;

    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    let repo_id = create_test_repo(&pool, &repo_path).await;
    let workspace_id = create_test_workspace(&pool, task_id, "feature-branch", None).await;

    // Create merge queue entry
    let entry = MergeQueue::create(&pool, project_id, workspace_id, repo_id)
        .await
        .unwrap();

    assert_eq!(entry.project_id, project_id);
    assert_eq!(entry.workspace_id, workspace_id);
    assert_eq!(entry.repo_id, repo_id);
    assert_eq!(entry.status, MergeQueueStatus::Queued);
    assert!(entry.conflict_message.is_none());
    assert!(entry.started_at.is_none());
    assert!(entry.completed_at.is_none());

    // Find by ID
    let found = MergeQueue::find_by_id(&pool, entry.id).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, entry.id);

    // Find by workspace
    let found_by_ws = MergeQueue::find_by_workspace(&pool, workspace_id)
        .await
        .unwrap();
    assert!(found_by_ws.is_some());
    assert_eq!(found_by_ws.unwrap().workspace_id, workspace_id);
}

#[tokio::test]
async fn test_merge_queue_pop_next_fifo_order() {
    let pool = create_test_db().await;

    // Setup: Create project and tasks
    let project_id = create_test_project(&pool, "FIFO Project").await;
    let temp_dir = TempDir::new().unwrap();

    // Create 3 tasks with workspaces and queue them in sequence
    let mut workspace_ids = Vec::new();
    for i in 0..3 {
        let task_id = create_test_task(&pool, project_id, &format!("Task {}", i + 1)).await;
        let repo_path = temp_dir.path().join(format!("repo{}", i));
        fs::create_dir_all(&repo_path).unwrap();
        let repo_id = create_test_repo(&pool, &repo_path).await;
        let workspace_id =
            create_test_workspace(&pool, task_id, &format!("feature-{}", i + 1), None).await;
        workspace_ids.push(workspace_id);

        MergeQueue::create(&pool, project_id, workspace_id, repo_id)
            .await
            .unwrap();

        // Small delay to ensure distinct queued_at timestamps
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // Verify count
    let count = MergeQueue::count_by_project(&pool, project_id)
        .await
        .unwrap();
    assert_eq!(count, 3);

    // Pop entries - should be in FIFO order (oldest first)
    let first = MergeQueue::pop_next(&pool, project_id).await.unwrap();
    assert!(first.is_some());
    assert_eq!(first.unwrap().workspace_id, workspace_ids[0]);

    let second = MergeQueue::pop_next(&pool, project_id).await.unwrap();
    assert!(second.is_some());
    assert_eq!(second.unwrap().workspace_id, workspace_ids[1]);

    let third = MergeQueue::pop_next(&pool, project_id).await.unwrap();
    assert!(third.is_some());
    assert_eq!(third.unwrap().workspace_id, workspace_ids[2]);

    // Queue should now be empty
    let empty = MergeQueue::pop_next(&pool, project_id).await.unwrap();
    assert!(empty.is_none());

    // Count should be 0
    let final_count = MergeQueue::count_by_project(&pool, project_id)
        .await
        .unwrap();
    assert_eq!(final_count, 0);
}

#[tokio::test]
async fn test_merge_queue_claim_next_fifo_order() {
    let pool = create_test_db().await;

    let project_id = create_test_project(&pool, "Claim FIFO Project").await;
    let temp_dir = TempDir::new().unwrap();

    // Create 3 entries
    let mut workspace_ids = Vec::new();
    for i in 0..3 {
        let task_id = create_test_task(&pool, project_id, &format!("Task {}", i + 1)).await;
        let repo_path = temp_dir.path().join(format!("repo{}", i));
        fs::create_dir_all(&repo_path).unwrap();
        let repo_id = create_test_repo(&pool, &repo_path).await;
        let workspace_id =
            create_test_workspace(&pool, task_id, &format!("feature-{}", i + 1), None).await;
        workspace_ids.push(workspace_id);

        MergeQueue::create(&pool, project_id, workspace_id, repo_id)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // Claim first - should be oldest
    let claimed = MergeQueue::claim_next(&pool, project_id).await.unwrap();
    assert!(claimed.is_some());
    let claimed_entry = claimed.unwrap();
    assert_eq!(claimed_entry.workspace_id, workspace_ids[0]);

    // Verify status was updated to 'merging' in the database
    let updated = MergeQueue::find_by_id(&pool, claimed_entry.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.status, MergeQueueStatus::Merging);
    assert!(updated.started_at.is_some());

    // Next claim should get second entry (first is now 'merging', not 'queued')
    let second = MergeQueue::claim_next(&pool, project_id).await.unwrap();
    assert!(second.is_some());
    assert_eq!(second.unwrap().workspace_id, workspace_ids[1]);
}

#[tokio::test]
async fn test_merge_queue_update_status_transitions() {
    let pool = create_test_db().await;

    let project_id = create_test_project(&pool, "Status Test").await;
    let task_id = create_test_task(&pool, project_id, "Test Task").await;

    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    let repo_id = create_test_repo(&pool, &repo_path).await;
    let workspace_id = create_test_workspace(&pool, task_id, "feature-branch", None).await;

    let entry = MergeQueue::create(&pool, project_id, workspace_id, repo_id)
        .await
        .unwrap();

    // Initial status is Queued
    assert_eq!(entry.status, MergeQueueStatus::Queued);

    // Transition to Merging
    MergeQueue::update_status(&pool, entry.id, MergeQueueStatus::Merging, None)
        .await
        .unwrap();
    let updated = MergeQueue::find_by_id(&pool, entry.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.status, MergeQueueStatus::Merging);
    assert!(updated.started_at.is_some());
    assert!(updated.completed_at.is_none());

    // Transition to Completed
    MergeQueue::update_status(&pool, entry.id, MergeQueueStatus::Completed, None)
        .await
        .unwrap();
    let completed = MergeQueue::find_by_id(&pool, entry.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(completed.status, MergeQueueStatus::Completed);
    assert!(completed.completed_at.is_some());
}

#[tokio::test]
async fn test_merge_queue_conflict_status_with_message() {
    let pool = create_test_db().await;

    let project_id = create_test_project(&pool, "Conflict Test").await;
    let task_id = create_test_task(&pool, project_id, "Test Task").await;

    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    let repo_id = create_test_repo(&pool, &repo_path).await;
    let workspace_id = create_test_workspace(&pool, task_id, "feature-branch", None).await;

    let entry = MergeQueue::create(&pool, project_id, workspace_id, repo_id)
        .await
        .unwrap();

    // Transition to Conflict with message
    let conflict_msg = "CONFLICT (content): Merge conflict in src/main.rs";
    MergeQueue::update_status(
        &pool,
        entry.id,
        MergeQueueStatus::Conflict,
        Some(conflict_msg),
    )
    .await
    .unwrap();

    let conflicted = MergeQueue::find_by_id(&pool, entry.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(conflicted.status, MergeQueueStatus::Conflict);
    assert_eq!(conflicted.conflict_message, Some(conflict_msg.to_string()));
    assert!(conflicted.completed_at.is_some());
}

#[tokio::test]
async fn test_merge_queue_delete_by_workspace() {
    let pool = create_test_db().await;

    let project_id = create_test_project(&pool, "Delete Test").await;
    let task_id = create_test_task(&pool, project_id, "Test Task").await;

    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    let repo_id = create_test_repo(&pool, &repo_path).await;
    let workspace_id = create_test_workspace(&pool, task_id, "feature-branch", None).await;

    let entry = MergeQueue::create(&pool, project_id, workspace_id, repo_id)
        .await
        .unwrap();

    // Verify entry exists
    assert!(MergeQueue::find_by_id(&pool, entry.id)
        .await
        .unwrap()
        .is_some());

    // Delete by workspace (simulates cancel)
    MergeQueue::delete_by_workspace(&pool, workspace_id)
        .await
        .unwrap();

    // Verify entry is gone
    assert!(MergeQueue::find_by_id(&pool, entry.id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn test_merge_queue_list_by_project_ordered() {
    let pool = create_test_db().await;

    let project_id = create_test_project(&pool, "List Test").await;
    let temp_dir = TempDir::new().unwrap();

    // Create 3 entries with distinct timestamps
    for i in 0..3 {
        let task_id = create_test_task(&pool, project_id, &format!("Task {}", i + 1)).await;
        let repo_path = temp_dir.path().join(format!("repo{}", i));
        fs::create_dir_all(&repo_path).unwrap();
        let repo_id = create_test_repo(&pool, &repo_path).await;
        let workspace_id =
            create_test_workspace(&pool, task_id, &format!("feature-{}", i + 1), None).await;

        MergeQueue::create(&pool, project_id, workspace_id, repo_id)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // List should return entries in FIFO order
    let entries = MergeQueue::list_by_project(&pool, project_id)
        .await
        .unwrap();
    assert_eq!(entries.len(), 3);

    // Verify chronological order (oldest first)
    for i in 0..2 {
        assert!(entries[i].queued_at <= entries[i + 1].queued_at);
    }
}

#[tokio::test]
async fn test_merge_queue_separate_project_queues() {
    let pool = create_test_db().await;

    // Create two projects with their own queues
    let project1_id = create_test_project(&pool, "Project 1").await;
    let project2_id = create_test_project(&pool, "Project 2").await;

    let temp_dir = TempDir::new().unwrap();

    // Add 2 entries to project 1
    for i in 0..2 {
        let task_id = create_test_task(&pool, project1_id, &format!("P1 Task {}", i + 1)).await;
        let repo_path = temp_dir.path().join(format!("p1_repo{}", i));
        fs::create_dir_all(&repo_path).unwrap();
        let repo_id = create_test_repo(&pool, &repo_path).await;
        let workspace_id =
            create_test_workspace(&pool, task_id, &format!("p1-feature-{}", i + 1), None).await;
        MergeQueue::create(&pool, project1_id, workspace_id, repo_id)
            .await
            .unwrap();
    }

    // Add 1 entry to project 2
    let task_id = create_test_task(&pool, project2_id, "P2 Task 1").await;
    let repo_path = temp_dir.path().join("p2_repo0");
    fs::create_dir_all(&repo_path).unwrap();
    let repo_id = create_test_repo(&pool, &repo_path).await;
    let workspace_id = create_test_workspace(&pool, task_id, "p2-feature-1", None).await;
    MergeQueue::create(&pool, project2_id, workspace_id, repo_id)
        .await
        .unwrap();

    // Verify counts are separate
    let p1_count = MergeQueue::count_by_project(&pool, project1_id)
        .await
        .unwrap();
    let p2_count = MergeQueue::count_by_project(&pool, project2_id)
        .await
        .unwrap();
    assert_eq!(p1_count, 2);
    assert_eq!(p2_count, 1);

    // Pop from project 1 should not affect project 2
    let _ = MergeQueue::pop_next(&pool, project1_id).await.unwrap();
    let p2_count_after = MergeQueue::count_by_project(&pool, project2_id)
        .await
        .unwrap();
    assert_eq!(p2_count_after, 1);
}

// ============================================================================
// MergeQueueProcessor Tests
// ============================================================================

#[tokio::test]
async fn test_processor_empty_queue_returns_ok() {
    let pool = create_test_db().await;
    let project_id = create_test_project(&pool, "Empty Queue Project").await;

    let processor = MergeQueueProcessor::new(pool, GitService::new());
    let result = processor.process_project_queue(project_id).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_workspace_deletion_cascades_to_queue() {
    let pool = create_test_db().await;

    let project_id = create_test_project(&pool, "Cascade Test").await;
    let task_id = create_test_task(&pool, project_id, "Test Task").await;

    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    let repo_id = create_test_repo(&pool, &repo_path).await;

    // Create workspace and queue entry
    let workspace_id = create_test_workspace(&pool, task_id, "feature-branch", None).await;
    let entry = MergeQueue::create(&pool, project_id, workspace_id, repo_id)
        .await
        .unwrap();

    // Verify entry exists
    assert!(MergeQueue::find_by_id(&pool, entry.id)
        .await
        .unwrap()
        .is_some());

    // Delete the workspace - foreign key cascade should delete queue entry
    sqlx::query("DELETE FROM workspaces WHERE id = ?")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();

    // Queue entry should be deleted by cascade
    assert!(MergeQueue::find_by_id(&pool, entry.id)
        .await
        .unwrap()
        .is_none());

    // Processor should handle empty queue gracefully
    let processor = MergeQueueProcessor::new(pool.clone(), GitService::new());
    let result = processor.process_project_queue(project_id).await;
    assert!(result.is_ok());
}

// ============================================================================
// End-to-End Integration Tests with Git Operations
// ============================================================================

#[tokio::test]
async fn test_full_merge_flow_success() {
    let pool = create_test_db().await;
    let temp_dir = TempDir::new().unwrap();

    // Initialize main repo
    let repo_path = init_repo_main(&temp_dir);
    let git = GitService::new();

    // Create base commit on main
    write_file(&repo_path, "README.md", "# Project\n");
    git.commit(&repo_path, "Initial commit").unwrap();

    // Create feature branch and commit changes
    create_branch(&repo_path, "feature-1");
    checkout_branch(&repo_path, "feature-1");
    write_file(&repo_path, "feature.txt", "New feature\n");
    git.commit(&repo_path, "Add feature").unwrap();

    // Switch back to main so we can create a worktree for feature-1
    checkout_branch(&repo_path, "main");

    // Create worktree for the feature branch
    let worktree_path = temp_dir.path().join("worktree-feature-1");
    git.add_worktree(&repo_path, &worktree_path, "feature-1", false)
        .unwrap();

    // Setup database entities
    let project_id = create_test_project(&pool, "Full Flow Project").await;
    let task_id = create_test_task(&pool, project_id, "Add feature").await;
    let repo_id = create_test_repo(&pool, &repo_path).await;
    let workspace_id = create_test_workspace(
        &pool,
        task_id,
        "feature-1",
        Some(&worktree_path.to_string_lossy()),
    )
    .await;
    create_workspace_repo(&pool, workspace_id, repo_id, "main").await;

    // Queue the merge
    let entry = MergeQueue::create(&pool, project_id, workspace_id, repo_id)
        .await
        .unwrap();
    assert_eq!(entry.status, MergeQueueStatus::Queued);

    // Process the queue
    let processor = MergeQueueProcessor::new(pool.clone(), GitService::new());
    processor.process_project_queue(project_id).await.unwrap();

    // Verify merge queue entry is completed
    let completed_entry = MergeQueue::find_by_id(&pool, entry.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(completed_entry.status, MergeQueueStatus::Completed);
    assert!(completed_entry.conflict_message.is_none());

    // Verify Merge record was created
    let merge_row = sqlx::query(
        "SELECT id, workspace_id, repo_id, target_branch_name, merge_commit FROM merges WHERE workspace_id = ?",
    )
    .bind(workspace_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let target_branch: String = merge_row.get("target_branch_name");
    let merge_commit: String = merge_row.get("merge_commit");
    assert_eq!(target_branch, "main");
    assert!(!merge_commit.is_empty());

    // Verify task status was updated to Done
    let task = Task::find_by_id(&pool, task_id).await.unwrap().unwrap();
    assert_eq!(task.status, TaskStatus::Done);

    // Verify main branch has the merge
    checkout_branch(&repo_path, "main");
    let main_files: Vec<_> = fs::read_dir(&repo_path)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert!(main_files.contains(&"feature.txt".to_string()));
}

#[tokio::test]
async fn test_merge_with_conflict_marks_entry() {
    let pool = create_test_db().await;
    let temp_dir = TempDir::new().unwrap();

    // Initialize main repo
    let repo_path = init_repo_main(&temp_dir);
    let git = GitService::new();

    // Create base commit with a file
    write_file(&repo_path, "conflict.txt", "Original content\n");
    git.commit(&repo_path, "Initial commit").unwrap();

    // Create feature branch with conflicting changes
    create_branch(&repo_path, "feature-conflict");
    checkout_branch(&repo_path, "feature-conflict");
    write_file(&repo_path, "conflict.txt", "Feature branch content\n");
    git.commit(&repo_path, "Feature changes").unwrap();

    // Go back to main and make conflicting changes
    checkout_branch(&repo_path, "main");
    write_file(&repo_path, "conflict.txt", "Main branch content\n");
    git.commit(&repo_path, "Main changes").unwrap();

    // Create worktree for feature branch
    let worktree_path = temp_dir.path().join("worktree-conflict");
    git.add_worktree(&repo_path, &worktree_path, "feature-conflict", false)
        .unwrap();

    // Setup database
    let project_id = create_test_project(&pool, "Conflict Project").await;
    let task_id = create_test_task(&pool, project_id, "Conflicting task").await;
    let repo_id = create_test_repo(&pool, &repo_path).await;
    let workspace_id = create_test_workspace(
        &pool,
        task_id,
        "feature-conflict",
        Some(&worktree_path.to_string_lossy()),
    )
    .await;
    create_workspace_repo(&pool, workspace_id, repo_id, "main").await;

    // Queue the merge
    let entry = MergeQueue::create(&pool, project_id, workspace_id, repo_id)
        .await
        .unwrap();

    // Process - should fail with conflict
    let processor = MergeQueueProcessor::new(pool.clone(), GitService::new());
    processor.process_project_queue(project_id).await.unwrap();

    // Verify entry has conflict status
    let conflict_entry = MergeQueue::find_by_id(&pool, entry.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(conflict_entry.status, MergeQueueStatus::Conflict);
    assert!(conflict_entry.conflict_message.is_some());

    // Verify task status was NOT changed to Done
    let task = Task::find_by_id(&pool, task_id).await.unwrap().unwrap();
    assert_ne!(task.status, TaskStatus::Done);
}

#[tokio::test]
async fn test_queue_processes_multiple_entries_fifo() {
    let pool = create_test_db().await;
    let temp_dir = TempDir::new().unwrap();

    // Initialize main repo
    let repo_path = init_repo_main(&temp_dir);
    let git = GitService::new();

    write_file(&repo_path, "README.md", "# Project\n");
    git.commit(&repo_path, "Initial").unwrap();

    let project_id = create_test_project(&pool, "Multi-entry Project").await;
    let repo_id = create_test_repo(&pool, &repo_path).await;

    // Create 3 feature branches and queue them
    let mut entry_ids = Vec::new();
    for i in 1..=3 {
        let branch_name = format!("feature-{}", i);
        checkout_branch(&repo_path, "main");
        create_branch(&repo_path, &branch_name);
        checkout_branch(&repo_path, &branch_name);
        write_file(
            &repo_path,
            &format!("feature{}.txt", i),
            &format!("Feature {} content\n", i),
        );
        git.commit(&repo_path, &format!("Add feature {}", i))
            .unwrap();

        // Switch back to main before creating worktree
        checkout_branch(&repo_path, "main");

        let worktree_path = temp_dir.path().join(format!("wt-{}", i));
        git.add_worktree(&repo_path, &worktree_path, &branch_name, false)
            .unwrap();

        let task_id = create_test_task(&pool, project_id, &format!("Feature {} task", i)).await;
        let workspace_id = create_test_workspace(
            &pool,
            task_id,
            &branch_name,
            Some(&worktree_path.to_string_lossy()),
        )
        .await;
        create_workspace_repo(&pool, workspace_id, repo_id, "main").await;

        let entry = MergeQueue::create(&pool, project_id, workspace_id, repo_id)
            .await
            .unwrap();
        entry_ids.push(entry.id);

        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // Process all
    let processor = MergeQueueProcessor::new(pool.clone(), GitService::new());
    processor.process_project_queue(project_id).await.unwrap();

    // All should be completed
    for entry_id in &entry_ids {
        let entry = MergeQueue::find_by_id(&pool, *entry_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            entry.status,
            MergeQueueStatus::Completed,
            "Entry {:?} should be completed",
            entry_id
        );
    }

    // Verify order by checking completed_at timestamps
    let mut completed_times = Vec::new();
    for entry_id in &entry_ids {
        let entry = MergeQueue::find_by_id(&pool, *entry_id)
            .await
            .unwrap()
            .unwrap();
        completed_times.push(entry.completed_at.unwrap());
    }

    // First queued should complete first (FIFO)
    assert!(completed_times[0] <= completed_times[1]);
    assert!(completed_times[1] <= completed_times[2]);

    // Verify all files are on main
    checkout_branch(&repo_path, "main");
    for i in 1..=3 {
        let content = fs::read_to_string(repo_path.join(format!("feature{}.txt", i)));
        assert!(content.is_ok(), "feature{}.txt should exist on main", i);
    }
}

#[tokio::test]
async fn test_conflict_skips_to_next_entry() {
    let pool = create_test_db().await;
    let temp_dir = TempDir::new().unwrap();

    let repo_path = init_repo_main(&temp_dir);
    let git = GitService::new();

    write_file(&repo_path, "shared.txt", "Original\n");
    git.commit(&repo_path, "Initial").unwrap();

    let project_id = create_test_project(&pool, "Skip Conflict Project").await;
    let repo_id = create_test_repo(&pool, &repo_path).await;

    // Feature 1: Will cause conflict
    create_branch(&repo_path, "conflict-feature");
    checkout_branch(&repo_path, "conflict-feature");
    write_file(&repo_path, "shared.txt", "Conflict content\n");
    git.commit(&repo_path, "Conflict change").unwrap();

    // Now modify main to create conflict
    checkout_branch(&repo_path, "main");
    write_file(&repo_path, "shared.txt", "Main changed\n");
    git.commit(&repo_path, "Main change").unwrap();

    // Feature 2: Should succeed (no conflict)
    create_branch(&repo_path, "safe-feature");
    checkout_branch(&repo_path, "safe-feature");
    write_file(&repo_path, "safe.txt", "Safe content\n");
    git.commit(&repo_path, "Safe change").unwrap();

    // Switch back to main before creating worktrees
    checkout_branch(&repo_path, "main");

    // Setup conflict entry
    let wt1 = temp_dir.path().join("wt-conflict");
    git.add_worktree(&repo_path, &wt1, "conflict-feature", false)
        .unwrap();
    let task1_id = create_test_task(&pool, project_id, "Conflict task").await;
    let ws1_id = create_test_workspace(
        &pool,
        task1_id,
        "conflict-feature",
        Some(&wt1.to_string_lossy()),
    )
    .await;
    create_workspace_repo(&pool, ws1_id, repo_id, "main").await;
    let entry1 = MergeQueue::create(&pool, project_id, ws1_id, repo_id)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(10)).await;

    // Setup safe entry
    let wt2 = temp_dir.path().join("wt-safe");
    git.add_worktree(&repo_path, &wt2, "safe-feature", false)
        .unwrap();
    let task2_id = create_test_task(&pool, project_id, "Safe task").await;
    let ws2_id =
        create_test_workspace(&pool, task2_id, "safe-feature", Some(&wt2.to_string_lossy())).await;
    create_workspace_repo(&pool, ws2_id, repo_id, "main").await;
    let entry2 = MergeQueue::create(&pool, project_id, ws2_id, repo_id)
        .await
        .unwrap();

    // Process queue
    let processor = MergeQueueProcessor::new(pool.clone(), GitService::new());
    processor.process_project_queue(project_id).await.unwrap();

    // First entry should be conflict
    let e1 = MergeQueue::find_by_id(&pool, entry1.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(e1.status, MergeQueueStatus::Conflict);

    // Second entry should be completed (processor continued after conflict)
    let e2 = MergeQueue::find_by_id(&pool, entry2.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(e2.status, MergeQueueStatus::Completed);

    // Verify safe.txt is on main
    checkout_branch(&repo_path, "main");
    assert!(repo_path.join("safe.txt").exists());
}

// ============================================================================
// MergeQueueError Tests
// ============================================================================

#[test]
fn test_merge_queue_error_is_conflict() {
    assert!(MergeQueueError::MergeConflict("test".to_string()).is_conflict());
    assert!(MergeQueueError::RebaseConflict("test".to_string()).is_conflict());
    assert!(!MergeQueueError::TaskNotFound(Uuid::new_v4()).is_conflict());
    assert!(!MergeQueueError::WorkspaceNotFound(Uuid::new_v4()).is_conflict());
    assert!(!MergeQueueError::RepoNotFound(Uuid::new_v4()).is_conflict());
}

#[test]
fn test_merge_queue_error_conflict_message() {
    let merge_err = MergeQueueError::MergeConflict("merge conflict in file.rs".to_string());
    assert_eq!(
        merge_err.conflict_message(),
        Some("merge conflict in file.rs")
    );

    let rebase_err = MergeQueueError::RebaseConflict("rebase conflict".to_string());
    assert_eq!(rebase_err.conflict_message(), Some("rebase conflict"));

    let other = MergeQueueError::TaskNotFound(Uuid::new_v4());
    assert_eq!(other.conflict_message(), None);
}
