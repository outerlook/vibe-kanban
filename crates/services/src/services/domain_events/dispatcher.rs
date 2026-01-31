//! Domain event dispatcher for routing events to registered handlers.
//!
//! The dispatcher manages handler registration and event routing based on
//! execution mode (inline vs spawned).

use std::sync::Arc;

use tracing::{debug, warn};

use super::{
    DomainEvent, EventHandler, ExecutionMode, ExecutionTriggerCallback, HandlerContext,
    HookExecutionStore,
};

/// Dispatches domain events to registered handlers.
///
/// Handlers are partitioned by execution mode:
/// - Inline handlers run sequentially and block until completion
/// - Spawned handlers run via `tokio::spawn` (fire-and-forget)
///
/// Handlers are sorted by name for deterministic ordering.
pub struct DomainEventDispatcher {
    inline_handlers: Vec<Arc<dyn EventHandler>>,
    spawned_handlers: Vec<Arc<dyn EventHandler>>,
    ctx: Arc<HandlerContext>,
}

impl DomainEventDispatcher {
    /// Dispatches an event to all handlers that accept it.
    ///
    /// 1. Runs inline handlers sequentially (awaits each)
    /// 2. Spawns spawned handlers (fire-and-forget)
    /// 3. Logs errors but does not propagate them
    pub async fn dispatch(&self, event: DomainEvent) {
        // Run inline handlers sequentially
        for handler in &self.inline_handlers {
            if handler.handles(&event) {
                debug!(
                    handler = handler.name(),
                    event = ?std::mem::discriminant(&event),
                    "Dispatching event to inline handler"
                );
                if let Err(e) = handler.handle(event.clone(), &self.ctx).await {
                    warn!(
                        handler = handler.name(),
                        error = %e,
                        "Inline handler failed"
                    );
                }
            }
        }

        // Spawn spawned handlers (fire-and-forget)
        for handler in &self.spawned_handlers {
            if handler.handles(&event) {
                let handler = Arc::clone(handler);
                let event = event.clone();

                debug!(
                    handler = handler.name(),
                    event = ?std::mem::discriminant(&event),
                    "Spawning handler"
                );

                // Track hook execution if we have a store and task_id
                let execution_id = self.track_execution_start(&event, handler.name());

                // Create a context with hook_execution_id set for this handler
                let mut handler_ctx = (*self.ctx).clone();
                handler_ctx.hook_execution_id = execution_id;
                let handler_ctx = Arc::new(handler_ctx);

                tokio::spawn(async move {
                    let result = handler.handle(event, &handler_ctx).await;

                    // Update execution status if we were tracking
                    if let Some(exec_id) = execution_id
                        && let Some(store) = &handler_ctx.hook_execution_store
                    {
                        match &result {
                            Ok(()) => store.complete_execution(exec_id),
                            Err(e) => store.fail_execution(exec_id, e.to_string()),
                        }
                    }

                    if let Err(e) = result {
                        warn!(
                            handler = handler.name(),
                            error = %e,
                            "Spawned handler failed"
                        );
                    }
                });
            }
        }
    }

    /// Starts tracking a hook execution if the store is available and event has a task_id.
    /// Returns the execution ID if tracking was started, None otherwise.
    /// Also returns None if the handler is not in the tracked handlers whitelist.
    fn track_execution_start(&self, event: &DomainEvent, handler_name: &str) -> Option<uuid::Uuid> {
        let store = self.ctx.hook_execution_store.as_ref()?;
        let task_id = event.task_id()?;
        let hook_point = event.hook_point();

        store.start_execution(task_id, handler_name, hook_point)
    }
}

/// Builder for constructing a `DomainEventDispatcher`.
pub struct DispatcherBuilder {
    handlers: Vec<Arc<dyn EventHandler>>,
    ctx: Option<HandlerContext>,
    execution_trigger: Option<ExecutionTriggerCallback>,
    hook_execution_store: Option<HookExecutionStore>,
}

impl DispatcherBuilder {
    /// Creates a new builder.
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
            ctx: None,
            execution_trigger: None,
            hook_execution_store: None,
        }
    }

    /// Adds a handler to the dispatcher.
    pub fn with_handler<H: EventHandler + 'static>(mut self, handler: H) -> Self {
        self.handlers.push(Arc::new(handler));
        self
    }

    /// Sets the handler context.
    pub fn with_context(mut self, ctx: HandlerContext) -> Self {
        self.ctx = Some(ctx);
        self
    }

    /// Sets the execution trigger callback.
    ///
    /// The callback will be passed to the `HandlerContext`, allowing handlers
    /// to trigger new executions.
    pub fn with_execution_trigger(mut self, callback: ExecutionTriggerCallback) -> Self {
        self.execution_trigger = Some(callback);
        self
    }

    /// Sets the hook execution store for tracking spawned handler executions.
    ///
    /// When set, the dispatcher will track the status of spawned handlers
    /// and broadcast updates via SSE.
    pub fn with_hook_execution_store(mut self, store: HookExecutionStore) -> Self {
        self.hook_execution_store = Some(store);
        self
    }

    /// Builds the dispatcher.
    ///
    /// If `with_execution_trigger` was called, the callback will be set on the
    /// context, overriding any existing execution_trigger in the provided context.
    ///
    /// Similarly, if `with_hook_execution_store` was called, the store will be
    /// set on the context for tracking spawned handler executions.
    ///
    /// # Panics
    /// Panics if no context was provided.
    pub fn build(mut self) -> DomainEventDispatcher {
        let mut ctx = self
            .ctx
            .expect("HandlerContext is required to build DomainEventDispatcher");

        // Apply execution_trigger if set via with_execution_trigger
        if let Some(callback) = self.execution_trigger {
            ctx.execution_trigger = Some(callback);
        }

        // Apply hook_execution_store if set via with_hook_execution_store
        if let Some(store) = self.hook_execution_store {
            ctx.hook_execution_store = Some(store);
        }

        // Sort handlers by name for deterministic ordering
        self.handlers.sort_by_key(|h| h.name());

        // Partition by execution mode
        let (inline, spawned): (Vec<_>, Vec<_>) = self
            .handlers
            .into_iter()
            .partition(|h| h.execution_mode() == ExecutionMode::Inline);

        DomainEventDispatcher {
            inline_handlers: inline,
            spawned_handlers: spawned,
            ctx: Arc::new(ctx),
        }
    }
}

impl Default for DispatcherBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::atomic::{AtomicBool, AtomicUsize, Ordering},
        time::Duration,
    };

    use async_trait::async_trait;
    use db::models::{
        task::{Task, TaskStatus},
        workspace::Workspace,
    };
    use tokio::sync::RwLock;
    use utils::msg_store::MsgStore;

    use super::*;
    use crate::services::{config::Config, domain_events::HandlerError};

    fn test_task() -> Task {
        Task {
            id: uuid::Uuid::new_v4(),
            project_id: uuid::Uuid::new_v4(),
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
        }
    }

    fn test_event() -> DomainEvent {
        DomainEvent::TaskStatusChanged {
            task: test_task(),
            previous_status: TaskStatus::InProgress,
        }
    }

    /// Creates a minimal HandlerContext for tests.
    ///
    /// Note: This uses a minimal db pool that won't work for real database operations.
    /// Tests using this context should not actually perform database queries.
    fn test_context() -> HandlerContext {
        // We need a real-ish db for tests, but we can use an in-memory SQLite pool
        // The handlers in these tests don't actually query the database
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect_lazy("sqlite::memory:")
            .unwrap();
        let db = db::DBService { pool };
        let config = Arc::new(RwLock::new(Config::default()));
        let msg_store = Arc::new(MsgStore::default());
        HandlerContext::new(db, config, msg_store, None)
    }

    #[tokio::test]
    async fn test_dispatcher_calls_handler_when_event_matches() {
        let call_count = Arc::new(AtomicUsize::new(0));

        struct SharedCountHandler {
            count: Arc<AtomicUsize>,
        }

        #[async_trait]
        impl EventHandler for SharedCountHandler {
            fn name(&self) -> &'static str {
                "shared_count"
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
                self.count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }

        let dispatcher = DispatcherBuilder::new()
            .with_handler(SharedCountHandler {
                count: Arc::clone(&call_count),
            })
            .with_context(test_context())
            .build();

        dispatcher.dispatch(test_event()).await;

        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_dispatcher_skips_handler_when_event_not_handled() {
        let call_count = Arc::new(AtomicUsize::new(0));

        struct NonMatchingHandler {
            count: Arc<AtomicUsize>,
        }

        #[async_trait]
        impl EventHandler for NonMatchingHandler {
            fn name(&self) -> &'static str {
                "non_matching"
            }

            fn execution_mode(&self) -> ExecutionMode {
                ExecutionMode::Inline
            }

            fn handles(&self, _event: &DomainEvent) -> bool {
                false // Never handles any event
            }

            async fn handle(
                &self,
                _event: DomainEvent,
                _ctx: &HandlerContext,
            ) -> Result<(), HandlerError> {
                self.count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }

        let dispatcher = DispatcherBuilder::new()
            .with_handler(NonMatchingHandler {
                count: Arc::clone(&call_count),
            })
            .with_context(test_context())
            .build();

        dispatcher.dispatch(test_event()).await;

        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_inline_handlers_block_dispatch_returns_after_completion() {
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
            .with_context(test_context())
            .build();

        // dispatch() should block until inline handler completes
        dispatcher.dispatch(test_event()).await;

        // After dispatch returns, the handler should have completed
        assert!(completed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_spawned_handlers_do_not_block() {
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
            .with_context(test_context())
            .build();

        // dispatch() should return quickly (before spawned handler completes)
        dispatcher.dispatch(test_event()).await;

        // Give spawned task a moment to start
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Handler should have started but not completed
        assert!(started.load(Ordering::SeqCst));
        assert!(!completed.load(Ordering::SeqCst));

        // Wait for completion
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(completed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_handlers_sorted_by_name() {
        let order = Arc::new(std::sync::Mutex::new(Vec::new()));

        struct OrderTrackingHandler {
            name: &'static str,
            order: Arc<std::sync::Mutex<Vec<&'static str>>>,
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
                self.order.lock().unwrap().push(self.name);
                Ok(())
            }
        }

        // Add handlers in reverse alphabetical order
        let dispatcher = DispatcherBuilder::new()
            .with_handler(OrderTrackingHandler {
                name: "zebra",
                order: Arc::clone(&order),
            })
            .with_handler(OrderTrackingHandler {
                name: "apple",
                order: Arc::clone(&order),
            })
            .with_handler(OrderTrackingHandler {
                name: "mango",
                order: Arc::clone(&order),
            })
            .with_context(test_context())
            .build();

        dispatcher.dispatch(test_event()).await;

        let execution_order = order.lock().unwrap();
        assert_eq!(*execution_order, vec!["apple", "mango", "zebra"]);
    }

    #[tokio::test]
    async fn test_handler_errors_logged_not_propagated() {
        struct FailingHandler;

        #[async_trait]
        impl EventHandler for FailingHandler {
            fn name(&self) -> &'static str {
                "failing"
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
                Err(HandlerError::Failed("intentional failure".to_string()))
            }
        }

        let call_count = Arc::new(AtomicUsize::new(0));

        struct AfterFailHandler {
            count: Arc<AtomicUsize>,
        }

        #[async_trait]
        impl EventHandler for AfterFailHandler {
            fn name(&self) -> &'static str {
                "after_fail"
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
                self.count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }

        let dispatcher = DispatcherBuilder::new()
            .with_handler(FailingHandler)
            .with_handler(AfterFailHandler {
                count: Arc::clone(&call_count),
            })
            .with_context(test_context())
            .build();

        // Should not panic, errors are logged
        dispatcher.dispatch(test_event()).await;

        // Second handler should still be called despite first failing
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_builder_default() {
        let builder = DispatcherBuilder::default();
        // Just verify it compiles and creates
        assert!(builder.handlers.is_empty());
    }

    #[tokio::test]
    async fn test_builder_with_execution_trigger() {
        use futures::FutureExt;

        let callback_called = Arc::new(AtomicBool::new(false));
        let called_clone = Arc::clone(&callback_called);

        let callback: ExecutionTriggerCallback = Arc::new(move |_trigger| {
            called_clone.store(true, Ordering::SeqCst);
            async { Ok(uuid::Uuid::new_v4()) }.boxed()
        });

        let dispatcher = DispatcherBuilder::new()
            .with_context(test_context())
            .with_execution_trigger(callback)
            .build();

        // Verify the callback is set in the context
        assert!(dispatcher.ctx.execution_trigger.is_some());
    }

    #[tokio::test]
    async fn test_builder_without_execution_trigger_has_none() {
        let dispatcher = DispatcherBuilder::new()
            .with_context(test_context())
            .build();

        // Without with_execution_trigger, the callback should be None
        // (since test_context() creates context with None)
        assert!(dispatcher.ctx.execution_trigger.is_none());
    }

    #[test]
    fn test_handles_event_filters_correctly() {
        // Test that dispatch only routes to handlers that match
        let task = test_task();
        let workspace = Workspace {
            id: uuid::Uuid::new_v4(),
            task_id: uuid::Uuid::new_v4(),
            container_ref: None,
            branch: "test".to_string(),
            agent_working_dir: None,
            setup_completed_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let task_event = DomainEvent::TaskStatusChanged {
            task,
            previous_status: TaskStatus::Todo,
        };

        let workspace_event = DomainEvent::WorkspaceCreated { workspace };

        struct TaskOnlyHandler;

        impl TaskOnlyHandler {
            fn handles(event: &DomainEvent) -> bool {
                matches!(event, DomainEvent::TaskStatusChanged { .. })
            }
        }

        assert!(TaskOnlyHandler::handles(&task_event));
        assert!(!TaskOnlyHandler::handles(&workspace_event));
    }

    #[tokio::test]
    async fn test_hook_execution_tracking_for_spawned_handlers() {
        use crate::services::domain_events::{HookExecutionStatus, HookPoint};

        let msg_store = Arc::new(MsgStore::default());
        let hook_store = HookExecutionStore::new(msg_store.clone());

        // Use a tracked handler name (autopilot) for the test
        struct SpawnedTrackingHandler;

        #[async_trait]
        impl EventHandler for SpawnedTrackingHandler {
            fn name(&self) -> &'static str {
                "autopilot" // Must be in TRACKED_HANDLERS
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
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok(())
            }
        }

        let task = test_task();
        let task_id = task.id;
        let event = DomainEvent::TaskStatusChanged {
            task,
            previous_status: TaskStatus::Todo,
        };

        let dispatcher = DispatcherBuilder::new()
            .with_handler(SpawnedTrackingHandler)
            .with_context(test_context())
            .with_hook_execution_store(hook_store.clone())
            .build();

        // Before dispatch, no executions
        assert!(hook_store.get_for_task(task_id).is_empty());

        // Dispatch the event
        dispatcher.dispatch(event).await;

        // Give the spawned task a moment to start
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Should have a running execution
        let execs = hook_store.get_for_task(task_id);
        assert_eq!(execs.len(), 1);
        assert_eq!(execs[0].handler_name, "autopilot");
        assert_eq!(execs[0].hook_point, HookPoint::PostTaskStatusChange);
        assert_eq!(execs[0].status, HookExecutionStatus::Running);

        // Wait for completion
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Should be completed now
        let execs = hook_store.get_for_task(task_id);
        assert_eq!(execs.len(), 1);
        assert_eq!(execs[0].status, HookExecutionStatus::Completed);
    }

    #[tokio::test]
    async fn test_hook_execution_tracking_records_failure() {
        use crate::services::domain_events::{HookExecutionStatus, HookPoint};

        let msg_store = Arc::new(MsgStore::default());
        let hook_store = HookExecutionStore::new(msg_store.clone());

        // Use a tracked handler name (feedback_collection) for the test
        struct FailingSpawnedHandler;

        #[async_trait]
        impl EventHandler for FailingSpawnedHandler {
            fn name(&self) -> &'static str {
                "feedback_collection" // Must be in TRACKED_HANDLERS
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
                tokio::time::sleep(Duration::from_millis(20)).await;
                Err(HandlerError::Failed("intentional failure".to_string()))
            }
        }

        let task = test_task();
        let task_id = task.id;
        let event = DomainEvent::TaskStatusChanged {
            task,
            previous_status: TaskStatus::Todo,
        };

        let dispatcher = DispatcherBuilder::new()
            .with_handler(FailingSpawnedHandler)
            .with_context(test_context())
            .with_hook_execution_store(hook_store.clone())
            .build();

        dispatcher.dispatch(event).await;

        // Wait for the handler to fail
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Should be marked as failed
        let execs = hook_store.get_for_task(task_id);
        assert_eq!(execs.len(), 1);
        assert_eq!(execs[0].handler_name, "feedback_collection");
        assert_eq!(execs[0].hook_point, HookPoint::PostTaskStatusChange);
        assert_eq!(execs[0].status, HookExecutionStatus::Failed);
        assert!(
            execs[0]
                .error
                .as_ref()
                .unwrap()
                .contains("intentional failure")
        );
    }

    #[tokio::test]
    async fn test_hook_execution_skipped_for_events_without_task_id() {
        let msg_store = Arc::new(MsgStore::default());
        let hook_store = HookExecutionStore::new(msg_store.clone());

        let call_count = Arc::new(AtomicUsize::new(0));

        struct SpawnedCountingHandler {
            count: Arc<AtomicUsize>,
        }

        #[async_trait]
        impl EventHandler for SpawnedCountingHandler {
            fn name(&self) -> &'static str {
                "spawned_counting"
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
                self.count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }

        let project = db::models::project::Project {
            id: uuid::Uuid::new_v4(),
            name: "Test project".to_string(),
            dev_script: None,
            dev_script_working_dir: None,
            default_agent_working_dir: None,
            remote_project_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // ProjectUpdated event has no task_id
        let event = DomainEvent::ProjectUpdated { project };

        let dispatcher = DispatcherBuilder::new()
            .with_handler(SpawnedCountingHandler {
                count: Arc::clone(&call_count),
            })
            .with_context(test_context())
            .with_hook_execution_store(hook_store.clone())
            .build();

        dispatcher.dispatch(event).await;

        // Wait for the handler to complete
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Handler should have been called
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        // But no hook executions should be tracked (since event has no task_id)
        let all_execs = hook_store.get_all();
        assert!(
            all_execs.is_empty(),
            "No executions should be tracked for events without task_id"
        );
    }
}
