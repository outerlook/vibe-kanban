//! Comprehensive E2E test for the full autopilot flow.
//!
//! Tests the complete autopilot lifecycle:
//! 1. Agent completes → Feedback collection trigger
//! 2. Task moves to InReview → Review attention trigger
//! 3. Review says needs_attention=false → Merge queue enqueue
//! 4. Merge succeeds → Task Done
//! 5. Dependent task unblocked and enqueued

use std::sync::Arc;
use std::time::Duration;

use db::models::{
    execution_queue::ExecutionQueue,
    merge::Merge,
    task::{Task, TaskStatus},
};
use services::services::{
    domain_events::{
        AutopilotHandler, DispatcherBuilder, DomainEvent, ExecutionTrigger, HandlerContext,
        handlers::{FeedbackCollectionHandler, ReviewAttentionHandler},
    },
    review_attention::ReviewAttentionService,
};
use utils::msg_store::MsgStore;

use super::fixtures::{
    autopilot_config,
    git_fixtures::{
        MergeTestContext, TestRepo, add_and_commit, create_repo, create_workspace_repo,
        update_workspace_container_ref,
    },
    update_task_status, EntityGraphBuilder,
};
use super::mock_execution_controller::MockExecutionController;

/// Full autopilot flow E2E test.
///
/// This test exercises the complete autopilot workflow:
///
/// ```text
/// Agent completes
///       ↓
/// FeedbackCollection trigger
///       ↓
/// Task → InReview
///       ↓
/// ReviewAttention trigger
///       ↓
/// needs_attention=false
///       ↓
/// Merge queue enqueue
///       ↓
/// Merge succeeds
///       ↓
/// Task → Done
///       ↓
/// Dependent task unblocked → enqueued
/// ```
#[tokio::test]
async fn test_full_autopilot_flow_with_dependent_task() {
    // === Setup ===
    let ctx = MergeTestContext::new().await;
    let test_repo = TestRepo::new("full-flow-repo");

    // Create Task A (InProgress) with workspace, session, and completed execution
    let task_a_ctx = EntityGraphBuilder::new(ctx.pool.clone())
        .with_project("Full Flow Project")
        .create_task("Task A - Primary", TaskStatus::InProgress)
        .await
        .with_workspace("feature-a")
        .await
        .with_session()
        .await
        .with_completed_coding_execution()
        .await;

    let task_a_id = task_a_ctx.task_id();
    let workspace_a_id = task_a_ctx.workspace_id();
    let project_id = task_a_ctx.project_id();
    let execution_a = task_a_ctx.execution().expect("execution should exist").clone();
    let task_a = task_a_ctx.task().clone();

    // Create Task B (Todo, depends on A) with workspace
    let task_b_ctx = task_a_ctx
        .builder()
        .create_task("Task B - Dependent", TaskStatus::Todo)
        .await
        .with_dependency(task_a_id)
        .await
        .with_workspace("feature-b")
        .await
        .with_session()
        .await;

    let task_b_id = task_b_ctx.task_id();
    let workspace_b_id = task_b_ctx.workspace_id();

    // Create repo in database
    let repo_id = create_repo(&ctx.pool, &test_repo.path, &test_repo.name).await;

    // Create worktree for Task A's branch
    let worktree_a_path = test_repo.create_worktree("feature-a");
    update_workspace_container_ref(
        &ctx.pool,
        workspace_a_id,
        &worktree_a_path.parent().unwrap().to_string_lossy(),
    )
    .await;

    // Link workspace to repo with target branch = main
    create_workspace_repo(&ctx.pool, workspace_a_id, repo_id, "main").await;

    // Add a commit on Task A's branch (simulating work done)
    add_and_commit(
        &worktree_a_path,
        "feature_a.txt",
        "Feature A implementation",
        "Implement feature A",
    );

    // Verify initial state: Task B is blocked
    let task_b = Task::find_by_id(&ctx.pool, task_b_id)
        .await
        .expect("Query should succeed")
        .expect("Task B should exist");
    assert!(task_b.is_blocked, "Task B should be blocked initially");

    // === Step 1: Agent completes → FeedbackCollection trigger ===
    let controller = MockExecutionController::new(ctx.pool.clone());
    let callback = controller.callback();

    let db_service = db::DBService {
        pool: ctx.pool.clone(),
    };
    let config = autopilot_config();
    let msg_store = Arc::new(MsgStore::default());

    let feedback_dispatcher = DispatcherBuilder::new()
        .with_handler(FeedbackCollectionHandler::new(db_service.clone()))
        .with_context(HandlerContext::new(
            db_service.clone(),
            config.clone(),
            msg_store.clone(),
            Some(callback.clone()),
        ))
        .build();

    // Dispatch ExecutionCompleted event
    feedback_dispatcher
        .dispatch(DomainEvent::ExecutionCompleted {
            process: execution_a.clone(),
            task_id: task_a_id,
        })
        .await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify FeedbackCollection trigger was captured
    let captures = controller.get_captures();
    assert_eq!(captures.len(), 1, "Expected FeedbackCollection trigger");
    match &captures[0] {
        ExecutionTrigger::FeedbackCollection { task_id, .. } => {
            assert_eq!(task_id, &task_a_id, "Trigger should be for Task A");
        }
        _ => panic!("Expected FeedbackCollection trigger"),
    }

    // === Step 2: Task moves to InReview → ReviewAttention trigger ===
    // Update task status to InReview
    update_task_status(&ctx.pool, task_a_id, TaskStatus::InReview).await;

    let controller2 = MockExecutionController::new(ctx.pool.clone());
    let callback2 = controller2.callback();

    let review_dispatcher = DispatcherBuilder::new()
        .with_handler(ReviewAttentionHandler::new())
        .with_context(HandlerContext::new(
            db_service.clone(),
            config.clone(),
            msg_store.clone(),
            Some(callback2),
        ))
        .build();

    // Create task object with InReview status for the event
    let mut task_a_inreview = task_a.clone();
    task_a_inreview.status = TaskStatus::InReview;

    review_dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task: task_a_inreview,
            previous_status: TaskStatus::InProgress,
        })
        .await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify ReviewAttention trigger was captured
    let captures2 = controller2.get_captures();
    assert_eq!(captures2.len(), 1, "Expected ReviewAttention trigger");
    match &captures2[0] {
        ExecutionTrigger::ReviewAttention { task_id, .. } => {
            assert_eq!(task_id, &task_a_id, "Trigger should be for Task A");
        }
        _ => panic!("Expected ReviewAttention trigger"),
    }

    // === Step 3: Review says needs_attention=false → Merge queue enqueue ===
    // Simulate review response
    let review_response = r#"{"needs_attention": false, "reasoning": "All tests pass"}"#;
    let parsed = ReviewAttentionService::parse_review_attention_response(review_response)
        .expect("Should parse review response");
    assert!(!parsed.needs_attention);

    // Update task's needs_attention
    Task::update_needs_attention(&ctx.pool, task_a_id, Some(false))
        .await
        .expect("Failed to update needs_attention");

    // Enqueue to merge queue
    let commit_message = "Merge feature-a: All tests pass";
    ctx.merge_queue_store.enqueue(
        project_id,
        workspace_a_id,
        repo_id,
        commit_message.to_string(),
    );

    // Verify entry is in queue
    assert_eq!(ctx.merge_queue_store.count_by_project(project_id), 1);

    // === Step 4: Merge succeeds → Task Done ===
    let processor = ctx.processor();
    processor
        .process_project_queue(project_id)
        .await
        .expect("Merge queue processing should succeed");

    // Verify Task A is Done
    let task_a = Task::find_by_id(&ctx.pool, task_a_id)
        .await
        .expect("Query should succeed")
        .expect("Task A should exist");
    assert_eq!(task_a.status, TaskStatus::Done, "Task A should be Done");

    // Verify merge record was created
    let merges = Merge::find_by_workspace_id(&ctx.pool, workspace_a_id)
        .await
        .expect("Query should succeed");
    assert_eq!(merges.len(), 1, "Should have one merge record");

    // Verify queue is empty
    assert_eq!(ctx.merge_queue_store.count_by_project(project_id), 0);

    // === Step 5: Dependent task unblocked → enqueued ===
    // Dispatch TaskStatusChanged for Task A → Done to trigger AutopilotHandler
    let autopilot_dispatcher = DispatcherBuilder::new()
        .with_handler(AutopilotHandler::new())
        .with_context(HandlerContext::new(
            db_service,
            config,
            msg_store,
            Some(callback),
        ))
        .build();

    // Create Task A with Done status for the event
    let mut task_a_done = task_a.clone();
    task_a_done.status = TaskStatus::Done;

    autopilot_dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task: task_a_done,
            previous_status: TaskStatus::InReview,
        })
        .await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify Task B is unblocked
    let task_b = Task::find_by_id(&ctx.pool, task_b_id)
        .await
        .expect("Query should succeed")
        .expect("Task B should exist");
    assert!(
        !task_b.is_blocked,
        "Task B should be unblocked after Task A completes"
    );

    // Verify Task B is enqueued for execution
    let queue_b = ExecutionQueue::find_by_workspace(&ctx.pool, workspace_b_id)
        .await
        .expect("Query should succeed");
    assert!(
        queue_b.is_some(),
        "Task B should be enqueued after Task A completes"
    );
}
