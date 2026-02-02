//! Tests for autopilot dependency triggering when tasks complete.
//!
//! These tests verify the flow: Task Done → AutopilotHandler → find unblocked dependents → ExecutionQueue.create

use std::sync::Arc;

use db::models::{execution_queue::ExecutionQueue, task::TaskStatus};
use services::services::{
    config::Config,
    domain_events::{AutopilotHandler, DomainEvent, DispatcherBuilder, HandlerContext},
};
use tokio::sync::RwLock;
use utils::msg_store::MsgStore;

use super::fixtures::{EntityGraphBuilder, TestDb, autopilot_config, autopilot_disabled_config};

/// Creates a HandlerContext for testing with the given config.
fn test_handler_context(pool: sqlx::SqlitePool, config: Arc<RwLock<Config>>) -> HandlerContext {
    let db = db::DBService { pool };
    let msg_store = Arc::new(MsgStore::default());
    HandlerContext::new(db, config, msg_store, None)
}

/// Helper to update task status directly in the database.
async fn update_task_status(pool: &sqlx::SqlitePool, task_id: uuid::Uuid, status: TaskStatus) {
    let status_str = match status {
        TaskStatus::Todo => "todo",
        TaskStatus::InProgress => "inprogress",
        TaskStatus::InReview => "inreview",
        TaskStatus::Done => "done",
        TaskStatus::Cancelled => "cancelled",
    };
    sqlx::query("UPDATE tasks SET status = ? WHERE id = ?")
        .bind(status_str)
        .bind(task_id)
        .execute(pool)
        .await
        .expect("Failed to update task status");
}

/// Helper to get is_blocked value for a task.
async fn get_task_is_blocked(pool: &sqlx::SqlitePool, task_id: uuid::Uuid) -> bool {
    sqlx::query_scalar("SELECT is_blocked FROM tasks WHERE id = ?")
        .bind(task_id)
        .fetch_one(pool)
        .await
        .expect("Failed to fetch is_blocked")
}

#[tokio::test]
async fn test_task_done_unblocks_and_enqueues_dependent() {
    // Setup: Task A (InProgress), Task B (Todo, depends on A)
    // Both tasks have workspaces with sessions
    let db = TestDb::new().await;
    let pool = db.pool().clone();

    // Create Task A (InProgress) with workspace and session
    let task_a_ctx = EntityGraphBuilder::new(pool.clone())
        .with_project("Dependency Test Project")
        .create_task("Task A - Base", TaskStatus::InProgress)
        .await
        .with_workspace("feature-a")
        .await
        .with_session()
        .await;

    let task_a_id = task_a_ctx.task_id();
    let project_id = task_a_ctx.project_id();

    // Create Task B (Todo, depends on A) with workspace and session
    let task_b_ctx = task_a_ctx
        .builder()
        .create_task("Task B - Depends on A", TaskStatus::Todo)
        .await
        .with_dependency(task_a_id)
        .await
        .with_workspace("feature-b")
        .await
        .with_session()
        .await;

    let task_b_id = task_b_ctx.task_id();
    let workspace_b_id = task_b_ctx.workspace_id();

    // Verify Task B is initially blocked (DB trigger should have set this)
    let is_blocked = get_task_is_blocked(&pool, task_b_id).await;
    assert!(is_blocked, "Task B should be blocked initially");

    // Verify no execution queue entries initially
    let queue_entry = ExecutionQueue::find_by_workspace(&pool, workspace_b_id)
        .await
        .expect("DB query failed");
    assert!(
        queue_entry.is_none(),
        "No queue entry should exist initially for Task B"
    );

    // Update Task A to Done (simulating completion)
    update_task_status(&pool, task_a_id, TaskStatus::Done).await;

    // The DB trigger should update is_blocked on Task B
    let is_blocked = get_task_is_blocked(&pool, task_b_id).await;
    assert!(
        !is_blocked,
        "Task B should be unblocked after Task A completes"
    );

    // Now dispatch TaskStatusChanged event via AutopilotHandler
    let config = autopilot_config();
    let ctx = test_handler_context(pool.clone(), config);

    let dispatcher = DispatcherBuilder::new()
        .with_handler(AutopilotHandler::new())
        .with_context(ctx)
        .build();

    // Create the event for Task A completing
    let task_a = db::models::task::Task {
        id: task_a_id,
        project_id,
        title: "Task A - Base".to_string(),
        description: None,
        status: TaskStatus::Done,
        parent_workspace_id: None,
        shared_task_id: None,
        task_group_id: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        is_blocked: false,
        has_in_progress_attempt: false,
        last_attempt_failed: false,
        is_queued: false,
        last_executor: String::new(),
        needs_attention: None,
    };

    dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task: task_a,
            previous_status: TaskStatus::InProgress,
        })
        .await;

    // Give spawned handler time to complete (AutopilotHandler is Spawned mode)
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Assert: ExecutionQueue entry created for Task B's workspace
    let queue_entry = ExecutionQueue::find_by_workspace(&pool, workspace_b_id)
        .await
        .expect("DB query failed");
    assert!(
        queue_entry.is_some(),
        "ExecutionQueue entry should be created for Task B's workspace"
    );

    let queue_entry = queue_entry.unwrap();
    assert_eq!(
        queue_entry.workspace_id, workspace_b_id,
        "Queue entry should be for Task B's workspace"
    );
}

#[tokio::test]
async fn test_multiple_dependents_all_enqueued() {
    // Task A with 3 dependents (B, C, D)
    // A completes
    // Assert: All 3 dependents enqueued
    let db = TestDb::new().await;
    let pool = db.pool().clone();

    // Create Task A (InProgress)
    let task_a_ctx = EntityGraphBuilder::new(pool.clone())
        .with_project("Multi-Dependent Project")
        .create_task("Task A - Base", TaskStatus::InProgress)
        .await
        .with_workspace("feature-a")
        .await
        .with_session()
        .await;

    let task_a_id = task_a_ctx.task_id();
    let project_id = task_a_ctx.project_id();

    // Create Tasks B, C, D - all depend on A
    let task_b_ctx = task_a_ctx
        .builder()
        .create_task("Task B", TaskStatus::Todo)
        .await
        .with_dependency(task_a_id)
        .await
        .with_workspace("feature-b")
        .await
        .with_session()
        .await;
    let workspace_b_id = task_b_ctx.workspace_id();

    let task_c_ctx = task_b_ctx
        .builder()
        .create_task("Task C", TaskStatus::Todo)
        .await
        .with_dependency(task_a_id)
        .await
        .with_workspace("feature-c")
        .await
        .with_session()
        .await;
    let workspace_c_id = task_c_ctx.workspace_id();

    let task_d_ctx = task_c_ctx
        .builder()
        .create_task("Task D", TaskStatus::Todo)
        .await
        .with_dependency(task_a_id)
        .await
        .with_workspace("feature-d")
        .await
        .with_session()
        .await;
    let workspace_d_id = task_d_ctx.workspace_id();

    // Mark Task A as Done
    update_task_status(&pool, task_a_id, TaskStatus::Done).await;

    // Dispatch event
    let config = autopilot_config();
    let ctx = test_handler_context(pool.clone(), config);

    let dispatcher = DispatcherBuilder::new()
        .with_handler(AutopilotHandler::new())
        .with_context(ctx)
        .build();

    let task_a = db::models::task::Task {
        id: task_a_id,
        project_id,
        title: "Task A - Base".to_string(),
        description: None,
        status: TaskStatus::Done,
        parent_workspace_id: None,
        shared_task_id: None,
        task_group_id: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        is_blocked: false,
        has_in_progress_attempt: false,
        last_attempt_failed: false,
        is_queued: false,
        last_executor: String::new(),
        needs_attention: None,
    };

    dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task: task_a,
            previous_status: TaskStatus::InProgress,
        })
        .await;

    // Give spawned handler time to complete
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Assert: All 3 dependents enqueued
    let queue_b = ExecutionQueue::find_by_workspace(&pool, workspace_b_id)
        .await
        .expect("DB query failed");
    let queue_c = ExecutionQueue::find_by_workspace(&pool, workspace_c_id)
        .await
        .expect("DB query failed");
    let queue_d = ExecutionQueue::find_by_workspace(&pool, workspace_d_id)
        .await
        .expect("DB query failed");

    assert!(
        queue_b.is_some(),
        "Task B's workspace should be enqueued"
    );
    assert!(
        queue_c.is_some(),
        "Task C's workspace should be enqueued"
    );
    assert!(
        queue_d.is_some(),
        "Task D's workspace should be enqueued"
    );
}

#[tokio::test]
async fn test_autopilot_disabled_skips_enqueue() {
    // Use autopilot_disabled_config()
    // Task A completes
    // Assert: Dependent B is_blocked=false (via DB trigger), but NOT enqueued
    let db = TestDb::new().await;
    let pool = db.pool().clone();

    // Create Task A and Task B (depends on A)
    let task_a_ctx = EntityGraphBuilder::new(pool.clone())
        .with_project("Autopilot Disabled Test")
        .create_task("Task A", TaskStatus::InProgress)
        .await
        .with_workspace("feature-a")
        .await
        .with_session()
        .await;

    let task_a_id = task_a_ctx.task_id();
    let project_id = task_a_ctx.project_id();

    let task_b_ctx = task_a_ctx
        .builder()
        .create_task("Task B", TaskStatus::Todo)
        .await
        .with_dependency(task_a_id)
        .await
        .with_workspace("feature-b")
        .await
        .with_session()
        .await;

    let task_b_id = task_b_ctx.task_id();
    let workspace_b_id = task_b_ctx.workspace_id();

    // Mark Task A as Done
    update_task_status(&pool, task_a_id, TaskStatus::Done).await;

    // Verify is_blocked is false (DB trigger)
    let is_blocked = get_task_is_blocked(&pool, task_b_id).await;
    assert!(
        !is_blocked,
        "Task B should be unblocked after Task A completes"
    );

    // Dispatch event with autopilot DISABLED
    let config = autopilot_disabled_config();
    let ctx = test_handler_context(pool.clone(), config);

    let dispatcher = DispatcherBuilder::new()
        .with_handler(AutopilotHandler::new())
        .with_context(ctx)
        .build();

    let task_a = db::models::task::Task {
        id: task_a_id,
        project_id,
        title: "Task A".to_string(),
        description: None,
        status: TaskStatus::Done,
        parent_workspace_id: None,
        shared_task_id: None,
        task_group_id: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        is_blocked: false,
        has_in_progress_attempt: false,
        last_attempt_failed: false,
        is_queued: false,
        last_executor: String::new(),
        needs_attention: None,
    };

    dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task: task_a,
            previous_status: TaskStatus::InProgress,
        })
        .await;

    // Give spawned handler time to process
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Assert: Task B is NOT enqueued (autopilot disabled)
    let queue_entry = ExecutionQueue::find_by_workspace(&pool, workspace_b_id)
        .await
        .expect("DB query failed");
    assert!(
        queue_entry.is_none(),
        "Task B should NOT be enqueued when autopilot is disabled"
    );
}

#[tokio::test]
async fn test_dependent_without_workspace_skipped() {
    // Task B depends on A but has no workspace
    // A completes
    // Assert: B not enqueued (no workspace to run)
    let db = TestDb::new().await;
    let pool = db.pool().clone();

    // Create Task A with workspace and session
    let task_a_ctx = EntityGraphBuilder::new(pool.clone())
        .with_project("No Workspace Test")
        .create_task("Task A", TaskStatus::InProgress)
        .await
        .with_workspace("feature-a")
        .await
        .with_session()
        .await;

    let task_a_id = task_a_ctx.task_id();
    let project_id = task_a_ctx.project_id();

    // Create Task B WITHOUT workspace (just task and dependency)
    let task_b_ctx = task_a_ctx
        .builder()
        .create_task("Task B - No Workspace", TaskStatus::Todo)
        .await
        .with_dependency(task_a_id)
        .await;

    let task_b_id = task_b_ctx.task_id();

    // Mark Task A as Done
    update_task_status(&pool, task_a_id, TaskStatus::Done).await;

    // Verify Task B is unblocked (DB trigger)
    let is_blocked = get_task_is_blocked(&pool, task_b_id).await;
    assert!(
        !is_blocked,
        "Task B should be unblocked after Task A completes"
    );

    // Dispatch event
    let config = autopilot_config();
    let ctx = test_handler_context(pool.clone(), config);

    let dispatcher = DispatcherBuilder::new()
        .with_handler(AutopilotHandler::new())
        .with_context(ctx)
        .build();

    let task_a = db::models::task::Task {
        id: task_a_id,
        project_id,
        title: "Task A".to_string(),
        description: None,
        status: TaskStatus::Done,
        parent_workspace_id: None,
        shared_task_id: None,
        task_group_id: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        is_blocked: false,
        has_in_progress_attempt: false,
        last_attempt_failed: false,
        is_queued: false,
        last_executor: String::new(),
        needs_attention: None,
    };

    dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task: task_a,
            previous_status: TaskStatus::InProgress,
        })
        .await;

    // Give spawned handler time to process
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Assert: No execution queue entries for the whole test (Task B has no workspace)
    let queue_count = ExecutionQueue::count(&pool)
        .await
        .expect("DB query failed");
    assert_eq!(
        queue_count, 0,
        "No execution queue entries should exist (Task B has no workspace)"
    );
}
