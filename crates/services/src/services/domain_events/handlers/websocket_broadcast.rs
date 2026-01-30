use async_trait::async_trait;
use db::models::{project::Project, task::Task, task_dependency::TaskDependency};
use tracing::warn;
use uuid::Uuid;

use super::super::{DomainEvent, EventHandler, ExecutionMode, HandlerContext, HandlerError};
use crate::services::events::{project_with_counts_patch, task_patch};

/// Handler that broadcasts task and project updates via WebSocket patches.
///
/// When a task status changes, this handler:
/// 1. Pushes the updated task patch to connected clients
/// 2. Pushes updated project counts (for the projects page)
/// 3. Pushes updates for dependent tasks (their is_blocked status may have changed)
pub struct WebSocketBroadcastHandler;

impl WebSocketBroadcastHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebSocketBroadcastHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventHandler for WebSocketBroadcastHandler {
    fn name(&self) -> &'static str {
        "websocket_broadcast"
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Spawned
    }

    fn handles(&self, event: &DomainEvent) -> bool {
        matches!(
            event,
            DomainEvent::TaskStatusChanged { .. } | DomainEvent::ExecutionCompleted { .. }
        )
    }

    async fn handle(&self, event: DomainEvent, ctx: &HandlerContext) -> Result<(), HandlerError> {
        match event {
            DomainEvent::TaskStatusChanged { task, .. } => {
                self.handle_task_status_changed(&task, ctx).await
            }
            DomainEvent::ExecutionCompleted { task_id, .. } => {
                // When execution completes, refresh the task to get latest status.
                if let Some(task_with_status) =
                    Task::find_by_id_with_attempt_status(&ctx.db.pool, task_id).await?
                {
                    let patch = task_patch::replace(&task_with_status);
                    ctx.msg_store.push_patch(patch);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

impl WebSocketBroadcastHandler {
    async fn handle_task_status_changed(
        &self,
        task: &Task,
        ctx: &HandlerContext,
    ) -> Result<(), HandlerError> {
        // Fetch task with attempt status for full data
        let task_with_status = Task::find_by_id_with_attempt_status(&ctx.db.pool, task.id).await?;

        if let Some(task_with_status) = task_with_status {
            // Push task patch
            let patch = task_patch::replace(&task_with_status);
            ctx.msg_store.push_patch(patch);

            // Push project counts update for real-time task count updates on projects page
            if let Some(project_with_counts) =
                Project::find_by_id_with_task_counts(&ctx.db.pool, task.project_id).await?
            {
                ctx.msg_store
                    .push_patch(project_with_counts_patch::replace(&project_with_counts));
            }

            // Push updates for tasks that depend on this one.
            // Their is_blocked status may have changed.
            match TaskDependency::find_blocking(&ctx.db.pool, task.id).await {
                Ok(dependent_tasks) => {
                    for dep_task in dependent_tasks {
                        if let Err(e) = self
                            .push_task_update(&ctx.db.pool, &ctx.msg_store, dep_task.id)
                            .await
                        {
                            warn!(
                                task_id = %dep_task.id,
                                error = %e,
                                "Failed to push dependent task update"
                            );
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        task_id = %task.id,
                        error = %e,
                        "Failed to find blocking tasks"
                    );
                }
            }
        }

        Ok(())
    }

    async fn push_task_update(
        &self,
        pool: &sqlx::SqlitePool,
        msg_store: &utils::msg_store::MsgStore,
        task_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        if let Some(task_with_status) = Task::find_by_id_with_attempt_status(pool, task_id).await? {
            msg_store.push_patch(task_patch::replace(&task_with_status));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handler_name() {
        let handler = WebSocketBroadcastHandler::new();
        assert_eq!(handler.name(), "websocket_broadcast");
    }

    #[test]
    fn test_execution_mode() {
        let handler = WebSocketBroadcastHandler::new();
        assert_eq!(handler.execution_mode(), ExecutionMode::Spawned);
    }

    #[test]
    fn test_handles_task_status_changed() {
        use db::models::task::{Task, TaskStatus};

        let handler = WebSocketBroadcastHandler::new();
        let task = Task {
            id: uuid::Uuid::new_v4(),
            project_id: uuid::Uuid::new_v4(),
            title: "Test".to_string(),
            description: None,
            status: TaskStatus::InProgress,
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
            task: task.clone(),
            previous_status: TaskStatus::Todo,
        };

        assert!(handler.handles(&event));
    }

    #[test]
    fn test_handles_execution_completed() {
        use db::models::execution_process::{
            ExecutionProcess, ExecutionProcessRunReason, ExecutionProcessStatus,
            ExecutorActionField,
        };

        let handler = WebSocketBroadcastHandler::new();
        let process = ExecutionProcess {
            id: uuid::Uuid::new_v4(),
            session_id: Some(uuid::Uuid::new_v4()),
            conversation_session_id: None,
            run_reason: ExecutionProcessRunReason::CodingAgent,
            executor_action: sqlx::types::Json(ExecutorActionField::Other(
                serde_json::json!({"type": "test"}),
            )),
            status: ExecutionProcessStatus::Completed,
            exit_code: Some(0),
            dropped: false,
            input_tokens: None,
            output_tokens: None,
            started_at: chrono::Utc::now(),
            completed_at: Some(chrono::Utc::now()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let event = DomainEvent::ExecutionCompleted {
            process,
            task_id: uuid::Uuid::new_v4(),
        };

        assert!(handler.handles(&event));
    }

    #[test]
    fn test_does_not_handle_other_events() {
        use db::models::workspace::Workspace;

        let handler = WebSocketBroadcastHandler::new();
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

        let event = DomainEvent::WorkspaceCreated { workspace };

        assert!(!handler.handles(&event));
    }
}
