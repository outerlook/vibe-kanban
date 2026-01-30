//! Integration tests for the Domain Event System.
//!
//! Tests verify:
//! - Dispatcher calls handlers when event matches
//! - Inline handlers complete before dispatch returns
//! - Handlers receive correct event data
//! - Handler errors are logged but don't fail dispatch

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use db::models::task::{Task, TaskStatus};
use db::models::workspace::Workspace;
use services::services::domain_events::{
    DispatcherBuilder, DomainEvent, EventHandler, ExecutionMode, HandlerContext, HandlerError,
};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use tempfile::NamedTempFile;
use tokio::sync::{Mutex, RwLock};
use utils::msg_store::MsgStore;

// ============================================================================
// Test Setup Helpers
// ============================================================================

/// Creates a unique file-based SQLite database and runs all migrations.
/// Returns the pool and the temp file (which must be kept alive for the test duration).
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

/// Creates a test HandlerContext with a real database pool.
fn test_context(pool: SqlitePool) -> HandlerContext {
    let db = db::DBService { pool };
    let config = Arc::new(RwLock::new(services::services::config::Config::default()));
    let msg_store = Arc::new(MsgStore::default());
    HandlerContext::new(db, config, msg_store, None)
}

/// Creates a test Task for use in events.
fn test_task() -> Task {
    Task {
        id: uuid::Uuid::new_v4(),
        project_id: uuid::Uuid::new_v4(),
        title: "Test task".to_string(),
        description: Some("Task description for testing".to_string()),
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
    }
}

/// Creates a test Workspace for use in events.
fn test_workspace() -> Workspace {
    Workspace {
        id: uuid::Uuid::new_v4(),
        task_id: uuid::Uuid::new_v4(),
        container_ref: Some("test-container".to_string()),
        branch: "feature/test-branch".to_string(),
        agent_working_dir: Some("/workspace/project".to_string()),
        setup_completed_at: Some(chrono::Utc::now()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

// ============================================================================
// Test Handler Implementation
// ============================================================================

/// A test handler that captures events for assertions.
/// Uses Arc<Mutex<Vec<DomainEvent>>> to collect received events.
struct TestHandler {
    name: &'static str,
    execution_mode: ExecutionMode,
    received_events: Arc<Mutex<Vec<DomainEvent>>>,
    accepts_all: bool,
    should_fail: bool,
    call_count: Arc<AtomicUsize>,
}

impl TestHandler {
    fn new(name: &'static str, execution_mode: ExecutionMode) -> Self {
        Self {
            name,
            execution_mode,
            received_events: Arc::new(Mutex::new(Vec::new())),
            accepts_all: true,
            should_fail: false,
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn events(&self) -> Arc<Mutex<Vec<DomainEvent>>> {
        Arc::clone(&self.received_events)
    }

    fn call_count(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.call_count)
    }

    fn accepts_none(mut self) -> Self {
        self.accepts_all = false;
        self
    }

    fn fails(mut self) -> Self {
        self.should_fail = true;
        self
    }
}

#[async_trait]
impl EventHandler for TestHandler {
    fn name(&self) -> &'static str {
        self.name
    }

    fn execution_mode(&self) -> ExecutionMode {
        self.execution_mode
    }

    fn handles(&self, _event: &DomainEvent) -> bool {
        self.accepts_all
    }

    async fn handle(&self, event: DomainEvent, _ctx: &HandlerContext) -> Result<(), HandlerError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        self.received_events.lock().await.push(event);

        if self.should_fail {
            return Err(HandlerError::Failed("intentional test failure".to_string()));
        }

        Ok(())
    }
}

/// A test handler that only accepts specific event types.
struct SelectiveHandler {
    name: &'static str,
    accepts_task_events: bool,
    accepts_workspace_events: bool,
    received_events: Arc<Mutex<Vec<DomainEvent>>>,
}

impl SelectiveHandler {
    fn task_only(name: &'static str) -> Self {
        Self {
            name,
            accepts_task_events: true,
            accepts_workspace_events: false,
            received_events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn workspace_only(name: &'static str) -> Self {
        Self {
            name,
            accepts_task_events: false,
            accepts_workspace_events: true,
            received_events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn events(&self) -> Arc<Mutex<Vec<DomainEvent>>> {
        Arc::clone(&self.received_events)
    }
}

#[async_trait]
impl EventHandler for SelectiveHandler {
    fn name(&self) -> &'static str {
        self.name
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Inline
    }

    fn handles(&self, event: &DomainEvent) -> bool {
        match event {
            DomainEvent::TaskStatusChanged { .. } => self.accepts_task_events,
            DomainEvent::WorkspaceCreated { .. } | DomainEvent::WorkspaceDeleted { .. } => {
                self.accepts_workspace_events
            }
            _ => false,
        }
    }

    async fn handle(&self, event: DomainEvent, _ctx: &HandlerContext) -> Result<(), HandlerError> {
        self.received_events.lock().await.push(event);
        Ok(())
    }
}

// ============================================================================
// Integration Tests: Dispatcher Calls Handlers When Event Matches
// ============================================================================

#[tokio::test]
async fn test_dispatcher_calls_handler_when_event_matches() {
    let (pool, _db_file) = create_test_db().await;
    let ctx = test_context(pool);

    let handler = TestHandler::new("test_handler", ExecutionMode::Inline);
    let events = handler.events();
    let call_count = handler.call_count();

    let dispatcher = DispatcherBuilder::new()
        .with_handler(handler)
        .with_context(ctx)
        .build();

    let task = test_task();
    let event = DomainEvent::TaskStatusChanged {
        task: task.clone(),
        previous_status: TaskStatus::InProgress,
    };

    dispatcher.dispatch(event).await;

    assert_eq!(call_count.load(Ordering::SeqCst), 1);
    let received = events.lock().await;
    assert_eq!(received.len(), 1);
}

#[tokio::test]
async fn test_dispatcher_skips_handler_when_event_not_matched() {
    let (pool, _db_file) = create_test_db().await;
    let ctx = test_context(pool);

    let handler = TestHandler::new("non_matching", ExecutionMode::Inline).accepts_none();
    let call_count = handler.call_count();

    let dispatcher = DispatcherBuilder::new()
        .with_handler(handler)
        .with_context(ctx)
        .build();

    let event = DomainEvent::TaskStatusChanged {
        task: test_task(),
        previous_status: TaskStatus::Todo,
    };

    dispatcher.dispatch(event).await;

    assert_eq!(call_count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn test_multiple_handlers_all_receive_matching_events() {
    let (pool, _db_file) = create_test_db().await;
    let ctx = test_context(pool);

    let handler1 = TestHandler::new("handler_a", ExecutionMode::Inline);
    let handler2 = TestHandler::new("handler_b", ExecutionMode::Inline);
    let handler3 = TestHandler::new("handler_c", ExecutionMode::Inline);

    let count1 = handler1.call_count();
    let count2 = handler2.call_count();
    let count3 = handler3.call_count();

    let dispatcher = DispatcherBuilder::new()
        .with_handler(handler1)
        .with_handler(handler2)
        .with_handler(handler3)
        .with_context(ctx)
        .build();

    dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task: test_task(),
            previous_status: TaskStatus::Todo,
        })
        .await;

    assert_eq!(count1.load(Ordering::SeqCst), 1);
    assert_eq!(count2.load(Ordering::SeqCst), 1);
    assert_eq!(count3.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_selective_handlers_only_receive_matching_events() {
    let (pool, _db_file) = create_test_db().await;
    let ctx = test_context(pool);

    let task_handler = SelectiveHandler::task_only("task_handler");
    let workspace_handler = SelectiveHandler::workspace_only("workspace_handler");

    let task_events = task_handler.events();
    let workspace_events = workspace_handler.events();

    let dispatcher = DispatcherBuilder::new()
        .with_handler(task_handler)
        .with_handler(workspace_handler)
        .with_context(ctx)
        .build();

    // Dispatch a task event
    dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task: test_task(),
            previous_status: TaskStatus::Todo,
        })
        .await;

    // Dispatch a workspace event
    dispatcher
        .dispatch(DomainEvent::WorkspaceCreated {
            workspace: test_workspace(),
        })
        .await;

    // Task handler should have received 1 event
    assert_eq!(task_events.lock().await.len(), 1);
    // Workspace handler should have received 1 event
    assert_eq!(workspace_events.lock().await.len(), 1);
}

// ============================================================================
// Integration Tests: Inline Handlers Complete Before Dispatch Returns
// ============================================================================

#[tokio::test]
async fn test_inline_handlers_complete_before_dispatch_returns() {
    let (pool, _db_file) = create_test_db().await;
    let ctx = test_context(pool);

    let completed = Arc::new(AtomicBool::new(false));

    struct BlockingHandler {
        completed: Arc<AtomicBool>,
    }

    #[async_trait]
    impl EventHandler for BlockingHandler {
        fn name(&self) -> &'static str {
            "blocking"
        }

        fn execution_mode(&self) -> ExecutionMode {
            ExecutionMode::Inline
        }

        fn handles(&self, _event: &DomainEvent) -> bool {
            true
        }

        async fn handle(
            &self,
            _event: DomainEvent,
            _ctx: &HandlerContext,
        ) -> Result<(), HandlerError> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            self.completed.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    let dispatcher = DispatcherBuilder::new()
        .with_handler(BlockingHandler {
            completed: Arc::clone(&completed),
        })
        .with_context(ctx)
        .build();

    // Before dispatch
    assert!(!completed.load(Ordering::SeqCst));

    // dispatch() should block until inline handler completes
    dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task: test_task(),
            previous_status: TaskStatus::Todo,
        })
        .await;

    // After dispatch returns, the handler MUST have completed
    assert!(completed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn test_multiple_inline_handlers_run_sequentially() {
    let (pool, _db_file) = create_test_db().await;
    let ctx = test_context(pool);

    let execution_order = Arc::new(Mutex::new(Vec::new()));

    struct OrderTrackingHandler {
        name: &'static str,
        order: Arc<Mutex<Vec<&'static str>>>,
    }

    #[async_trait]
    impl EventHandler for OrderTrackingHandler {
        fn name(&self) -> &'static str {
            self.name
        }

        fn execution_mode(&self) -> ExecutionMode {
            ExecutionMode::Inline
        }

        fn handles(&self, _event: &DomainEvent) -> bool {
            true
        }

        async fn handle(
            &self,
            _event: DomainEvent,
            _ctx: &HandlerContext,
        ) -> Result<(), HandlerError> {
            self.order.lock().await.push(self.name);
            Ok(())
        }
    }

    // Add handlers in reverse alphabetical order
    let dispatcher = DispatcherBuilder::new()
        .with_handler(OrderTrackingHandler {
            name: "zebra",
            order: Arc::clone(&execution_order),
        })
        .with_handler(OrderTrackingHandler {
            name: "apple",
            order: Arc::clone(&execution_order),
        })
        .with_handler(OrderTrackingHandler {
            name: "mango",
            order: Arc::clone(&execution_order),
        })
        .with_context(ctx)
        .build();

    dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task: test_task(),
            previous_status: TaskStatus::Todo,
        })
        .await;

    // Handlers are sorted alphabetically by name
    let order = execution_order.lock().await;
    assert_eq!(*order, vec!["apple", "mango", "zebra"]);
}

// ============================================================================
// Integration Tests: Handlers Receive Correct Event Data
// ============================================================================

#[tokio::test]
async fn test_handlers_receive_correct_task_event_data() {
    let (pool, _db_file) = create_test_db().await;
    let ctx = test_context(pool);

    let handler = TestHandler::new("data_checker", ExecutionMode::Inline);
    let events = handler.events();

    let dispatcher = DispatcherBuilder::new()
        .with_handler(handler)
        .with_context(ctx)
        .build();

    let task = test_task();
    let task_id = task.id;
    let task_title = task.title.clone();
    let previous_status = TaskStatus::InProgress;

    dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task,
            previous_status,
        })
        .await;

    let received = events.lock().await;
    assert_eq!(received.len(), 1);

    match &received[0] {
        DomainEvent::TaskStatusChanged {
            task,
            previous_status: prev,
        } => {
            assert_eq!(task.id, task_id);
            assert_eq!(task.title, task_title);
            assert_eq!(*prev, TaskStatus::InProgress);
        }
        _ => panic!("Expected TaskStatusChanged event"),
    }
}

#[tokio::test]
async fn test_handlers_receive_correct_workspace_event_data() {
    let (pool, _db_file) = create_test_db().await;
    let ctx = test_context(pool);

    let handler = TestHandler::new("workspace_checker", ExecutionMode::Inline);
    let events = handler.events();

    let dispatcher = DispatcherBuilder::new()
        .with_handler(handler)
        .with_context(ctx)
        .build();

    let workspace = test_workspace();
    let workspace_id = workspace.id;
    let workspace_branch = workspace.branch.clone();

    dispatcher
        .dispatch(DomainEvent::WorkspaceCreated { workspace })
        .await;

    let received = events.lock().await;
    assert_eq!(received.len(), 1);

    match &received[0] {
        DomainEvent::WorkspaceCreated { workspace } => {
            assert_eq!(workspace.id, workspace_id);
            assert_eq!(workspace.branch, workspace_branch);
        }
        _ => panic!("Expected WorkspaceCreated event"),
    }
}

#[tokio::test]
async fn test_handlers_receive_correct_workspace_deleted_data() {
    let (pool, _db_file) = create_test_db().await;
    let ctx = test_context(pool);

    let handler = TestHandler::new("deletion_checker", ExecutionMode::Inline);
    let events = handler.events();

    let dispatcher = DispatcherBuilder::new()
        .with_handler(handler)
        .with_context(ctx)
        .build();

    let workspace_id = uuid::Uuid::new_v4();
    let task_id = uuid::Uuid::new_v4();

    dispatcher
        .dispatch(DomainEvent::WorkspaceDeleted {
            workspace_id,
            task_id,
        })
        .await;

    let received = events.lock().await;
    assert_eq!(received.len(), 1);

    match &received[0] {
        DomainEvent::WorkspaceDeleted {
            workspace_id: ws_id,
            task_id: t_id,
        } => {
            assert_eq!(*ws_id, workspace_id);
            assert_eq!(*t_id, task_id);
        }
        _ => panic!("Expected WorkspaceDeleted event"),
    }
}

// ============================================================================
// Integration Tests: Handler Errors Are Logged But Don't Fail Dispatch
// ============================================================================

#[tokio::test]
async fn test_handler_error_does_not_fail_dispatch() {
    let (pool, _db_file) = create_test_db().await;
    let ctx = test_context(pool);

    let failing_handler = TestHandler::new("failing", ExecutionMode::Inline).fails();
    let failing_count = failing_handler.call_count();

    let dispatcher = DispatcherBuilder::new()
        .with_handler(failing_handler)
        .with_context(ctx)
        .build();

    // Should not panic
    dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task: test_task(),
            previous_status: TaskStatus::Todo,
        })
        .await;

    // Handler was called even though it failed
    assert_eq!(failing_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_handler_error_does_not_stop_subsequent_handlers() {
    let (pool, _db_file) = create_test_db().await;
    let ctx = test_context(pool);

    // Handler "after_fail" comes after "failing" alphabetically
    let failing_handler = TestHandler::new("failing", ExecutionMode::Inline).fails();
    let successful_handler = TestHandler::new("zhandler", ExecutionMode::Inline);

    let failing_count = failing_handler.call_count();
    let successful_count = successful_handler.call_count();
    let successful_events = successful_handler.events();

    let dispatcher = DispatcherBuilder::new()
        .with_handler(failing_handler)
        .with_handler(successful_handler)
        .with_context(ctx)
        .build();

    dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task: test_task(),
            previous_status: TaskStatus::Todo,
        })
        .await;

    // Both handlers were called
    assert_eq!(failing_count.load(Ordering::SeqCst), 1);
    assert_eq!(successful_count.load(Ordering::SeqCst), 1);

    // Successful handler received the event
    assert_eq!(successful_events.lock().await.len(), 1);
}

#[tokio::test]
async fn test_multiple_failing_handlers_all_run() {
    let (pool, _db_file) = create_test_db().await;
    let ctx = test_context(pool);

    let handler1 = TestHandler::new("fail_a", ExecutionMode::Inline).fails();
    let handler2 = TestHandler::new("fail_b", ExecutionMode::Inline).fails();
    let handler3 = TestHandler::new("success_c", ExecutionMode::Inline);

    let count1 = handler1.call_count();
    let count2 = handler2.call_count();
    let count3 = handler3.call_count();

    let dispatcher = DispatcherBuilder::new()
        .with_handler(handler1)
        .with_handler(handler2)
        .with_handler(handler3)
        .with_context(ctx)
        .build();

    dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task: test_task(),
            previous_status: TaskStatus::Todo,
        })
        .await;

    // All handlers ran despite failures
    assert_eq!(count1.load(Ordering::SeqCst), 1);
    assert_eq!(count2.load(Ordering::SeqCst), 1);
    assert_eq!(count3.load(Ordering::SeqCst), 1);
}

// ============================================================================
// Integration Tests: Spawned Handlers Behavior
// ============================================================================

#[tokio::test]
async fn test_spawned_handlers_do_not_block_dispatch() {
    let (pool, _db_file) = create_test_db().await;
    let ctx = test_context(pool);

    let started = Arc::new(AtomicBool::new(false));
    let completed = Arc::new(AtomicBool::new(false));

    struct SpawnedHandler {
        started: Arc<AtomicBool>,
        completed: Arc<AtomicBool>,
    }

    #[async_trait]
    impl EventHandler for SpawnedHandler {
        fn name(&self) -> &'static str {
            "spawned"
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
            _ctx: &HandlerContext,
        ) -> Result<(), HandlerError> {
            self.started.store(true, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(100)).await;
            self.completed.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    let dispatcher = DispatcherBuilder::new()
        .with_handler(SpawnedHandler {
            started: Arc::clone(&started),
            completed: Arc::clone(&completed),
        })
        .with_context(ctx)
        .build();

    // dispatch() should return quickly (before spawned handler completes)
    dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task: test_task(),
            previous_status: TaskStatus::Todo,
        })
        .await;

    // Give spawned task a moment to start
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Handler should have started but not completed
    assert!(started.load(Ordering::SeqCst));
    assert!(!completed.load(Ordering::SeqCst));

    // Wait for completion
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(completed.load(Ordering::SeqCst));
}

// ============================================================================
// Integration Tests: Edge Cases
// ============================================================================

#[tokio::test]
async fn test_dispatch_with_no_handlers() {
    let (pool, _db_file) = create_test_db().await;
    let ctx = test_context(pool);

    let dispatcher = DispatcherBuilder::new().with_context(ctx).build();

    // Should not panic with no handlers
    dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task: test_task(),
            previous_status: TaskStatus::Todo,
        })
        .await;
}

#[tokio::test]
async fn test_dispatch_multiple_events_sequentially() {
    let (pool, _db_file) = create_test_db().await;
    let ctx = test_context(pool);

    let handler = TestHandler::new("counter", ExecutionMode::Inline);
    let events = handler.events();
    let count = handler.call_count();

    let dispatcher = DispatcherBuilder::new()
        .with_handler(handler)
        .with_context(ctx)
        .build();

    for i in 0..5 {
        let mut task = test_task();
        task.title = format!("Task {}", i);
        dispatcher
            .dispatch(DomainEvent::TaskStatusChanged {
                task,
                previous_status: TaskStatus::Todo,
            })
            .await;
    }

    assert_eq!(count.load(Ordering::SeqCst), 5);
    let received = events.lock().await;
    assert_eq!(received.len(), 5);

    // Verify events are in order
    for (i, event) in received.iter().enumerate() {
        match event {
            DomainEvent::TaskStatusChanged { task, .. } => {
                assert_eq!(task.title, format!("Task {}", i));
            }
            _ => panic!("Expected TaskStatusChanged event"),
        }
    }
}

#[tokio::test]
async fn test_handler_receives_event_clone() {
    let (pool, _db_file) = create_test_db().await;
    let ctx = test_context(pool);

    // Two handlers both receiving the same event
    let handler1 = TestHandler::new("handler_1", ExecutionMode::Inline);
    let handler2 = TestHandler::new("handler_2", ExecutionMode::Inline);

    let events1 = handler1.events();
    let events2 = handler2.events();

    let dispatcher = DispatcherBuilder::new()
        .with_handler(handler1)
        .with_handler(handler2)
        .with_context(ctx)
        .build();

    let task = test_task();
    let task_id = task.id;

    dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task,
            previous_status: TaskStatus::Todo,
        })
        .await;

    // Both handlers should receive the same event data
    let received1 = events1.lock().await;
    let received2 = events2.lock().await;

    match (&received1[0], &received2[0]) {
        (
            DomainEvent::TaskStatusChanged { task: t1, .. },
            DomainEvent::TaskStatusChanged { task: t2, .. },
        ) => {
            assert_eq!(t1.id, task_id);
            assert_eq!(t2.id, task_id);
        }
        _ => panic!("Expected TaskStatusChanged events"),
    }
}
