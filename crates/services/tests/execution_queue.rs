//! Integration tests for the execution queue cancellation feature.
//!
//! Tests verify:
//! - Queue entry creation
//! - Queue cancellation (delete_by_workspace)
//! - Task materialized status update after cancellation

use db::models::{execution_queue::ExecutionQueue, task::Task};
use executors::{executors::BaseCodingAgent, profile::ExecutorProfileId};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
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
    sqlx::query("INSERT INTO tasks (id, project_id, title, status) VALUES (?, ?, ?, 'todo')")
        .bind(id)
        .bind(project_id)
        .bind(title)
        .execute(pool)
        .await
        .expect("Failed to create task");
    id
}

/// Creates a test workspace in the database.
async fn create_test_workspace(pool: &SqlitePool, task_id: Uuid, branch: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id, task_id, branch) VALUES (?, ?, ?)")
        .bind(id)
        .bind(task_id)
        .bind(branch)
        .execute(pool)
        .await
        .expect("Failed to create workspace");
    id
}

#[tokio::test]
async fn test_execution_queue_create_and_find() {
    let pool = create_test_db().await;
    let project_id = create_test_project(&pool, "Test Project").await;
    let task_id = create_test_task(&pool, project_id, "Test Task").await;
    let workspace_id = create_test_workspace(&pool, task_id, "test-branch").await;

    let executor_profile = ExecutorProfileId {
        executor: BaseCodingAgent::ClaudeCode,
        variant: None,
    };

    // Create queue entry
    let entry = ExecutionQueue::create(&pool, workspace_id, &executor_profile)
        .await
        .expect("Failed to create queue entry");

    assert_eq!(entry.workspace_id, workspace_id);
    assert!(!entry.is_follow_up());

    // Find by workspace
    let found = ExecutionQueue::find_by_workspace(&pool, workspace_id)
        .await
        .expect("Failed to find queue entry");
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, entry.id);
}

#[tokio::test]
async fn test_execution_queue_delete_by_workspace() {
    let pool = create_test_db().await;
    let project_id = create_test_project(&pool, "Test Project").await;
    let task_id = create_test_task(&pool, project_id, "Test Task").await;
    let workspace_id = create_test_workspace(&pool, task_id, "test-branch").await;

    let executor_profile = ExecutorProfileId {
        executor: BaseCodingAgent::ClaudeCode,
        variant: None,
    };

    // Create queue entry
    ExecutionQueue::create(&pool, workspace_id, &executor_profile)
        .await
        .expect("Failed to create queue entry");

    // Verify entry exists
    let found = ExecutionQueue::find_by_workspace(&pool, workspace_id)
        .await
        .expect("Failed to find queue entry");
    assert!(found.is_some());

    // Delete by workspace
    ExecutionQueue::delete_by_workspace(&pool, workspace_id)
        .await
        .expect("Failed to delete queue entry");

    // Verify entry is deleted
    let found_after = ExecutionQueue::find_by_workspace(&pool, workspace_id)
        .await
        .expect("Failed to check queue entry");
    assert!(found_after.is_none());
}

#[tokio::test]
async fn test_execution_queue_cancel_updates_task_is_queued() {
    let pool = create_test_db().await;
    let project_id = create_test_project(&pool, "Test Project").await;
    let task_id = create_test_task(&pool, project_id, "Test Task").await;
    let workspace_id = create_test_workspace(&pool, task_id, "test-branch").await;

    let executor_profile = ExecutorProfileId {
        executor: BaseCodingAgent::ClaudeCode,
        variant: None,
    };

    // Create queue entry
    ExecutionQueue::create(&pool, workspace_id, &executor_profile)
        .await
        .expect("Failed to create queue entry");

    // Update materialized status - should set is_queued = true
    Task::update_materialized_status(&pool, task_id)
        .await
        .expect("Failed to update materialized status");

    // Verify task is_queued is true
    let task = Task::find_by_id(&pool, task_id)
        .await
        .expect("Failed to find task")
        .expect("Task not found");
    assert!(task.is_queued, "Task should be queued after creating queue entry");

    // Delete queue entry (simulating cancel)
    ExecutionQueue::delete_by_workspace(&pool, workspace_id)
        .await
        .expect("Failed to delete queue entry");

    // Update materialized status again - should set is_queued = false
    Task::update_materialized_status(&pool, task_id)
        .await
        .expect("Failed to update materialized status");

    // Verify task is_queued is false
    let task_after = Task::find_by_id(&pool, task_id)
        .await
        .expect("Failed to find task")
        .expect("Task not found");
    assert!(
        !task_after.is_queued,
        "Task should not be queued after cancelling"
    );
}

#[tokio::test]
async fn test_execution_queue_delete_nonexistent_is_noop() {
    let pool = create_test_db().await;
    let project_id = create_test_project(&pool, "Test Project").await;
    let task_id = create_test_task(&pool, project_id, "Test Task").await;
    let workspace_id = create_test_workspace(&pool, task_id, "test-branch").await;

    // Delete without creating - should succeed without error
    ExecutionQueue::delete_by_workspace(&pool, workspace_id)
        .await
        .expect("Delete of non-existent entry should succeed");

    // Verify nothing is there
    let found = ExecutionQueue::find_by_workspace(&pool, workspace_id)
        .await
        .expect("Failed to check queue entry");
    assert!(found.is_none());
}

#[tokio::test]
async fn test_execution_queue_cancel_is_idempotent() {
    let pool = create_test_db().await;
    let project_id = create_test_project(&pool, "Test Project").await;
    let task_id = create_test_task(&pool, project_id, "Test Task").await;
    let workspace_id = create_test_workspace(&pool, task_id, "test-branch").await;

    let executor_profile = ExecutorProfileId {
        executor: BaseCodingAgent::ClaudeCode,
        variant: None,
    };

    // Create queue entry
    ExecutionQueue::create(&pool, workspace_id, &executor_profile)
        .await
        .expect("Failed to create queue entry");

    // Delete multiple times (simulating rapid cancel clicks)
    for _ in 0..3 {
        ExecutionQueue::delete_by_workspace(&pool, workspace_id)
            .await
            .expect("Delete should be idempotent");
    }

    // Verify entry is deleted
    let found = ExecutionQueue::find_by_workspace(&pool, workspace_id)
        .await
        .expect("Failed to check queue entry");
    assert!(found.is_none());
}

#[tokio::test]
async fn test_execution_queue_count() {
    let pool = create_test_db().await;
    let project_id = create_test_project(&pool, "Test Project").await;

    // Start with empty queue
    let initial_count = ExecutionQueue::count(&pool)
        .await
        .expect("Failed to get count");
    assert_eq!(initial_count, 0);

    let executor_profile = ExecutorProfileId {
        executor: BaseCodingAgent::ClaudeCode,
        variant: None,
    };

    // Create multiple queue entries
    for i in 0..3 {
        let task_id = create_test_task(&pool, project_id, &format!("Task {}", i)).await;
        let workspace_id = create_test_workspace(&pool, task_id, &format!("branch-{}", i)).await;
        ExecutionQueue::create(&pool, workspace_id, &executor_profile)
            .await
            .expect("Failed to create queue entry");
    }

    // Verify count
    let count = ExecutionQueue::count(&pool).await.expect("Failed to get count");
    assert_eq!(count, 3);
}
