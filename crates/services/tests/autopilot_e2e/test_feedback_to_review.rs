//! Tests for the feedback-to-review flow in autopilot.
//!
//! These tests verify the first part of the autopilot flow:
//! - Agent completes → FeedbackCollectionHandler → task moves to InReview
//! - Task moves to InReview → ReviewAttentionHandler triggers

use std::sync::Arc;
use std::time::Duration;

use db::models::{agent_feedback::CreateAgentFeedback, task::TaskStatus};
use services::services::domain_events::{
    DomainEvent, DispatcherBuilder, ExecutionTrigger, HandlerContext,
    handlers::{FeedbackCollectionHandler, ReviewAttentionHandler},
};
use utils::msg_store::MsgStore;
use uuid::Uuid;

use super::fixtures::{EntityGraphBuilder, TestDb, autopilot_config, create_task};
use super::mock_execution_controller::MockExecutionController;

/// Test that a completed CodingAgent execution triggers feedback collection.
///
/// Setup:
/// - Project, task (InProgress), workspace, session, completed CodingAgent execution
///
/// Action:
/// - Dispatch ExecutionCompleted event
///
/// Assert:
/// - FeedbackCollection trigger captured by MockExecutionController
#[tokio::test]
async fn test_agent_completes_triggers_feedback_collection() {
    let test_db = TestDb::new().await;
    let pool = test_db.pool().clone();

    // Setup: Create project → task → workspace → session → completed CodingAgent execution
    let ctx = EntityGraphBuilder::new(pool.clone())
        .with_project("Feedback Flow Project")
        .create_task("Implement feature", TaskStatus::InProgress)
        .await
        .with_workspace("feature/test-branch")
        .await
        .with_session()
        .await
        .with_completed_coding_execution()
        .await;

    let task_id = ctx.task_id();
    let workspace_id = ctx.workspace_id();
    let execution = ctx.execution().expect("execution should exist").clone();

    // Setup dispatcher with FeedbackCollectionHandler and MockExecutionController
    let controller = MockExecutionController::new(pool.clone());
    let callback = controller.callback();

    let db_service = db::DBService { pool: pool.clone() };
    let config = autopilot_config();
    let msg_store = Arc::new(MsgStore::default());

    let dispatcher = DispatcherBuilder::new()
        .with_handler(FeedbackCollectionHandler::new(db_service.clone()))
        .with_context(HandlerContext::new(
            db_service,
            config,
            msg_store,
            Some(callback),
        ))
        .build();

    // Dispatch ExecutionCompleted event
    dispatcher
        .dispatch(DomainEvent::ExecutionCompleted {
            process: execution.clone(),
            task_id,
        })
        .await;

    // Wait for spawned handler to complete
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify FeedbackCollection trigger was captured
    let captures = controller.get_captures();
    assert_eq!(captures.len(), 1, "Expected exactly one trigger");

    match &captures[0] {
        ExecutionTrigger::FeedbackCollection {
            workspace_id: ws_id,
            task_id: t_id,
            execution_process_id: ep_id,
        } => {
            assert_eq!(ws_id, &workspace_id, "Workspace ID should match");
            assert_eq!(t_id, &task_id, "Task ID should match");
            assert_eq!(ep_id, &execution.id, "Execution process ID should match");
        }
        _ => panic!("Expected FeedbackCollection trigger, got {:?}", captures[0]),
    }
}

/// Test that moving a task to InReview triggers ReviewAttention.
///
/// Setup:
/// - Task with completed CodingAgent execution and workspace
///
/// Action:
/// - Update task status to InReview, dispatch TaskStatusChanged
///
/// Assert:
/// - ReviewAttention trigger captured
#[tokio::test]
async fn test_task_inreview_triggers_review_attention() {
    let test_db = TestDb::new().await;
    let pool = test_db.pool().clone();

    // Setup: Create project → task → workspace → session → completed CodingAgent execution
    let ctx = EntityGraphBuilder::new(pool.clone())
        .with_project("Review Attention Project")
        .create_task("Review task", TaskStatus::InProgress)
        .await
        .with_workspace("feature/review-branch")
        .await
        .with_session()
        .await
        .with_completed_coding_execution()
        .await;

    let task_id = ctx.task_id();
    let execution = ctx.execution().expect("execution should exist").clone();

    // Create a task object with InReview status for the event
    let mut task_in_review = ctx.task().clone();
    task_in_review.status = TaskStatus::InReview;

    // Setup dispatcher with ReviewAttentionHandler and MockExecutionController
    let controller = MockExecutionController::new(pool.clone());
    let callback = controller.callback();

    let db_service = db::DBService { pool: pool.clone() };
    let config = autopilot_config();
    let msg_store = Arc::new(MsgStore::default());

    let dispatcher = DispatcherBuilder::new()
        .with_handler(ReviewAttentionHandler::new())
        .with_context(HandlerContext::new(
            db_service,
            config,
            msg_store,
            Some(callback),
        ))
        .build();

    // Dispatch TaskStatusChanged event (task moved to InReview)
    dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task: task_in_review,
            previous_status: TaskStatus::InProgress,
        })
        .await;

    // Wait for spawned handler to complete
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify ReviewAttention trigger was captured
    let captures = controller.get_captures();
    assert_eq!(captures.len(), 1, "Expected exactly one trigger");

    match &captures[0] {
        ExecutionTrigger::ReviewAttention {
            task_id: t_id,
            execution_process_id: ep_id,
        } => {
            assert_eq!(t_id, &task_id, "Task ID should match");
            assert_eq!(
                ep_id, &execution.id,
                "Execution process ID should match latest CodingAgent"
            );
        }
        _ => panic!("Expected ReviewAttention trigger, got {:?}", captures[0]),
    }
}

/// Test that feedback collection is skipped if feedback already exists.
///
/// Setup:
/// - Task with completed execution and pre-existing AgentFeedback record
///
/// Action:
/// - Dispatch ExecutionCompleted
///
/// Assert:
/// - No trigger (feedback already exists)
#[tokio::test]
async fn test_feedback_skipped_if_already_exists() {
    let test_db = TestDb::new().await;
    let pool = test_db.pool().clone();

    // Setup: Create project → task → workspace → session → completed CodingAgent execution
    let ctx = EntityGraphBuilder::new(pool.clone())
        .with_project("Skip Feedback Project")
        .create_task("Already has feedback", TaskStatus::InProgress)
        .await
        .with_workspace("feature/existing-feedback")
        .await
        .with_session()
        .await
        .with_completed_coding_execution()
        .await;

    let task_id = ctx.task_id();
    let workspace_id = ctx.workspace_id();
    let execution = ctx.execution().expect("execution should exist").clone();

    // Pre-create AgentFeedback record for this workspace
    let feedback_data = CreateAgentFeedback {
        execution_process_id: execution.id,
        task_id,
        workspace_id,
        feedback_json: Some(r#"{"status": "completed"}"#.to_string()),
    };
    db::models::agent_feedback::AgentFeedback::create(&pool, &feedback_data, Uuid::new_v4())
        .await
        .expect("Failed to create feedback");

    // Setup dispatcher with FeedbackCollectionHandler
    let controller = MockExecutionController::new(pool.clone());
    let callback = controller.callback();

    let db_service = db::DBService { pool: pool.clone() };
    let config = autopilot_config();
    let msg_store = Arc::new(MsgStore::default());

    let dispatcher = DispatcherBuilder::new()
        .with_handler(FeedbackCollectionHandler::new(db_service.clone()))
        .with_context(HandlerContext::new(
            db_service,
            config,
            msg_store,
            Some(callback),
        ))
        .build();

    // Dispatch ExecutionCompleted event
    dispatcher
        .dispatch(DomainEvent::ExecutionCompleted {
            process: execution,
            task_id,
        })
        .await;

    // Wait for spawned handler
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify no trigger was captured (feedback already exists, so collection is skipped)
    let captures = controller.get_captures();
    assert!(
        captures.is_empty(),
        "Expected no triggers when feedback already exists, got {} triggers",
        captures.len()
    );
}

/// Test that ReviewAttention is skipped when task has no workspace.
///
/// Setup:
/// - Task without workspace
///
/// Action:
/// - Dispatch TaskStatusChanged to InReview
///
/// Assert:
/// - No trigger, handler returns Ok
#[tokio::test]
async fn test_review_attention_skipped_without_workspace() {
    let test_db = TestDb::new().await;
    let pool = test_db.pool().clone();

    // Setup: Create project → task (no workspace)
    let project_id = super::fixtures::create_project(&pool, "No Workspace Project").await;
    let task = create_task(&pool, project_id, "Task without workspace", TaskStatus::InProgress).await;

    // Create task object with InReview status for the event
    let mut task_in_review = task.clone();
    task_in_review.status = TaskStatus::InReview;

    // Setup dispatcher with ReviewAttentionHandler
    let controller = MockExecutionController::new(pool.clone());
    let callback = controller.callback();

    let db_service = db::DBService { pool: pool.clone() };
    let config = autopilot_config();
    let msg_store = Arc::new(MsgStore::default());

    let dispatcher = DispatcherBuilder::new()
        .with_handler(ReviewAttentionHandler::new())
        .with_context(HandlerContext::new(
            db_service,
            config,
            msg_store,
            Some(callback),
        ))
        .build();

    // Dispatch TaskStatusChanged event (task moved to InReview)
    dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task: task_in_review,
            previous_status: TaskStatus::InProgress,
        })
        .await;

    // Wait for spawned handler
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify no trigger was captured (task has no workspace)
    let captures = controller.get_captures();
    assert!(
        captures.is_empty(),
        "Expected no triggers when task has no workspace, got {} triggers",
        captures.len()
    );
}
