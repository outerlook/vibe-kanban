use std::sync::Arc;

use async_trait::async_trait;
use db::DBService;
use thiserror::Error;
use tokio::sync::RwLock;
use utils::msg_store::MsgStore;

use crate::services::config::Config;

use super::DomainEvent;

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
}

impl HandlerContext {
    pub fn new(db: DBService, config: Arc<RwLock<Config>>, msg_store: Arc<MsgStore>) -> Self {
        Self {
            db,
            config,
            msg_store,
        }
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
