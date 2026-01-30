use std::sync::Arc;

use async_trait::async_trait;
use db::DBService;
use thiserror::Error;
use tokio::sync::RwLock;
use utils::msg_store::MsgStore;

use super::{DomainEvent, ExecutionTriggerCallback, HookExecutionStore};
use crate::services::config::Config;

/// Determines how an event handler should be executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Handler runs inline (blocking) - the dispatcher waits for completion.
    Inline,
    /// Handler runs via `tokio::spawn` (fire-and-forget) - the dispatcher does not wait.
    Spawned,
}

/// Error type for event handler failures.
#[derive(Debug, Error)]
pub enum HandlerError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Handler failed: {0}")]
    Failed(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Context provided to event handlers, containing shared services.
#[derive(Clone)]
pub struct HandlerContext {
    pub db: DBService,
    pub config: Arc<RwLock<Config>>,
    pub msg_store: Arc<MsgStore>,
    /// Optional callback for triggering executions from handlers.
    /// This is None in test contexts where execution triggering is not needed.
    pub execution_trigger: Option<ExecutionTriggerCallback>,
    /// Store for tracking hook execution status. Used by the dispatcher
    /// to track spawned handler executions.
    pub hook_execution_store: Option<HookExecutionStore>,
}

impl HandlerContext {
    pub fn new(
        db: DBService,
        config: Arc<RwLock<Config>>,
        msg_store: Arc<MsgStore>,
        execution_trigger: Option<ExecutionTriggerCallback>,
    ) -> Self {
        Self {
            db,
            config,
            msg_store,
            execution_trigger,
            hook_execution_store: None,
        }
    }

    /// Sets the hook execution store for tracking handler executions.
    pub fn with_hook_execution_store(mut self, store: HookExecutionStore) -> Self {
        self.hook_execution_store = Some(store);
        self
    }
}

/// Trait for domain event handlers.
///
/// Implement this trait to create handlers that react to domain events.
/// Handlers can specify their execution mode (inline or spawned) and
/// filter which events they handle via the `handles` method.
#[async_trait]
pub trait EventHandler: Send + Sync {
    /// Returns the name of this handler (for logging and debugging).
    fn name(&self) -> &'static str;

    /// Returns the execution mode for this handler.
    fn execution_mode(&self) -> ExecutionMode;

    /// Returns true if this handler should process the given event.
    fn handles(&self, event: &DomainEvent) -> bool;

    /// Handles the event. Called only if `handles` returned true.
    async fn handle(&self, event: DomainEvent, ctx: &HandlerContext) -> Result<(), HandlerError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test struct to verify the EventHandler trait can be implemented.
    struct TestHandler;

    #[async_trait]
    impl EventHandler for TestHandler {
        fn name(&self) -> &'static str {
            "test_handler"
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
            Ok(())
        }
    }

    #[test]
    fn test_handler_compiles() {
        let _handler: Box<dyn EventHandler> = Box::new(TestHandler);
    }

    #[test]
    fn test_execution_mode_variants() {
        assert_eq!(ExecutionMode::Inline, ExecutionMode::Inline);
        assert_eq!(ExecutionMode::Spawned, ExecutionMode::Spawned);
        assert_ne!(ExecutionMode::Inline, ExecutionMode::Spawned);
    }
}
