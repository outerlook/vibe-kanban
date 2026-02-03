//! Tests for diamond dependency graph patterns.
//!
//! Diamond pattern:
//! ```text
//!     A
//!    / \
//!   B   C
//!    \ /
//!     D
//! ```
//!
//! These tests verify that D only unblocks when BOTH B and C are Done.

use db::models::{execution_queue::ExecutionQueue, task::TaskStatus};
use services::services::domain_events::{AutopilotHandler, DispatcherBuilder, DomainEvent};

use super::fixtures::{
    autopilot_config, dispatch_task_done, get_task_is_blocked, make_task_for_event,
    test_handler_context, update_task_needs_attention, update_task_status, EntityGraphBuilder,
    TestDb,
};

/// Test diamond pattern: D waits for both B and C
///
/// ```text
///     A
///    / \
///   B   C
///    \ /
///     D
/// ```
///
/// Flow:
/// 1. A completes → B and C both unblocked and enqueued
/// 2. B completes → D still blocked (waiting on C)
/// 3. C completes → D now unblocked and enqueued
#[tokio::test]
async fn test_diamond_d_waits_for_both_b_and_c() {
    let db = TestDb::new().await;
    let pool = db.pool().clone();

    // Create Task A (InProgress) - the root
    let task_a_ctx = EntityGraphBuilder::new(pool.clone())
        .with_project("Diamond Test Project")
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
        .create_task("Task B - Left Branch", TaskStatus::Todo)
        .await
        .with_dependency(task_a_id)
        .await
        .with_workspace("feature-b")
        .await
        .with_session()
        .await;

    let task_b_id = task_b_ctx.task_id();
    let workspace_b_id = task_b_ctx.workspace_id();

    // Create Task C (Todo, depends on A) with workspace
    let task_c_ctx = task_b_ctx
        .builder()
        .create_task("Task C - Right Branch", TaskStatus::Todo)
        .await
        .with_dependency(task_a_id)
        .await
        .with_workspace("feature-c")
        .await
        .with_session()
        .await;

    let task_c_id = task_c_ctx.task_id();
    let workspace_c_id = task_c_ctx.workspace_id();

    // Create Task D (Todo, depends on BOTH B and C) with workspace
    let task_d_ctx = task_c_ctx
        .builder()
        .create_task("Task D - Diamond Merge", TaskStatus::Todo)
        .await
        .with_dependency(task_b_id)
        .await
        .with_dependency(task_c_id)
        .await
        .with_workspace("feature-d")
        .await
        .with_session()
        .await;

    let task_d_id = task_d_ctx.task_id();
    let workspace_d_id = task_d_ctx.workspace_id();

    // Verify initial blocked states
    assert!(
        get_task_is_blocked(&pool, task_b_id).await,
        "Task B should be blocked initially"
    );
    assert!(
        get_task_is_blocked(&pool, task_c_id).await,
        "Task C should be blocked initially"
    );
    assert!(
        get_task_is_blocked(&pool, task_d_id).await,
        "Task D should be blocked initially"
    );

    // === Step 1: Complete Task A ===
    update_task_status(&pool, task_a_id, TaskStatus::Done).await;

    // Verify B and C are unblocked, D still blocked
    assert!(
        !get_task_is_blocked(&pool, task_b_id).await,
        "Task B should be unblocked after A completes"
    );
    assert!(
        !get_task_is_blocked(&pool, task_c_id).await,
        "Task C should be unblocked after A completes"
    );
    assert!(
        get_task_is_blocked(&pool, task_d_id).await,
        "Task D should still be blocked (B and C not done)"
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

    // Verify B and C are enqueued
    let queue_b = ExecutionQueue::find_by_workspace(&pool, workspace_b_id)
        .await
        .expect("DB query failed");
    let queue_c = ExecutionQueue::find_by_workspace(&pool, workspace_c_id)
        .await
        .expect("DB query failed");
    let queue_d = ExecutionQueue::find_by_workspace(&pool, workspace_d_id)
        .await
        .expect("DB query failed");

    assert!(queue_b.is_some(), "Task B should be enqueued after A done");
    assert!(queue_c.is_some(), "Task C should be enqueued after A done");
    assert!(
        queue_d.is_none(),
        "Task D should NOT be enqueued yet (B and C not done)"
    );

    // === Step 2: Complete Task B ===
    // First update status to InProgress (simulating work starting)
    update_task_status(&pool, task_b_id, TaskStatus::InProgress).await;
    // Then mark as Done
    update_task_status(&pool, task_b_id, TaskStatus::Done).await;

    // D should still be blocked (C not done)
    assert!(
        get_task_is_blocked(&pool, task_d_id).await,
        "Task D should still be blocked (C not done)"
    );

    // Dispatch event for B completing
    dispatch_task_done(
        &pool,
        task_b_id,
        project_id,
        "Task B - Left Branch",
        TaskStatus::InProgress,
    )
    .await;

    // D should still not be enqueued
    let queue_d = ExecutionQueue::find_by_workspace(&pool, workspace_d_id)
        .await
        .expect("DB query failed");
    assert!(
        queue_d.is_none(),
        "Task D should NOT be enqueued after only B completes"
    );

    // === Step 3: Complete Task C ===
    update_task_status(&pool, task_c_id, TaskStatus::InProgress).await;
    update_task_status(&pool, task_c_id, TaskStatus::Done).await;

    // D should now be unblocked
    assert!(
        !get_task_is_blocked(&pool, task_d_id).await,
        "Task D should be unblocked after both B and C complete"
    );

    // Dispatch event for C completing
    dispatch_task_done(
        &pool,
        task_c_id,
        project_id,
        "Task C - Right Branch",
        TaskStatus::InProgress,
    )
    .await;

    // D should now be enqueued
    let queue_d = ExecutionQueue::find_by_workspace(&pool, workspace_d_id)
        .await
        .expect("DB query failed");
    assert!(
        queue_d.is_some(),
        "Task D should be enqueued after both B and C complete"
    );
}

/// Test diamond pattern with partial completion: D never unblocks if C stays in review.
///
/// ```text
///     A
///    / \
///   B   C
///    \ /
///     D
/// ```
///
/// Flow:
/// 1. A completes
/// 2. B completes
/// 3. C stays in InReview with needs_attention=true
/// 4. D should never get unblocked or enqueued
#[tokio::test]
async fn test_diamond_partial_completion() {
    let db = TestDb::new().await;
    let pool = db.pool().clone();

    // Create Task A (InProgress) - the root
    let task_a_ctx = EntityGraphBuilder::new(pool.clone())
        .with_project("Diamond Partial Test")
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
        .create_task("Task B - Left Branch", TaskStatus::Todo)
        .await
        .with_dependency(task_a_id)
        .await
        .with_workspace("feature-b")
        .await
        .with_session()
        .await;

    let task_b_id = task_b_ctx.task_id();

    // Create Task C (Todo, depends on A) with workspace
    let task_c_ctx = task_b_ctx
        .builder()
        .create_task("Task C - Right Branch", TaskStatus::Todo)
        .await
        .with_dependency(task_a_id)
        .await
        .with_workspace("feature-c")
        .await
        .with_session()
        .await;

    let task_c_id = task_c_ctx.task_id();

    // Create Task D (Todo, depends on BOTH B and C) with workspace
    let task_d_ctx = task_c_ctx
        .builder()
        .create_task("Task D - Diamond Merge", TaskStatus::Todo)
        .await
        .with_dependency(task_b_id)
        .await
        .with_dependency(task_c_id)
        .await
        .with_workspace("feature-d")
        .await
        .with_session()
        .await;

    let task_d_id = task_d_ctx.task_id();
    let workspace_d_id = task_d_ctx.workspace_id();

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

    // === Step 2: Complete Task B ===
    update_task_status(&pool, task_b_id, TaskStatus::InProgress).await;
    update_task_status(&pool, task_b_id, TaskStatus::Done).await;
    dispatch_task_done(
        &pool,
        task_b_id,
        project_id,
        "Task B - Left Branch",
        TaskStatus::InProgress,
    )
    .await;

    // === Step 3: Move Task C to InReview with needs_attention ===
    update_task_status(&pool, task_c_id, TaskStatus::InProgress).await;
    update_task_status(&pool, task_c_id, TaskStatus::InReview).await;
    update_task_needs_attention(&pool, task_c_id, true).await;

    // D should still be blocked (C is InReview, not Done)
    assert!(
        get_task_is_blocked(&pool, task_d_id).await,
        "Task D should still be blocked (C is InReview, not Done)"
    );

    // D should not be enqueued
    let queue_d = ExecutionQueue::find_by_workspace(&pool, workspace_d_id)
        .await
        .expect("DB query failed");
    assert!(
        queue_d.is_none(),
        "Task D should NOT be enqueued when C is stuck in InReview"
    );

    // Even if we dispatch an event for C (though it's not completing), D shouldn't unblock
    // This verifies that status InReview doesn't trigger dependent unblocking
    let config = autopilot_config();
    let ctx = test_handler_context(pool.clone(), config);

    let dispatcher = DispatcherBuilder::new()
        .with_handler(AutopilotHandler::new())
        .with_context(ctx)
        .build();

    let task_c = make_task_for_event(
        task_c_id,
        project_id,
        "Task C - Right Branch",
        TaskStatus::InReview,
    );

    dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task: task_c,
            previous_status: TaskStatus::InProgress,
        })
        .await;

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // D should still be blocked and not enqueued
    assert!(
        get_task_is_blocked(&pool, task_d_id).await,
        "Task D should remain blocked after C moves to InReview"
    );

    let queue_d = ExecutionQueue::find_by_workspace(&pool, workspace_d_id)
        .await
        .expect("DB query failed");
    assert!(
        queue_d.is_none(),
        "Task D should NOT be enqueued after C moves to InReview"
    );
}
