//! Integration tests for handler-to-callback trigger flow.
//!
//! These tests verify that:
//! 1. FeedbackCollectionHandler triggers ExecutionTrigger::FeedbackCollection when
//!    a CodingAgent execution completes successfully.
//! 2. ReviewAttentionHandler triggers ExecutionTrigger::ReviewAttention when a task
//!    status changes to InReview.
//! 3. No duplicate triggers occur (only handlers trigger, not inline code).
//! 4. The correct trigger variant is dispatched with expected data.
//!
//! These tests use mock callbacks to capture triggers without needing the full
//! LocalContainerService infrastructure.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use db::models::{
    execution_process::{
        ExecutionProcess, ExecutionProcessRunReason, ExecutionProcessStatus, ExecutorActionField,
    },
    task::{Task, TaskStatus},
    workspace::Workspace,
};
use executors::actions::{
    ExecutorAction, ExecutorActionType,
    script::{ScriptContext, ScriptRequest, ScriptRequestLanguage},
};
use futures::FutureExt;
use services::services::domain_events::{
    DispatcherBuilder, DomainEvent, EventHandler, ExecutionTrigger, ExecutionTriggerCallback,
    HandlerContext,
    handlers::{FeedbackCollectionHandler, ReviewAttentionHandler},
};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use tempfile::NamedTempFile;
use tokio::sync::{Mutex, RwLock};
use utils::msg_store::MsgStore;
use uuid::Uuid;

// ============================================================================
// Test Setup Helpers
// ============================================================================

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
async fn create_test_task(
    pool: &SqlitePool,
    project_id: Uuid,
    title: &str,
    status: TaskStatus,
) -> Task {
    let id = Uuid::new_v4();
    let status_str = match status {
        TaskStatus::Todo => "todo",
        TaskStatus::InProgress => "inprogress",
        TaskStatus::InReview => "inreview",
        TaskStatus::Done => "done",
        TaskStatus::Cancelled => "cancelled",
    };
    let now = chrono::Utc::now();

    sqlx::query(
        "INSERT INTO tasks (id, project_id, title, status, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(project_id)
    .bind(title)
    .bind(status_str)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .expect("Failed to create task");

    Task {
        id,
        project_id,
        title: title.to_string(),
        description: None,
        status,
        parent_workspace_id: None,
        shared_task_id: None,
        task_group_id: None,
        created_at: now,
        updated_at: now,
        is_blocked: false,
        has_in_progress_attempt: false,
        last_attempt_failed: false,
        is_queued: false,
        last_executor: String::new(),
        needs_attention: None,
    }
}

/// Creates a test workspace in the database.
async fn create_test_workspace(pool: &SqlitePool, task_id: Uuid, branch: &str) -> Workspace {
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

    Workspace {
        id,
        task_id,
        container_ref: None,
        branch: branch.to_string(),
        agent_working_dir: None,
        setup_completed_at: None,
        created_at: now,
        updated_at: now,
    }
}

/// Creates a test session in the database.
async fn create_test_session(pool: &SqlitePool, workspace_id: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO sessions (id, workspace_id, executor) VALUES (?, ?, ?)")
        .bind(id)
        .bind(workspace_id)
        .bind("claude_code") // Required by trigger that updates tasks.last_executor
        .execute(pool)
        .await
        .expect("Failed to create session");
    id
}

/// Creates a test execution process in the database.
async fn create_test_execution_process(
    pool: &SqlitePool,
    session_id: Uuid,
    status: ExecutionProcessStatus,
    run_reason: ExecutionProcessRunReason,
) -> ExecutionProcess {
    let id = Uuid::new_v4();
    let status_str = match &status {
        ExecutionProcessStatus::Running => "running",
        ExecutionProcessStatus::Completed => "completed",
        ExecutionProcessStatus::Failed => "failed",
        ExecutionProcessStatus::Killed => "killed",
    };
    let run_reason_str = match &run_reason {
        ExecutionProcessRunReason::CodingAgent => "codingagent",
        ExecutionProcessRunReason::SetupScript => "setupscript",
        ExecutionProcessRunReason::CleanupScript => "cleanupscript",
        ExecutionProcessRunReason::InternalAgent => "internalagent",
        ExecutionProcessRunReason::DevServer => "devserver",
        ExecutionProcessRunReason::DisposableConversation => "disposableconversation",
    };
    let now = chrono::Utc::now();
    let script_request = ScriptRequest {
        script: "echo test".to_string(),
        language: ScriptRequestLanguage::Bash,
        context: ScriptContext::SetupScript,
        working_dir: None,
    };
    let executor_action =
        ExecutorAction::new(ExecutorActionType::ScriptRequest(script_request), None);
    let executor_action_json =
        serde_json::to_string(&executor_action).expect("serialize executor action");

    sqlx::query(
        "INSERT INTO execution_processes (id, session_id, status, run_reason, executor_action, started_at, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(session_id)
    .bind(status_str)
    .bind(run_reason_str)
    .bind(&executor_action_json)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .expect("Failed to create execution process");

    ExecutionProcess {
        id,
        session_id: Some(session_id),
        conversation_session_id: None,
        status,
        run_reason,
        executor_action: sqlx::types::Json(ExecutorActionField::ExecutorAction(executor_action)),
        exit_code: None,
        dropped: false,
        input_tokens: None,
        output_tokens: None,
        started_at: now,
        completed_at: None,
        created_at: now,
        updated_at: now,
    }
}

/// Container for captured triggers during tests.
#[derive(Default)]
struct TriggerCapture {
    feedback_triggers: Vec<(Uuid, Uuid, Uuid)>, // (workspace_id, task_id, execution_process_id)
    review_attention_triggers: Vec<(Uuid, Uuid)>, // (task_id, execution_process_id)
}

/// Creates a mock execution trigger callback that captures all triggers.
fn create_mock_trigger_callback(capture: Arc<Mutex<TriggerCapture>>) -> ExecutionTriggerCallback {
    Arc::new(move |trigger: ExecutionTrigger| {
        let capture = Arc::clone(&capture);
        async move {
            let mut cap = capture.lock().await;
            match trigger {
                ExecutionTrigger::FeedbackCollection {
                    workspace_id,
                    task_id,
                    execution_process_id,
                } => {
                    cap.feedback_triggers
                        .push((workspace_id, task_id, execution_process_id));
                }
                ExecutionTrigger::ReviewAttention {
                    task_id,
                    execution_process_id,
                } => {
                    cap.review_attention_triggers
                        .push((task_id, execution_process_id));
                }
            }
            Ok(Uuid::new_v4())
        }
        .boxed()
    })
}

/// Creates a HandlerContext with a mock trigger callback.
fn test_context_with_trigger(
    pool: SqlitePool,
    trigger_callback: ExecutionTriggerCallback,
) -> HandlerContext {
    let db = db::DBService { pool };
    let config = Arc::new(RwLock::new(services::services::config::Config::default()));
    let msg_store = Arc::new(MsgStore::default());
    HandlerContext::new(db, config, msg_store, Some(trigger_callback))
}

// ============================================================================
// FeedbackCollectionHandler Integration Tests
// ============================================================================

/// Test that FeedbackCollectionHandler triggers FeedbackCollection callback when:
/// - ExecutionCompleted event is dispatched
/// - Execution status is Completed
/// - Run reason is CodingAgent
#[tokio::test]
async fn test_feedback_handler_triggers_callback_on_coding_agent_completion() {
    let (pool, _db_file) = create_test_db().await;

    // Setup: Create project, task, workspace, session, execution
    let project_id = create_test_project(&pool, "Feedback Test Project").await;
    let task = create_test_task(&pool, project_id, "Test Task", TaskStatus::InProgress).await;
    let workspace = create_test_workspace(&pool, task.id, "feature-branch").await;
    let session_id = create_test_session(&pool, workspace.id).await;
    let execution = create_test_execution_process(
        &pool,
        session_id,
        ExecutionProcessStatus::Completed,
        ExecutionProcessRunReason::CodingAgent,
    )
    .await;

    // Create mock trigger callback
    let trigger_capture = Arc::new(Mutex::new(TriggerCapture::default()));
    let callback = create_mock_trigger_callback(Arc::clone(&trigger_capture));
    let ctx = test_context_with_trigger(pool.clone(), callback);

    // Create handler
    let handler = FeedbackCollectionHandler::new(
        db::DBService { pool: pool.clone() },
        Arc::new(RwLock::new(services::services::config::Config::default())),
        Arc::new(RwLock::new(HashMap::new())),
        Arc::new(RwLock::new(std::collections::HashSet::new())),
    );

    // Verify handler accepts this event
    let event = DomainEvent::ExecutionCompleted {
        process: execution.clone(),
        task_id: task.id,
    };
    assert!(
        handler.handles(&event),
        "Handler should handle CodingAgent completed event"
    );

    // Dispatch event directly to handler
    handler
        .handle(event, &ctx)
        .await
        .expect("Handler should succeed");

    // Verify trigger was captured
    let capture = trigger_capture.lock().await;
    assert_eq!(
        capture.feedback_triggers.len(),
        1,
        "Should have exactly one feedback trigger"
    );

    let (ws_id, t_id, exec_id) = capture.feedback_triggers[0];
    assert_eq!(ws_id, workspace.id, "Workspace ID should match");
    assert_eq!(t_id, task.id, "Task ID should match");
    assert_eq!(exec_id, execution.id, "Execution process ID should match");

    // Verify no review attention triggers were captured
    assert!(
        capture.review_attention_triggers.is_empty(),
        "Should not trigger review attention"
    );
}

/// Test that FeedbackCollectionHandler does NOT trigger for non-CodingAgent runs.
#[tokio::test]
async fn test_feedback_handler_ignores_non_coding_agent_executions() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "Non-CodingAgent Project").await;
    let task = create_test_task(&pool, project_id, "Test Task", TaskStatus::InProgress).await;
    let workspace = create_test_workspace(&pool, task.id, "feature-branch").await;
    let session_id = create_test_session(&pool, workspace.id).await;

    let handler = FeedbackCollectionHandler::new(
        db::DBService { pool: pool.clone() },
        Arc::new(RwLock::new(services::services::config::Config::default())),
        Arc::new(RwLock::new(HashMap::new())),
        Arc::new(RwLock::new(std::collections::HashSet::new())),
    );

    // Test with different run reasons
    let run_reasons = [
        ExecutionProcessRunReason::SetupScript,
        ExecutionProcessRunReason::CleanupScript,
        ExecutionProcessRunReason::InternalAgent,
    ];
    for run_reason in run_reasons {
        let execution = create_test_execution_process(
            &pool,
            session_id,
            ExecutionProcessStatus::Completed,
            run_reason.clone(),
        )
        .await;

        let event = DomainEvent::ExecutionCompleted {
            process: execution.clone(),
            task_id: task.id,
        };

        assert!(
            !handler.handles(&event),
            "Handler should NOT handle {:?} completed event",
            run_reason
        );
    }
}

/// Test that FeedbackCollectionHandler does NOT trigger for failed executions.
#[tokio::test]
async fn test_feedback_handler_ignores_failed_executions() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "Failed Execution Project").await;
    let task = create_test_task(&pool, project_id, "Test Task", TaskStatus::InProgress).await;
    let workspace = create_test_workspace(&pool, task.id, "feature-branch").await;
    let session_id = create_test_session(&pool, workspace.id).await;

    let handler = FeedbackCollectionHandler::new(
        db::DBService { pool: pool.clone() },
        Arc::new(RwLock::new(services::services::config::Config::default())),
        Arc::new(RwLock::new(HashMap::new())),
        Arc::new(RwLock::new(std::collections::HashSet::new())),
    );

    // Test with non-completed statuses
    let statuses = [
        ExecutionProcessStatus::Failed,
        ExecutionProcessStatus::Killed,
        ExecutionProcessStatus::Running,
    ];
    for status in statuses {
        let execution = create_test_execution_process(
            &pool,
            session_id,
            status.clone(),
            ExecutionProcessRunReason::CodingAgent,
        )
        .await;

        let event = DomainEvent::ExecutionCompleted {
            process: execution.clone(),
            task_id: task.id,
        };

        assert!(
            !handler.handles(&event),
            "Handler should NOT handle CodingAgent with status {:?}",
            status
        );
    }
}

/// Test that FeedbackCollectionHandler skips if feedback already exists for workspace.
#[tokio::test]
async fn test_feedback_handler_skips_if_feedback_exists() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "Existing Feedback Project").await;
    let task = create_test_task(&pool, project_id, "Test Task", TaskStatus::InProgress).await;
    let workspace = create_test_workspace(&pool, task.id, "feature-branch").await;
    let session_id = create_test_session(&pool, workspace.id).await;
    let execution = create_test_execution_process(
        &pool,
        session_id,
        ExecutionProcessStatus::Completed,
        ExecutionProcessRunReason::CodingAgent,
    )
    .await;

    // Insert existing feedback for this workspace
    let feedback_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    sqlx::query(
        "INSERT INTO agent_feedback (id, execution_process_id, task_id, workspace_id, collected_at, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(feedback_id)
    .bind(execution.id)
    .bind(task.id)
    .bind(workspace.id)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .expect("Failed to create feedback");

    // Create mock trigger callback
    let trigger_capture = Arc::new(Mutex::new(TriggerCapture::default()));
    let callback = create_mock_trigger_callback(Arc::clone(&trigger_capture));
    let ctx = test_context_with_trigger(pool.clone(), callback);

    let handler = FeedbackCollectionHandler::new(
        db::DBService { pool: pool.clone() },
        Arc::new(RwLock::new(services::services::config::Config::default())),
        Arc::new(RwLock::new(HashMap::new())),
        Arc::new(RwLock::new(std::collections::HashSet::new())),
    );

    let event = DomainEvent::ExecutionCompleted {
        process: execution.clone(),
        task_id: task.id,
    };

    // Handler should succeed but NOT trigger callback (feedback already exists)
    handler
        .handle(event, &ctx)
        .await
        .expect("Handler should succeed");

    let capture = trigger_capture.lock().await;
    assert!(
        capture.feedback_triggers.is_empty(),
        "Should NOT trigger feedback collection when feedback already exists"
    );
}

// ============================================================================
// ReviewAttentionHandler Integration Tests
// ============================================================================

/// Test that ReviewAttentionHandler triggers ReviewAttention callback when:
/// - TaskStatusChanged event is dispatched
/// - New status is InReview
#[tokio::test]
async fn test_review_attention_handler_triggers_on_inreview_status() {
    let (pool, _db_file) = create_test_db().await;

    // Setup: Create project, task, workspace, session, execution
    let project_id = create_test_project(&pool, "Review Attention Project").await;
    let task = create_test_task(&pool, project_id, "Test Task", TaskStatus::InReview).await;
    let workspace = create_test_workspace(&pool, task.id, "feature-branch").await;
    let session_id = create_test_session(&pool, workspace.id).await;
    let execution = create_test_execution_process(
        &pool,
        session_id,
        ExecutionProcessStatus::Completed,
        ExecutionProcessRunReason::CodingAgent,
    )
    .await;

    // Create mock trigger callback
    let trigger_capture = Arc::new(Mutex::new(TriggerCapture::default()));
    let callback = create_mock_trigger_callback(Arc::clone(&trigger_capture));
    let ctx = test_context_with_trigger(pool.clone(), callback);

    let handler = ReviewAttentionHandler::new();

    let event = DomainEvent::TaskStatusChanged {
        task: task.clone(),
        previous_status: TaskStatus::InProgress,
    };

    // Verify handler accepts this event
    assert!(
        handler.handles(&event),
        "Handler should handle InReview status change"
    );

    // Dispatch event directly to handler
    handler
        .handle(event, &ctx)
        .await
        .expect("Handler should succeed");

    // Verify trigger was captured
    let capture = trigger_capture.lock().await;
    assert_eq!(
        capture.review_attention_triggers.len(),
        1,
        "Should have exactly one review attention trigger"
    );

    let (t_id, exec_id) = capture.review_attention_triggers[0];
    assert_eq!(t_id, task.id, "Task ID should match");
    assert_eq!(exec_id, execution.id, "Execution process ID should match");

    // Verify no feedback triggers were captured
    assert!(
        capture.feedback_triggers.is_empty(),
        "Should not trigger feedback collection"
    );
}

/// Test that ReviewAttentionHandler ignores non-InReview status changes.
#[tokio::test]
async fn test_review_attention_handler_ignores_other_statuses() {
    let handler = ReviewAttentionHandler::new();

    let statuses = [
        TaskStatus::Todo,
        TaskStatus::InProgress,
        TaskStatus::Done,
        TaskStatus::Cancelled,
    ];
    for status in statuses {
        let task = Task {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            title: "Test task".to_string(),
            description: None,
            status: status.clone(),
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

        let event = DomainEvent::TaskStatusChanged {
            task,
            previous_status: TaskStatus::Todo,
        };

        assert!(
            !handler.handles(&event),
            "Handler should NOT handle status {:?}",
            status
        );
    }
}

/// Test that ReviewAttentionHandler skips if no workspace exists for task.
#[tokio::test]
async fn test_review_attention_handler_skips_without_workspace() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "No Workspace Project").await;
    // Task exists but has no workspace
    let task = create_test_task(&pool, project_id, "Test Task", TaskStatus::InReview).await;

    let trigger_capture = Arc::new(Mutex::new(TriggerCapture::default()));
    let callback = create_mock_trigger_callback(Arc::clone(&trigger_capture));
    let ctx = test_context_with_trigger(pool.clone(), callback);

    let handler = ReviewAttentionHandler::new();

    let event = DomainEvent::TaskStatusChanged {
        task: task.clone(),
        previous_status: TaskStatus::InProgress,
    };

    // Handler should succeed but not trigger callback
    handler
        .handle(event, &ctx)
        .await
        .expect("Handler should succeed");

    let capture = trigger_capture.lock().await;
    assert!(
        capture.review_attention_triggers.is_empty(),
        "Should NOT trigger review attention when task has no workspace"
    );
}

/// Test that ReviewAttentionHandler skips if no CodingAgent execution exists.
#[tokio::test]
async fn test_review_attention_handler_skips_without_coding_agent_execution() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "No CodingAgent Project").await;
    let task = create_test_task(&pool, project_id, "Test Task", TaskStatus::InReview).await;
    let workspace = create_test_workspace(&pool, task.id, "feature-branch").await;
    let session_id = create_test_session(&pool, workspace.id).await;

    // Create a non-CodingAgent execution
    let _execution = create_test_execution_process(
        &pool,
        session_id,
        ExecutionProcessStatus::Completed,
        ExecutionProcessRunReason::SetupScript, // Not CodingAgent
    )
    .await;

    let trigger_capture = Arc::new(Mutex::new(TriggerCapture::default()));
    let callback = create_mock_trigger_callback(Arc::clone(&trigger_capture));
    let ctx = test_context_with_trigger(pool.clone(), callback);

    let handler = ReviewAttentionHandler::new();

    let event = DomainEvent::TaskStatusChanged {
        task: task.clone(),
        previous_status: TaskStatus::InProgress,
    };

    handler
        .handle(event, &ctx)
        .await
        .expect("Handler should succeed");

    let capture = trigger_capture.lock().await;
    assert!(
        capture.review_attention_triggers.is_empty(),
        "Should NOT trigger review attention when no CodingAgent execution exists"
    );
}

// ============================================================================
// Dispatcher Integration Tests
// ============================================================================

/// Test full dispatcher flow: FeedbackCollectionHandler receives ExecutionCompleted
/// and triggers callback correctly.
#[tokio::test]
async fn test_dispatcher_feedback_collection_flow() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "Dispatcher Feedback Project").await;
    let task = create_test_task(&pool, project_id, "Test Task", TaskStatus::InProgress).await;
    let workspace = create_test_workspace(&pool, task.id, "feature-branch").await;
    let session_id = create_test_session(&pool, workspace.id).await;
    let execution = create_test_execution_process(
        &pool,
        session_id,
        ExecutionProcessStatus::Completed,
        ExecutionProcessRunReason::CodingAgent,
    )
    .await;

    let trigger_capture = Arc::new(Mutex::new(TriggerCapture::default()));
    let callback = create_mock_trigger_callback(Arc::clone(&trigger_capture));

    let db_service = db::DBService { pool: pool.clone() };
    let config = Arc::new(RwLock::new(services::services::config::Config::default()));

    let dispatcher = DispatcherBuilder::new()
        .with_handler(FeedbackCollectionHandler::new(
            db_service.clone(),
            config.clone(),
            Arc::new(RwLock::new(HashMap::new())),
            Arc::new(RwLock::new(std::collections::HashSet::new())),
        ))
        .with_context(HandlerContext::new(
            db_service,
            config,
            Arc::new(MsgStore::default()),
            Some(callback),
        ))
        .build();

    // Dispatch ExecutionCompleted event
    dispatcher
        .dispatch(DomainEvent::ExecutionCompleted {
            process: execution.clone(),
            task_id: task.id,
        })
        .await;

    // Give spawned handler a moment to complete
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let capture = trigger_capture.lock().await;
    assert_eq!(
        capture.feedback_triggers.len(),
        1,
        "Dispatcher should route ExecutionCompleted to FeedbackCollectionHandler"
    );
}

/// Test full dispatcher flow: ReviewAttentionHandler receives TaskStatusChanged
/// and triggers callback correctly.
#[tokio::test]
async fn test_dispatcher_review_attention_flow() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "Dispatcher Review Project").await;
    let task = create_test_task(&pool, project_id, "Test Task", TaskStatus::InReview).await;
    let workspace = create_test_workspace(&pool, task.id, "feature-branch").await;
    let session_id = create_test_session(&pool, workspace.id).await;
    // Create execution in DB (handler will find it via workspace lookup)
    let _execution = create_test_execution_process(
        &pool,
        session_id,
        ExecutionProcessStatus::Completed,
        ExecutionProcessRunReason::CodingAgent,
    )
    .await;

    let trigger_capture = Arc::new(Mutex::new(TriggerCapture::default()));
    let callback = create_mock_trigger_callback(Arc::clone(&trigger_capture));

    let db_service = db::DBService { pool: pool.clone() };
    let config = Arc::new(RwLock::new(services::services::config::Config::default()));

    let dispatcher = DispatcherBuilder::new()
        .with_handler(ReviewAttentionHandler::new())
        .with_context(HandlerContext::new(
            db_service,
            config,
            Arc::new(MsgStore::default()),
            Some(callback),
        ))
        .build();

    // Dispatch TaskStatusChanged event (simulates finalize_task emitting event)
    dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task: task.clone(),
            previous_status: TaskStatus::InProgress,
        })
        .await;

    // Give spawned handler a moment to complete
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let capture = trigger_capture.lock().await;
    assert_eq!(
        capture.review_attention_triggers.len(),
        1,
        "Dispatcher should route TaskStatusChanged (InReview) to ReviewAttentionHandler"
    );
}

/// Test that both handlers work together without interference.
#[tokio::test]
async fn test_dispatcher_both_handlers_no_cross_triggering() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "Both Handlers Project").await;
    let task = create_test_task(&pool, project_id, "Test Task", TaskStatus::InProgress).await;
    let workspace = create_test_workspace(&pool, task.id, "feature-branch").await;
    let session_id = create_test_session(&pool, workspace.id).await;
    let execution = create_test_execution_process(
        &pool,
        session_id,
        ExecutionProcessStatus::Completed,
        ExecutionProcessRunReason::CodingAgent,
    )
    .await;

    let trigger_capture = Arc::new(Mutex::new(TriggerCapture::default()));
    let callback = create_mock_trigger_callback(Arc::clone(&trigger_capture));

    let db_service = db::DBService { pool: pool.clone() };
    let config = Arc::new(RwLock::new(services::services::config::Config::default()));

    let dispatcher = DispatcherBuilder::new()
        .with_handler(FeedbackCollectionHandler::new(
            db_service.clone(),
            config.clone(),
            Arc::new(RwLock::new(HashMap::new())),
            Arc::new(RwLock::new(std::collections::HashSet::new())),
        ))
        .with_handler(ReviewAttentionHandler::new())
        .with_context(HandlerContext::new(
            db_service,
            config,
            Arc::new(MsgStore::default()),
            Some(callback),
        ))
        .build();

    // Dispatch ExecutionCompleted - should only trigger FeedbackCollectionHandler
    dispatcher
        .dispatch(DomainEvent::ExecutionCompleted {
            process: execution.clone(),
            task_id: task.id,
        })
        .await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    {
        let capture = trigger_capture.lock().await;
        assert_eq!(
            capture.feedback_triggers.len(),
            1,
            "ExecutionCompleted should only trigger FeedbackCollection"
        );
        assert!(
            capture.review_attention_triggers.is_empty(),
            "ExecutionCompleted should NOT trigger ReviewAttention"
        );
    }

    // Clear capture for next test
    {
        let mut capture = trigger_capture.lock().await;
        capture.feedback_triggers.clear();
        capture.review_attention_triggers.clear();
    }

    // Create new task with InReview status for TaskStatusChanged event
    let task_inreview =
        create_test_task(&pool, project_id, "InReview Task", TaskStatus::InReview).await;
    let workspace2 = create_test_workspace(&pool, task_inreview.id, "review-branch").await;
    let session2_id = create_test_session(&pool, workspace2.id).await;
    let _execution2 = create_test_execution_process(
        &pool,
        session2_id,
        ExecutionProcessStatus::Completed,
        ExecutionProcessRunReason::CodingAgent,
    )
    .await;

    // Dispatch TaskStatusChanged - should only trigger ReviewAttentionHandler
    dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task: task_inreview.clone(),
            previous_status: TaskStatus::InProgress,
        })
        .await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let capture = trigger_capture.lock().await;
    assert!(
        capture.feedback_triggers.is_empty(),
        "TaskStatusChanged should NOT trigger FeedbackCollection"
    );
    assert_eq!(
        capture.review_attention_triggers.len(),
        1,
        "TaskStatusChanged (InReview) should trigger ReviewAttention"
    );
}

// ============================================================================
// No Duplicate Triggers Tests
// ============================================================================

/// Test that dispatching the same event only triggers handlers once (no duplicates).
#[tokio::test]
async fn test_no_duplicate_feedback_triggers() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "Duplicate Test Project").await;
    let task = create_test_task(&pool, project_id, "Test Task", TaskStatus::InProgress).await;
    let workspace = create_test_workspace(&pool, task.id, "feature-branch").await;
    let session_id = create_test_session(&pool, workspace.id).await;
    let execution = create_test_execution_process(
        &pool,
        session_id,
        ExecutionProcessStatus::Completed,
        ExecutionProcessRunReason::CodingAgent,
    )
    .await;

    let trigger_count = Arc::new(AtomicUsize::new(0));
    let count_clone = Arc::clone(&trigger_count);

    // Create a counting callback
    let counting_callback: ExecutionTriggerCallback =
        Arc::new(move |_trigger: ExecutionTrigger| {
            let count = Arc::clone(&count_clone);
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(Uuid::new_v4())
            }
            .boxed()
        });

    let db_service = db::DBService { pool: pool.clone() };
    let config = Arc::new(RwLock::new(services::services::config::Config::default()));

    let dispatcher = DispatcherBuilder::new()
        .with_handler(FeedbackCollectionHandler::new(
            db_service.clone(),
            config.clone(),
            Arc::new(RwLock::new(HashMap::new())),
            Arc::new(RwLock::new(std::collections::HashSet::new())),
        ))
        .with_context(HandlerContext::new(
            db_service,
            config,
            Arc::new(MsgStore::default()),
            Some(counting_callback),
        ))
        .build();

    // Dispatch the same event
    let event = DomainEvent::ExecutionCompleted {
        process: execution.clone(),
        task_id: task.id,
    };

    dispatcher.dispatch(event).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Should have triggered exactly once
    assert_eq!(
        trigger_count.load(Ordering::SeqCst),
        1,
        "Single dispatch should trigger callback exactly once"
    );
}

// ============================================================================
// finalize_task Path Simulation Tests
// ============================================================================

/// Simulates the finalize_task path: when a task moves to InReview via
/// Task::update_status followed by TaskStatusChanged event dispatch,
/// the ReviewAttentionHandler should trigger.
///
/// This was the original bug: finalize_task was missing the trigger.
/// After refactoring, finalize_task emits TaskStatusChanged which
/// ReviewAttentionHandler responds to.
#[tokio::test]
async fn test_finalize_task_path_triggers_review_attention() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "Finalize Task Project").await;
    // Start with InProgress task (as finalize_task would see it)
    let mut task = create_test_task(&pool, project_id, "Test Task", TaskStatus::InProgress).await;
    let workspace = create_test_workspace(&pool, task.id, "feature-branch").await;
    let session_id = create_test_session(&pool, workspace.id).await;
    let _execution = create_test_execution_process(
        &pool,
        session_id,
        ExecutionProcessStatus::Completed,
        ExecutionProcessRunReason::CodingAgent,
    )
    .await;

    let trigger_capture = Arc::new(Mutex::new(TriggerCapture::default()));
    let callback = create_mock_trigger_callback(Arc::clone(&trigger_capture));

    let db_service = db::DBService { pool: pool.clone() };
    let config = Arc::new(RwLock::new(services::services::config::Config::default()));

    let dispatcher = DispatcherBuilder::new()
        .with_handler(ReviewAttentionHandler::new())
        .with_context(HandlerContext::new(
            db_service,
            config,
            Arc::new(MsgStore::default()),
            Some(callback),
        ))
        .build();

    // Simulate finalize_task:
    // 1. Update task status in DB
    let previous_status = task.status.clone();
    Task::update_status(&pool, task.id, TaskStatus::InReview)
        .await
        .expect("Should update task status");

    // 2. Update task struct to reflect new status
    task.status = TaskStatus::InReview;

    // 3. Emit TaskStatusChanged event (as finalize_task does)
    dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task: task.clone(),
            previous_status,
        })
        .await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Verify ReviewAttentionHandler triggered
    let capture = trigger_capture.lock().await;
    assert_eq!(
        capture.review_attention_triggers.len(),
        1,
        "finalize_task path (TaskStatusChanged → InReview) should trigger ReviewAttention"
    );

    assert_eq!(
        capture.review_attention_triggers[0].0, task.id,
        "Task ID should match"
    );
}

// ============================================================================
// Edge Cases
// ============================================================================

/// Test handler behavior when no execution trigger callback is provided.
#[tokio::test]
async fn test_handlers_gracefully_handle_no_callback() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "No Callback Project").await;
    let task = create_test_task(&pool, project_id, "Test Task", TaskStatus::InReview).await;
    let workspace = create_test_workspace(&pool, task.id, "feature-branch").await;
    let session_id = create_test_session(&pool, workspace.id).await;
    let execution = create_test_execution_process(
        &pool,
        session_id,
        ExecutionProcessStatus::Completed,
        ExecutionProcessRunReason::CodingAgent,
    )
    .await;

    // Context WITHOUT trigger callback
    let db_service = db::DBService { pool: pool.clone() };
    let config = Arc::new(RwLock::new(services::services::config::Config::default()));
    let ctx = HandlerContext::new(
        db_service.clone(),
        config.clone(),
        Arc::new(MsgStore::default()),
        None, // No callback!
    );

    // FeedbackCollectionHandler should handle gracefully
    let feedback_handler = FeedbackCollectionHandler::new(
        db_service.clone(),
        config.clone(),
        Arc::new(RwLock::new(HashMap::new())),
        Arc::new(RwLock::new(std::collections::HashSet::new())),
    );

    let result = feedback_handler
        .handle(
            DomainEvent::ExecutionCompleted {
                process: execution.clone(),
                task_id: task.id,
            },
            &ctx,
        )
        .await;
    assert!(
        result.is_ok(),
        "FeedbackCollectionHandler should succeed without callback"
    );

    // ReviewAttentionHandler should handle gracefully
    let review_handler = ReviewAttentionHandler::new();
    let result = review_handler
        .handle(
            DomainEvent::TaskStatusChanged {
                task: task.clone(),
                previous_status: TaskStatus::InProgress,
            },
            &ctx,
        )
        .await;
    assert!(
        result.is_ok(),
        "ReviewAttentionHandler should succeed without callback"
    );
}

// ============================================================================
// Hook-to-Execution Linking Integration Tests
// ============================================================================

/// Tests that when FeedbackCollectionHandler triggers an execution, it links
/// the spawned execution process back to the hook execution that triggered it.
#[tokio::test]
async fn test_feedback_handler_links_hook_to_execution() {
    use services::services::domain_events::HookExecutionStore;

    let (pool, _db_file) = create_test_db().await;

    // Create test data
    let project_id = create_test_project(&pool, "link_test_project").await;
    let task = create_test_task(&pool, project_id, "Link test task", TaskStatus::InReview).await;
    let workspace = create_test_workspace(&pool, task.id, "test-branch").await;
    let session_id = create_test_session(&pool, workspace.id).await;
    let execution = create_test_execution_process(
        &pool,
        session_id,
        ExecutionProcessStatus::Completed,
        ExecutionProcessRunReason::CodingAgent,
    )
    .await;

    // Create a callback that captures the spawned execution ID
    let spawned_exec_id = Uuid::new_v4();
    let callback: ExecutionTriggerCallback = {
        let id = spawned_exec_id;
        Arc::new(move |_trigger| async move { Ok(id) }.boxed())
    };

    // Create handler context with hook_execution_id and store set
    let db_service = db::DBService { pool: pool.clone() };
    let config = Arc::new(RwLock::new(services::services::config::Config::default()));
    let msg_store = Arc::new(MsgStore::default());
    let hook_store = HookExecutionStore::new(msg_store.clone());

    // Simulate what the dispatcher does: set hook_execution_id on the context
    let hook_exec_id = hook_store
        .start_execution(task.id, "feedback_collection", services::services::domain_events::HookPoint::PostAgentComplete)
        .expect("feedback_collection should be a tracked handler");

    let mut ctx = HandlerContext::new(db_service.clone(), config.clone(), msg_store.clone(), Some(callback));
    ctx.hook_execution_store = Some(hook_store.clone());
    ctx.hook_execution_id = Some(hook_exec_id);

    // Call the handler
    let feedback_handler = FeedbackCollectionHandler::new(
        db_service.clone(),
        config.clone(),
        Arc::new(RwLock::new(HashMap::new())),
        Arc::new(RwLock::new(std::collections::HashSet::new())),
    );

    let result = feedback_handler
        .handle(
            DomainEvent::ExecutionCompleted {
                process: execution.clone(),
                task_id: task.id,
            },
            &ctx,
        )
        .await;

    assert!(result.is_ok(), "Handler should succeed");

    // Verify the hook execution is linked to the spawned execution
    let execs = hook_store.get_for_task(task.id);
    assert_eq!(execs.len(), 1);
    assert_eq!(execs[0].linked_execution_process_id, Some(spawned_exec_id));
}

/// Tests that when ReviewAttentionHandler triggers an execution, it links
/// the spawned execution process back to the hook execution that triggered it.
#[tokio::test]
async fn test_review_handler_links_hook_to_execution() {
    use services::services::domain_events::HookExecutionStore;

    let (pool, _db_file) = create_test_db().await;

    // Create test data
    let project_id = create_test_project(&pool, "review_link_test_project").await;
    let task = create_test_task(&pool, project_id, "Review link test task", TaskStatus::InReview).await;
    let workspace = create_test_workspace(&pool, task.id, "test-branch").await;
    let session_id = create_test_session(&pool, workspace.id).await;
    let _execution = create_test_execution_process(
        &pool,
        session_id,
        ExecutionProcessStatus::Completed,
        ExecutionProcessRunReason::CodingAgent,
    )
    .await;

    // Create a callback that returns a spawned execution ID
    let spawned_exec_id = Uuid::new_v4();
    let callback: ExecutionTriggerCallback = {
        let id = spawned_exec_id;
        Arc::new(move |_trigger| async move { Ok(id) }.boxed())
    };

    // Create handler context with hook_execution_id and store set
    let db_service = db::DBService { pool: pool.clone() };
    let config = Arc::new(RwLock::new(services::services::config::Config::default()));
    let msg_store = Arc::new(MsgStore::default());
    let hook_store = HookExecutionStore::new(msg_store.clone());

    // Simulate what the dispatcher does: set hook_execution_id on the context
    let hook_exec_id = hook_store
        .start_execution(task.id, "review_attention", services::services::domain_events::HookPoint::PostTaskStatusChange)
        .expect("review_attention should be a tracked handler");

    let mut ctx = HandlerContext::new(db_service.clone(), config.clone(), msg_store.clone(), Some(callback));
    ctx.hook_execution_store = Some(hook_store.clone());
    ctx.hook_execution_id = Some(hook_exec_id);

    // Call the handler
    let review_handler = ReviewAttentionHandler::new();
    let result = review_handler
        .handle(
            DomainEvent::TaskStatusChanged {
                task: task.clone(),
                previous_status: TaskStatus::InProgress,
            },
            &ctx,
        )
        .await;

    assert!(result.is_ok(), "Handler should succeed");

    // Verify the hook execution is linked to the spawned execution
    let execs = hook_store.get_for_task(task.id);
    assert_eq!(execs.len(), 1);
    assert_eq!(execs[0].linked_execution_process_id, Some(spawned_exec_id));
}

/// Tests that dispatcher correctly sets hook_execution_id on context for spawned handlers.
#[tokio::test]
async fn test_dispatcher_sets_hook_execution_id_for_spawned_handlers() {
    use services::services::domain_events::{ExecutionMode, HandlerError, HookExecutionStore};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    // Custom handler that checks if hook_execution_id is set
    struct HookIdCheckHandler {
        hook_id_was_set: Arc<AtomicBool>,
        hook_id_matched: Arc<AtomicBool>,
    }

    #[async_trait]
    impl EventHandler for HookIdCheckHandler {
        fn name(&self) -> &'static str {
            "feedback_collection" // Must be a tracked handler
        }

        fn execution_mode(&self) -> ExecutionMode {
            ExecutionMode::Spawned
        }

        fn handles(&self, _event: &DomainEvent) -> bool {
            true
        }

        async fn handle(
            &self,
            _event: DomainEvent,
            ctx: &HandlerContext,
        ) -> Result<(), HandlerError> {
            // Check if hook_execution_id was set
            if ctx.hook_execution_id.is_some() {
                self.hook_id_was_set.store(true, Ordering::SeqCst);
            }
            // Check if the hook_execution_id matches what's in the store
            if let (Some(id), Some(store)) = (ctx.hook_execution_id, &ctx.hook_execution_store) {
                let all_execs = store.get_all();
                if all_execs.iter().any(|e| e.id == id) {
                    self.hook_id_matched.store(true, Ordering::SeqCst);
                }
            }
            Ok(())
        }
    }

    let msg_store = Arc::new(MsgStore::default());
    let hook_store = HookExecutionStore::new(msg_store.clone());

    let hook_id_was_set = Arc::new(AtomicBool::new(false));
    let hook_id_matched = Arc::new(AtomicBool::new(false));

    let handler = HookIdCheckHandler {
        hook_id_was_set: Arc::clone(&hook_id_was_set),
        hook_id_matched: Arc::clone(&hook_id_matched),
    };

    // Create a minimal context
    let pool = SqlitePoolOptions::new()
        .connect_lazy("sqlite::memory:")
        .unwrap();
    let db = db::DBService { pool };
    let config = Arc::new(RwLock::new(services::services::config::Config::default()));
    let ctx = HandlerContext::new(db, config, msg_store.clone(), None);

    let dispatcher = DispatcherBuilder::new()
        .with_handler(handler)
        .with_context(ctx)
        .with_hook_execution_store(hook_store)
        .build();

    // Create a task event with a task_id so tracking is enabled
    let task = Task {
        id: Uuid::new_v4(),
        project_id: Uuid::new_v4(),
        title: "Test task".to_string(),
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

    let event = DomainEvent::TaskStatusChanged {
        task,
        previous_status: TaskStatus::InProgress,
    };

    dispatcher.dispatch(event).await;

    // Wait for spawned handler to complete
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(
        hook_id_was_set.load(Ordering::SeqCst),
        "hook_execution_id should be set on context for spawned handlers"
    );
    assert!(
        hook_id_matched.load(Ordering::SeqCst),
        "hook_execution_id should match an execution in the store"
    );
}
