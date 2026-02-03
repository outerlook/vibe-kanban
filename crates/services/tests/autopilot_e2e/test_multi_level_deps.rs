//! Tests for multi-level dependency chain propagation (A→B→C).
//!
//! These tests verify that the autopilot flow correctly propagates through
//! multi-level dependency chains, where C depends on B, and B depends on A.

use db::models::{execution_queue::ExecutionQueue, task::TaskStatus};
use services::services::domain_events::{AutopilotHandler, DispatcherBuilder, DomainEvent};

use super::fixtures::{
    autopilot_config, dispatch_task_done, get_task_is_blocked, make_task_for_event,
    test_handler_context, update_task_needs_attention, update_task_status, EntityGraphBuilder,
    TestDb,
};

/// Test three-level dependency chain: A → B → C
///
/// ```text
/// A → B → C
/// ```
///
/// Flow:
/// 1. A completes (Done) → dispatch event → AutopilotHandler
/// 2. Assert: B unblocked and enqueued, C still blocked
/// 3. B completes (Done) → dispatch event
/// 4. Assert: C now unblocked and enqueued
#[tokio::test]
async fn test_three_level_chain_propagates() {
    let db = TestDb::new().await;
    let pool = db.pool().clone();

    // Create Task A (InProgress) - the root of the chain
    let task_a_ctx = EntityGraphBuilder::new(pool.clone())
        .with_project("Multi-Level Chain Test")
        .create_task("Task A - Root", TaskStatus::InProgress)
        .await
        .with_workspace("feature-a")
        .await
        .with_session()
        .await;

    let task_a_id = task_a_ctx.task_id();
    let project_id = task_a_ctx.project_id();

    // Create Task B (Todo, depends on A) with workspace
    let task_b_ctx = task_a_ctx
        .builder()
        .create_task("Task B - Middle", TaskStatus::Todo)
        .await
        .with_dependency(task_a_id)
        .await
        .with_workspace("feature-b")
        .await
        .with_session()
        .await;

    let task_b_id = task_b_ctx.task_id();
    let workspace_b_id = task_b_ctx.workspace_id();

    // Create Task C (Todo, depends on B) with workspace
    let task_c_ctx = task_b_ctx
        .builder()
        .create_task("Task C - End", TaskStatus::Todo)
        .await
        .with_dependency(task_b_id)
        .await
        .with_workspace("feature-c")
        .await
        .with_session()
        .await;

    let task_c_id = task_c_ctx.task_id();
    let workspace_c_id = task_c_ctx.workspace_id();

    // Verify initial blocked states
    assert!(
        get_task_is_blocked(&pool, task_b_id).await,
        "Task B should be blocked initially (A not done)"
    );
    assert!(
        get_task_is_blocked(&pool, task_c_id).await,
        "Task C should be blocked initially (B not done)"
    );

    // Verify no execution queue entries initially
    let queue_b = ExecutionQueue::find_by_workspace(&pool, workspace_b_id)
        .await
        .expect("DB query failed");
    let queue_c = ExecutionQueue::find_by_workspace(&pool, workspace_c_id)
        .await
        .expect("DB query failed");
    assert!(queue_b.is_none(), "Task B should not be enqueued initially");
    assert!(queue_c.is_none(), "Task C should not be enqueued initially");

    // === Step 1: Complete Task A ===
    update_task_status(&pool, task_a_id, TaskStatus::Done).await;

    // Verify B is unblocked, C still blocked
    assert!(
        !get_task_is_blocked(&pool, task_b_id).await,
        "Task B should be unblocked after A completes"
    );
    assert!(
        get_task_is_blocked(&pool, task_c_id).await,
        "Task C should still be blocked (B not done)"
    );

    // Dispatch event for A completing
    dispatch_task_done(
        &pool,
        task_a_id,
        project_id,
        "Task A - Root",
        TaskStatus::InProgress,
    )
    .await;

    // Verify B is enqueued, C is not
    let queue_b = ExecutionQueue::find_by_workspace(&pool, workspace_b_id)
        .await
        .expect("DB query failed");
    let queue_c = ExecutionQueue::find_by_workspace(&pool, workspace_c_id)
        .await
        .expect("DB query failed");

    assert!(
        queue_b.is_some(),
        "Task B should be enqueued after A completes"
    );
    assert!(
        queue_c.is_none(),
        "Task C should NOT be enqueued yet (B not done)"
    );

    // === Step 2: Complete Task B ===
    // First update status to InProgress (simulating work starting)
    update_task_status(&pool, task_b_id, TaskStatus::InProgress).await;
    // Then mark as Done
    update_task_status(&pool, task_b_id, TaskStatus::Done).await;

    // Verify C is now unblocked
    assert!(
        !get_task_is_blocked(&pool, task_c_id).await,
        "Task C should be unblocked after B completes"
    );

    // Dispatch event for B completing
    dispatch_task_done(
        &pool,
        task_b_id,
        project_id,
        "Task B - Middle",
        TaskStatus::InProgress,
    )
    .await;

    // Verify C is now enqueued
    let queue_c = ExecutionQueue::find_by_workspace(&pool, workspace_c_id)
        .await
        .expect("DB query failed");

    assert!(
        queue_c.is_some(),
        "Task C should be enqueued after B completes"
    );
}

/// Test chain stops propagating if middle task needs attention.
///
/// ```text
/// A → B → C
/// ```
///
/// Flow:
/// 1. A completes → B unblocked and enqueued
/// 2. B starts (InProgress) then moves to InReview with needs_attention=true
/// 3. Assert: B stays in InReview, C remains blocked, C not enqueued
#[tokio::test]
async fn test_chain_stops_if_middle_needs_attention() {
    let db = TestDb::new().await;
    let pool = db.pool().clone();

    // Create Task A (InProgress) - the root
    let task_a_ctx = EntityGraphBuilder::new(pool.clone())
        .with_project("Chain Stops Test")
        .create_task("Task A - Root", TaskStatus::InProgress)
        .await
        .with_workspace("feature-a")
        .await
        .with_session()
        .await;

    let task_a_id = task_a_ctx.task_id();
    let project_id = task_a_ctx.project_id();

    // Create Task B (Todo, depends on A) with workspace
    let task_b_ctx = task_a_ctx
        .builder()
        .create_task("Task B - Middle", TaskStatus::Todo)
        .await
        .with_dependency(task_a_id)
        .await
        .with_workspace("feature-b")
        .await
        .with_session()
        .await;

    let task_b_id = task_b_ctx.task_id();
    let workspace_b_id = task_b_ctx.workspace_id();

    // Create Task C (Todo, depends on B) with workspace
    let task_c_ctx = task_b_ctx
        .builder()
        .create_task("Task C - End", TaskStatus::Todo)
        .await
        .with_dependency(task_b_id)
        .await
        .with_workspace("feature-c")
        .await
        .with_session()
        .await;

    let task_c_id = task_c_ctx.task_id();
    let workspace_c_id = task_c_ctx.workspace_id();

    // === Step 1: Complete Task A ===
    update_task_status(&pool, task_a_id, TaskStatus::Done).await;
    dispatch_task_done(
        &pool,
        task_a_id,
        project_id,
        "Task A - Root",
        TaskStatus::InProgress,
    )
    .await;

    // Verify B is enqueued
    let queue_b = ExecutionQueue::find_by_workspace(&pool, workspace_b_id)
        .await
        .expect("DB query failed");
    assert!(queue_b.is_some(), "Task B should be enqueued after A done");

    // === Step 2: Task B starts work then needs attention ===
    update_task_status(&pool, task_b_id, TaskStatus::InProgress).await;
    update_task_status(&pool, task_b_id, TaskStatus::InReview).await;
    update_task_needs_attention(&pool, task_b_id, true).await;

    // C should still be blocked (B is InReview, not Done)
    assert!(
        get_task_is_blocked(&pool, task_c_id).await,
        "Task C should still be blocked (B is InReview, not Done)"
    );

    // C should not be enqueued
    let queue_c = ExecutionQueue::find_by_workspace(&pool, workspace_c_id)
        .await
        .expect("DB query failed");
    assert!(
        queue_c.is_none(),
        "Task C should NOT be enqueued when B needs attention"
    );

    // Dispatch an event for B moving to InReview (this shouldn't trigger C)
    let config = autopilot_config();
    let ctx = test_handler_context(pool.clone(), config);

    let dispatcher = DispatcherBuilder::new()
        .with_handler(AutopilotHandler::new())
        .with_context(ctx)
        .build();

    let task_b = make_task_for_event(task_b_id, project_id, "Task B - Middle", TaskStatus::InReview);

    dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task: task_b,
            previous_status: TaskStatus::InProgress,
        })
        .await;

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // C should still be blocked and not enqueued
    assert!(
        get_task_is_blocked(&pool, task_c_id).await,
        "Task C should remain blocked after B moves to InReview"
    );

    let queue_c = ExecutionQueue::find_by_workspace(&pool, workspace_c_id)
        .await
        .expect("DB query failed");
    assert!(
        queue_c.is_none(),
        "Task C should NOT be enqueued after B moves to InReview"
    );
}
