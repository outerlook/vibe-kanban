//! Autopilot E2E tests for full flow testing.
//!
//! These tests verify the complete autopilot flow from task status changes
//! through handler execution to execution completion.

#[path = "autopilot_e2e_fixtures/mod.rs"]
mod mock_controller;
#[allow(dead_code)]
mod autopilot_e2e_git_fixtures;

use mock_controller::MockExecutionController;
use services::services::domain_events::ExecutionTrigger;
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use tempfile::NamedTempFile;
use uuid::Uuid;

/// Creates a unique file-based SQLite database and runs all migrations.
async fn create_test_db() -> (SqlitePool, NamedTempFile) {
    let db_file = NamedTempFile::new().expect("Failed to create temp file");
    let db_path = db_file.path().to_str().expect("Invalid temp file path");

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite:{}?mode=rwc", db_path))
        .await
        .expect("Failed to create database");

    sqlx::migrate!("../db/migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    (pool, db_file)
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
    let now = chrono::Utc::now();

    sqlx::query(
        "INSERT INTO tasks (id, project_id, title, status, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(project_id)
    .bind(title)
    .bind("inprogress")
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .expect("Failed to create task");

    id
}

/// Creates a test workspace in the database.
async fn create_test_workspace_record(pool: &SqlitePool, task_id: Uuid, branch: &str) -> Uuid {
    let id = Uuid::new_v4();
    let now = chrono::Utc::now();

    sqlx::query(
        "INSERT INTO workspaces (id, task_id, branch, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(task_id)
    .bind(branch)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .expect("Failed to create workspace");

    id
}

#[tokio::test]
async fn test_mock_controller_captures_triggers() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "test_project").await;
    let task_id = create_test_task(&pool, project_id, "Test task").await;
    let workspace_id = create_test_workspace_record(&pool, task_id, "feature/test").await;

    let controller = MockExecutionController::new(pool.clone());
    let callback = controller.callback();

    // Fire a FeedbackCollection trigger (execution_process_id is the process that triggered this)
    let source_exec_id = Uuid::new_v4();
    let trigger = ExecutionTrigger::FeedbackCollection {
        workspace_id,
        task_id,
        execution_process_id: source_exec_id,
    };
    let _exec_id = callback(trigger.clone()).await.unwrap();

    // Verify capture
    let captures = controller.get_captures();
    assert_eq!(captures.len(), 1);
    match &captures[0] {
        ExecutionTrigger::FeedbackCollection {
            workspace_id: ws_id,
            task_id: t_id,
            execution_process_id: ep_id,
        } => {
            assert_eq!(ws_id, &workspace_id);
            assert_eq!(t_id, &task_id);
            assert_eq!(ep_id, &source_exec_id);
        }
        _ => panic!("Expected FeedbackCollection trigger"),
    }
}

#[tokio::test]
async fn test_mock_controller_creates_db_records() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "test_project").await;
    let task_id = create_test_task(&pool, project_id, "Test task").await;
    let workspace_id = create_test_workspace_record(&pool, task_id, "feature/test").await;

    let controller = MockExecutionController::new(pool.clone());
    let callback = controller.callback();

    // Fire a ReviewAttention trigger
    let source_exec_id = Uuid::new_v4();
    let trigger = ExecutionTrigger::ReviewAttention {
        task_id,
        execution_process_id: source_exec_id,
    };
    let exec_id = callback(trigger).await.unwrap();

    // Verify session was created
    let session_count: (i32,) = sqlx::query_as(
        r#"SELECT COUNT(*) FROM sessions WHERE workspace_id = ?"#,
    )
    .bind(workspace_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(session_count.0, 1);

    // Verify execution process was created
    let exec_status: (String,) = sqlx::query_as(
        r#"SELECT status FROM execution_processes WHERE id = ?"#,
    )
    .bind(exec_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(exec_status.0, "running");

    // Complete the execution
    controller.complete_execution(exec_id).await.unwrap();

    // Verify status changed
    let exec_status: (String,) = sqlx::query_as(
        r#"SELECT status FROM execution_processes WHERE id = ?"#,
    )
    .bind(exec_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(exec_status.0, "completed");
}

#[tokio::test]
async fn test_mock_controller_set_review_response() {
    let (pool, _db_file) = create_test_db().await;
    let controller = MockExecutionController::new(pool);
    let task_id = Uuid::new_v4();

    // No response initially
    assert!(controller.get_response(task_id).is_none());

    // Set response
    controller.set_review_response(task_id, true);

    // Verify response
    let response = controller.get_response(task_id).unwrap();
    assert_eq!(response.needs_attention, Some(true));
}

#[tokio::test]
async fn test_mock_controller_clear_captures() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "test_project").await;
    let task_id = create_test_task(&pool, project_id, "Test task").await;
    let workspace_id = create_test_workspace_record(&pool, task_id, "feature/test").await;

    let controller = MockExecutionController::new(pool.clone());
    let callback = controller.callback();

    // Fire a trigger
    let source_exec_id = Uuid::new_v4();
    let trigger = ExecutionTrigger::FeedbackCollection {
        workspace_id,
        task_id,
        execution_process_id: source_exec_id,
    };
    let _exec_id = callback(trigger).await.unwrap();

    // Verify capture exists
    assert_eq!(controller.get_captures().len(), 1);

    // Clear captures
    controller.clear_captures();

    // Verify cleared
    assert_eq!(controller.get_captures().len(), 0);
}
