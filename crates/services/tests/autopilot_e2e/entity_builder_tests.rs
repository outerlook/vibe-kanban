//! Tests for the EntityGraphBuilder fluent API.

use db::models::task::TaskStatus;
use uuid::Uuid;

use super::fixtures::{EntityGraphBuilder, TestDb};

#[tokio::test]
async fn test_entity_builder_creates_full_graph() {
    let db = TestDb::new().await;
    let pool = db.pool().clone();

    // Create a full entity graph: project → task → workspace → session → execution
    let ctx = EntityGraphBuilder::new(pool.clone())
        .with_project("Test Project")
        .create_task("Task 1", TaskStatus::Todo)
        .await
        .with_workspace("feature-branch")
        .await
        .with_session()
        .await
        .with_completed_coding_execution()
        .await;

    // Verify all IDs are accessible
    let project_id = ctx.project_id();
    let task_id = ctx.task_id();
    let workspace_id = ctx.workspace_id();
    let session_id = ctx.session_id();
    let execution_id = ctx.execution_id().expect("execution should exist");

    // Verify project exists in DB
    let project_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projects WHERE id = ?)")
            .bind(project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(project_exists, "Project should exist in database");

    // Verify task exists and is linked to project
    let task_project_id: Uuid =
        sqlx::query_scalar("SELECT project_id FROM tasks WHERE id = ?")
            .bind(task_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        task_project_id, project_id,
        "Task should be linked to project"
    );

    // Verify workspace exists and is linked to task
    let workspace_task_id: Uuid =
        sqlx::query_scalar("SELECT task_id FROM workspaces WHERE id = ?")
            .bind(workspace_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        workspace_task_id, task_id,
        "Workspace should be linked to task"
    );

    // Verify session exists and is linked to workspace
    let session_workspace_id: Uuid =
        sqlx::query_scalar("SELECT workspace_id FROM sessions WHERE id = ?")
            .bind(session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        session_workspace_id, workspace_id,
        "Session should be linked to workspace"
    );

    // Verify execution exists and is linked to session
    let execution_session_id: Uuid =
        sqlx::query_scalar("SELECT session_id FROM execution_processes WHERE id = ?")
            .bind(execution_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        execution_session_id, session_id,
        "Execution should be linked to session"
    );

    // Verify execution is completed and is a coding agent
    let (status, run_reason): (String, String) = sqlx::query_as(
        "SELECT status, run_reason FROM execution_processes WHERE id = ?",
    )
    .bind(execution_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "completed", "Execution should be completed");
    assert_eq!(
        run_reason, "codingagent",
        "Execution should be a coding agent"
    );
}

#[tokio::test]
async fn test_entity_builder_creates_multiple_tasks_same_project() {
    let db = TestDb::new().await;
    let pool = db.pool().clone();

    // Create first task
    let task1_ctx = EntityGraphBuilder::new(pool.clone())
        .with_project("Multi-Task Project")
        .create_task("Task 1", TaskStatus::Todo)
        .await;

    let project_id = task1_ctx.project_id();
    let task1_id = task1_ctx.task_id();

    // Create second task using and_task
    let task2_ctx = task1_ctx
        .and_task("Task 2", TaskStatus::InProgress)
        .await;

    let task2_id = task2_ctx.task_id();

    // Create third task using builder
    let task3_ctx = task2_ctx
        .builder()
        .create_task("Task 3", TaskStatus::Done)
        .await;

    let task3_id = task3_ctx.task_id();

    // Verify all tasks exist and belong to the same project
    let task_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE project_id = ?")
            .bind(project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(task_count, 3, "Should have 3 tasks in project");

    // Verify task IDs are distinct
    assert_ne!(task1_id, task2_id);
    assert_ne!(task2_id, task3_id);
    assert_ne!(task1_id, task3_id);

    // Verify all belong to same project
    assert_eq!(task3_ctx.project_id(), project_id);
}

#[tokio::test]
async fn test_entity_builder_task_dependencies() {
    let db = TestDb::new().await;
    let pool = db.pool().clone();

    // Create first task
    let task1_ctx = EntityGraphBuilder::new(pool.clone())
        .with_project("Dependency Project")
        .create_task("Task 1 - Base", TaskStatus::Done)
        .await;

    let task1_id = task1_ctx.task_id();

    // Create second task that depends on first
    let task2_ctx = task1_ctx
        .and_task("Task 2 - Depends on Task 1", TaskStatus::Todo)
        .await
        .with_dependency(task1_id)
        .await;

    let task2_id = task2_ctx.task_id();

    // Verify dependency exists in DB
    let dependency_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM task_dependencies WHERE task_id = ? AND depends_on_id = ?)",
    )
    .bind(task2_id)
    .bind(task1_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(dependency_exists, "Dependency should exist in database");
}

#[tokio::test]
async fn test_entity_builder_default_project_name() {
    let db = TestDb::new().await;
    let pool = db.pool().clone();

    // Create task without specifying project name
    let ctx = EntityGraphBuilder::new(pool.clone())
        .create_task("Task without explicit project", TaskStatus::Todo)
        .await;

    // Verify project was created with default name
    let project_name: String =
        sqlx::query_scalar("SELECT name FROM projects WHERE id = ?")
            .bind(ctx.project_id())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(project_name, "Test Project", "Should use default project name");
}

#[tokio::test]
async fn test_entity_builder_accessors() {
    let db = TestDb::new().await;
    let pool = db.pool().clone();

    let ctx = EntityGraphBuilder::new(pool)
        .with_project("Accessor Test")
        .create_task("Test Task", TaskStatus::InProgress)
        .await
        .with_workspace("test-branch")
        .await
        .with_session()
        .await;

    // Verify task accessor returns correct data
    assert_eq!(ctx.task().title, "Test Task");
    assert_eq!(ctx.task().status, TaskStatus::InProgress);

    // Verify workspace accessor returns correct data
    assert_eq!(ctx.workspace().branch, "test-branch");

    // Verify execution is None before creating
    assert!(ctx.execution().is_none());
    assert!(ctx.execution_id().is_none());
}
