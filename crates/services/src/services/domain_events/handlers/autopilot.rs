//! Autopilot handler for auto-queueing unblocked dependent tasks.
//!
//! When a task is marked as Done, this handler finds dependent tasks that
//! become unblocked and queues them for execution (if autopilot is enabled).

use async_trait::async_trait;
use db::models::{
    execution_queue::ExecutionQueue, session::Session, task::TaskStatus, workspace::Workspace,
};
use executors::profile::ExecutorProfileId;
use tracing::{debug, error, info};

use crate::services::{
    autopilot,
    domain_events::{DomainEvent, EventHandler, ExecutionMode, HandlerContext, HandlerError},
};

/// Handler that auto-queues unblocked dependent tasks when autopilot is enabled.
pub struct AutopilotHandler;

impl AutopilotHandler {
    pub fn new() -> Self {
        Self
    }

    /// Get the executor profile ID from the latest session of a workspace.
    async fn get_executor_profile_for_workspace(
        &self,
        ctx: &HandlerContext,
        workspace_id: uuid::Uuid,
    ) -> Option<ExecutorProfileId> {
        let session = Session::find_latest_by_workspace_id(&ctx.db.pool, workspace_id)
            .await
            .ok()??;

        let executor_str = session.executor.as_ref()?;
        serde_json::from_str(executor_str).ok()
    }
}

impl Default for AutopilotHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventHandler for AutopilotHandler {
    fn name(&self) -> &'static str {
        "autopilot"
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Spawned
    }

    fn handles(&self, event: &DomainEvent) -> bool {
        matches!(
            event,
            DomainEvent::TaskStatusChanged { task, .. } if task.status == TaskStatus::Done
        )
    }

    async fn handle(&self, event: DomainEvent, ctx: &HandlerContext) -> Result<(), HandlerError> {
        let DomainEvent::TaskStatusChanged { task, .. } = event else {
            return Ok(());
        };

        let completed_task_id = task.id;

        // Check if autopilot is enabled
        let autopilot_enabled = ctx.config.read().await.autopilot_enabled;
        if !autopilot_enabled {
            debug!(
                task_id = %completed_task_id,
                "Autopilot disabled, skipping auto-dequeue of dependents"
            );
            return Ok(());
        }

        // Find unblocked dependent tasks
        let unblocked_tasks = autopilot::find_unblocked_dependents(&ctx.db.pool, completed_task_id)
            .await
            .map_err(HandlerError::Database)?;

        if unblocked_tasks.is_empty() {
            debug!(
                task_id = %completed_task_id,
                "No unblocked dependent tasks to auto-dequeue"
            );
            return Ok(());
        }

        info!(
            completed_task_id = %completed_task_id,
            unblocked_count = unblocked_tasks.len(),
            "Auto-dequeueing unblocked dependent tasks"
        );

        let mut enqueued_count = 0;

        for unblocked_task in unblocked_tasks {
            // Find the latest workspace for this task
            let workspace =
                match Workspace::find_latest_by_task_id(&ctx.db.pool, unblocked_task.id).await {
                    Ok(Some(ws)) => ws,
                    Ok(None) => {
                        debug!(
                            task_id = %unblocked_task.id,
                            "Skipping auto-dequeue: task has no workspace"
                        );
                        continue;
                    }
                    Err(e) => {
                        error!(
                            task_id = %unblocked_task.id,
                            error = %e,
                            "Failed to find workspace for unblocked task"
                        );
                        continue;
                    }
                };

            // Get the executor profile from the last session
            let executor_profile_id = match self
                .get_executor_profile_for_workspace(ctx, workspace.id)
                .await
            {
                Some(profile) => profile,
                None => {
                    debug!(
                        task_id = %unblocked_task.id,
                        workspace_id = %workspace.id,
                        "Skipping auto-dequeue: no session found for workspace"
                    );
                    continue;
                }
            };

            // Create execution queue entry
            match ExecutionQueue::create(&ctx.db.pool, workspace.id, &executor_profile_id).await {
                Ok(_) => {
                    info!(
                        task_id = %unblocked_task.id,
                        workspace_id = %workspace.id,
                        executor = %executor_profile_id,
                        "Auto-dequeued unblocked dependent task"
                    );
                    enqueued_count += 1;
                }
                Err(e) => {
                    error!(
                        task_id = %unblocked_task.id,
                        workspace_id = %workspace.id,
                        error = %e,
                        "Failed to create execution queue entry for unblocked task"
                    );
                }
            }
        }

        if enqueued_count > 0 {
            info!(
                completed_task_id = %completed_task_id,
                enqueued_count = enqueued_count,
                "Auto-dequeued unblocked dependent tasks"
            );
        }

        Ok(())
    }
}
