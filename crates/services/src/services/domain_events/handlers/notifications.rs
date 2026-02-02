use async_trait::async_trait;
use db::models::execution_process::{ExecutionProcess, ExecutionProcessStatus};

use crate::services::{
    domain_events::{DomainEvent, EventHandler, ExecutionMode, HandlerContext, HandlerError},
    notification::NotificationService,
};

/// Handler for sending OS and in-app notifications when executions complete.
pub struct NotificationHandler {
    notification_service: NotificationService,
}

impl NotificationHandler {
    pub fn new(notification_service: NotificationService) -> Self {
        Self {
            notification_service,
        }
    }
}

#[async_trait]
impl EventHandler for NotificationHandler {
    fn name(&self) -> &'static str {
        "notifications"
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Spawned
    }

    fn handles(&self, event: &DomainEvent) -> bool {
        matches!(event, DomainEvent::ExecutionCompleted { .. })
    }

    async fn handle(&self, event: DomainEvent, ctx: &HandlerContext) -> Result<(), HandlerError> {
        let DomainEvent::ExecutionCompleted { process, .. } = event else {
            return Ok(());
        };

        // Skip notification if process was intentionally killed by user
        if matches!(process.status, ExecutionProcessStatus::Killed) {
            return Ok(());
        }

        // Only notify for workspace-based executions (those with session_id)
        if process.session_id.is_none() {
            return Ok(());
        }

        // Load execution context to get task title, project, workspace
        let execution_ctx = match ExecutionProcess::load_context(&ctx.db.pool, process.id).await {
            Ok(ctx) => ctx,
            Err(e) => {
                tracing::warn!(
                    "Failed to load execution context for notification (process {}): {}",
                    process.id,
                    e
                );
                return Ok(());
            }
        };

        let title = format!("Task Complete: {}", execution_ctx.task.title);

        // Check if frontend handles sounds (skip backend sound playback if so)
        let frontend_sounds_enabled = ctx.config.read().await.notifications.frontend_sounds_enabled;

        match process.status {
            ExecutionProcessStatus::Completed => {
                let message = format!(
                    "✅ '{}' completed successfully\nBranch: {:?}\nExecutor: {:?}",
                    execution_ctx.task.title,
                    execution_ctx.workspace.branch,
                    execution_ctx.session.executor
                );

                // OS notification (skip sound if frontend handles it)
                if frontend_sounds_enabled {
                    self.notification_service
                        .notify_push_only(&title, &message)
                        .await;
                } else {
                    self.notification_service.notify(&title, &message).await;
                }

                // In-app notification
                if let Err(e) = NotificationService::notify_agent_complete(
                    &ctx.db.pool,
                    execution_ctx.project.id,
                    execution_ctx.workspace.id,
                    &execution_ctx.task.title,
                )
                .await
                {
                    tracing::warn!("Failed to create in-app completion notification: {}", e);
                }
            }
            ExecutionProcessStatus::Failed => {
                let message = format!(
                    "❌ '{}' execution failed\nBranch: {:?}\nExecutor: {:?}",
                    execution_ctx.task.title,
                    execution_ctx.workspace.branch,
                    execution_ctx.session.executor
                );

                // OS notification with error sound (skip sound if frontend handles it)
                if frontend_sounds_enabled {
                    self.notification_service
                        .notify_push_only(&title, &message)
                        .await;
                } else {
                    self.notification_service
                        .notify_error(&title, &message)
                        .await;
                }

                // In-app notification
                if let Err(e) = NotificationService::notify_agent_error(
                    &ctx.db.pool,
                    execution_ctx.project.id,
                    execution_ctx.workspace.id,
                    &execution_ctx.task.title,
                )
                .await
                {
                    tracing::warn!("Failed to create in-app error notification: {}", e);
                }
            }
            ExecutionProcessStatus::Running | ExecutionProcessStatus::Killed => {
                // Running shouldn't reach here (event is for completion)
                // Killed is already handled above
            }
        }

        Ok(())
    }
}
