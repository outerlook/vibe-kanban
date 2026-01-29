use async_trait::async_trait;

use crate::services::{
    domain_events::{DomainEvent, EventHandler, ExecutionMode, HandlerContext, HandlerError},
    share::SharePublisher,
};

/// Handler that syncs task status changes to the remote server.
///
/// When a task with a `shared_task_id` has its status changed,
/// this handler propagates that change to the remote share service.
pub struct RemoteSyncHandler {
    publisher: Option<SharePublisher>,
}

impl RemoteSyncHandler {
    pub fn new(publisher: Option<SharePublisher>) -> Self {
        Self { publisher }
    }
}

#[async_trait]
impl EventHandler for RemoteSyncHandler {
    fn name(&self) -> &'static str {
        "remote_sync"
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Spawned
    }

    fn handles(&self, event: &DomainEvent) -> bool {
        matches!(event, DomainEvent::TaskStatusChanged { .. })
    }

    async fn handle(&self, event: DomainEvent, _ctx: &HandlerContext) -> Result<(), HandlerError> {
        let Some(publisher) = &self.publisher else {
            return Ok(());
        };

        let DomainEvent::TaskStatusChanged { task, .. } = event else {
            return Ok(());
        };

        if let Err(e) = publisher.update_shared_task(&task).await {
            tracing::warn!(
                task_id = %task.id,
                error = %e,
                "Failed to sync task status to remote"
            );
        }

        Ok(())
    }
}
