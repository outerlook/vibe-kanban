//! Review attention handler for triggering review collection when tasks move to InReview.
//!
//! When a task's status changes to InReview, this handler triggers review attention collection
//! by calling the execution_trigger callback with the ReviewAttention variant.

use async_trait::async_trait;
use db::models::{
    execution_process::{ExecutionProcess, ExecutionProcessRunReason},
    task::TaskStatus,
    workspace::Workspace,
};
use tracing::{debug, info, warn};

use crate::services::domain_events::{
    DomainEvent, EventHandler, ExecutionMode, ExecutionTrigger, HandlerContext, HandlerError,
};

/// Handler that triggers review attention collection when a task moves to InReview.
pub struct ReviewAttentionHandler;

impl ReviewAttentionHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ReviewAttentionHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventHandler for ReviewAttentionHandler {
    fn name(&self) -> &'static str {
        "review_attention"
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Spawned
    }

    fn handles(&self, event: &DomainEvent) -> bool {
        matches!(
            event,
            DomainEvent::TaskStatusChanged { task, .. } if task.status == TaskStatus::InReview
        )
    }

    async fn handle(&self, event: DomainEvent, ctx: &HandlerContext) -> Result<(), HandlerError> {
        let DomainEvent::TaskStatusChanged { task, .. } = event else {
            return Ok(());
        };

        let task_id = task.id;

        debug!(
            task_id = %task_id,
            "Task moved to InReview, triggering review attention collection"
        );

        // Find the latest workspace for this task
        let workspace = match Workspace::find_latest_by_task_id(&ctx.db.pool, task_id).await {
            Ok(Some(ws)) => ws,
            Ok(None) => {
                debug!(
                    task_id = %task_id,
                    "Skipping review attention: task has no workspace"
                );
                return Ok(());
            }
            Err(e) => {
                warn!(
                    task_id = %task_id,
                    error = %e,
                    "Failed to find workspace for task"
                );
                return Err(HandlerError::Database(e));
            }
        };

        // Find the latest CodingAgent execution for this workspace
        let execution_process = match ExecutionProcess::find_latest_by_workspace_and_run_reason(
            &ctx.db.pool,
            workspace.id,
            &ExecutionProcessRunReason::CodingAgent,
        )
        .await
        {
            Ok(Some(ep)) => ep,
            Ok(None) => {
                debug!(
                    task_id = %task_id,
                    workspace_id = %workspace.id,
                    "Skipping review attention: no CodingAgent execution found"
                );
                return Ok(());
            }
            Err(e) => {
                warn!(
                    task_id = %task_id,
                    workspace_id = %workspace.id,
                    error = %e,
                    "Failed to find CodingAgent execution"
                );
                return Err(HandlerError::Database(e));
            }
        };

        // Trigger review attention collection via callback
        let Some(ref trigger_callback) = ctx.execution_trigger else {
            debug!(
                task_id = %task_id,
                "No execution trigger callback available, skipping review attention"
            );
            return Ok(());
        };

        let trigger = ExecutionTrigger::ReviewAttention {
            task_id,
            execution_process_id: execution_process.id,
        };

        info!(
            task_id = %task_id,
            execution_process_id = %execution_process.id,
            "Triggering review attention collection"
        );

        trigger_callback(trigger).await.map_err(|e| {
            HandlerError::Failed(format!("Failed to trigger review attention: {e}"))
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use db::models::task::Task;
    use uuid::Uuid;

    use super::*;

    fn make_task(status: TaskStatus) -> Task {
        Task {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            title: "Test task".to_string(),
            description: None,
            status,
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

    fn make_task_status_changed_event(status: TaskStatus) -> DomainEvent {
        DomainEvent::TaskStatusChanged {
            task: make_task(status),
            previous_status: TaskStatus::InProgress,
        }
    }

    #[test]
    fn test_handles_inreview_status() {
        let handler = ReviewAttentionHandler::new();

        // Should handle TaskStatusChanged to InReview
        let event = make_task_status_changed_event(TaskStatus::InReview);
        assert!(handler.handles(&event));
    }

    #[test]
    fn test_ignores_other_statuses() {
        let handler = ReviewAttentionHandler::new();

        // Should ignore TaskStatusChanged to other statuses
        assert!(
            !handler.handles(&make_task_status_changed_event(TaskStatus::Todo)),
            "Handler should not handle status Todo"
        );
        assert!(
            !handler.handles(&make_task_status_changed_event(TaskStatus::InProgress)),
            "Handler should not handle status InProgress"
        );
        assert!(
            !handler.handles(&make_task_status_changed_event(TaskStatus::Done)),
            "Handler should not handle status Done"
        );
        assert!(
            !handler.handles(&make_task_status_changed_event(TaskStatus::Cancelled)),
            "Handler should not handle status Cancelled"
        );
    }

    #[test]
    fn test_ignores_other_event_types() {
        let handler = ReviewAttentionHandler::new();

        // Should ignore other event types
        let event = DomainEvent::WorkspaceDeleted {
            workspace_id: Uuid::new_v4(),
            task_id: Uuid::new_v4(),
        };
        assert!(!handler.handles(&event));
    }

    #[test]
    fn test_execution_mode_is_spawned() {
        let handler = ReviewAttentionHandler::new();
        assert_eq!(handler.execution_mode(), ExecutionMode::Spawned);
    }

    #[test]
    fn test_handler_name() {
        let handler = ReviewAttentionHandler::new();
        assert_eq!(handler.name(), "review_attention");
    }
}
