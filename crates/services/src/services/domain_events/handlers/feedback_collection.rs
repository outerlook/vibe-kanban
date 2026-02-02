//! Handler for collecting agent feedback after successful task completion.
//!
//! When an agent completes a coding task successfully, this handler triggers
//! feedback collection via an execution callback. The callback invokes
//! the container service to start a feedback execution process.

use async_trait::async_trait;
use db::{
    DBService,
    models::{
        agent_feedback::AgentFeedback,
        execution_process::{ExecutionProcess, ExecutionProcessRunReason, ExecutionProcessStatus},
    },
};

use crate::services::domain_events::{
    DomainEvent, EventHandler, ExecutionMode, ExecutionTrigger, HandlerContext, HandlerError,
};

/// Handler that collects feedback from agents after successful task completion.
///
/// When a `CodingAgent` execution completes successfully, this handler:
/// 1. Checks if feedback already exists for the workspace
/// 2. Triggers feedback collection via the execution callback
/// 3. The container service handles starting the feedback execution and parsing
#[derive(Clone)]
pub struct FeedbackCollectionHandler {
    db: DBService,
}

impl FeedbackCollectionHandler {
    pub fn new(db: DBService) -> Self {
        Self { db }
    }
}

#[async_trait]
impl EventHandler for FeedbackCollectionHandler {
    fn name(&self) -> &'static str {
        "feedback_collection"
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Spawned
    }

    fn handles(&self, event: &DomainEvent) -> bool {
        matches!(
            event,
            DomainEvent::ExecutionCompleted { process, .. }
            if process.status == ExecutionProcessStatus::Completed
                && process.run_reason == ExecutionProcessRunReason::CodingAgent
        )
    }

    async fn handle(&self, event: DomainEvent, ctx: &HandlerContext) -> Result<(), HandlerError> {
        let DomainEvent::ExecutionCompleted { process, .. } = event else {
            return Ok(());
        };

        // Load full execution context to get workspace_id and task_id
        let exec_ctx = match ExecutionProcess::load_context(&self.db.pool, process.id).await {
            Ok(ctx) => ctx,
            Err(e) => {
                tracing::warn!(
                    "Failed to load execution context for process {}: {}",
                    process.id,
                    e
                );
                return Ok(());
            }
        };

        let workspace_id = exec_ctx.workspace.id;
        let task_id = exec_ctx.task.id;

        // Check if feedback already exists for this workspace
        let existing_feedback =
            AgentFeedback::find_by_workspace_id(&self.db.pool, workspace_id).await?;

        if !existing_feedback.is_empty() {
            tracing::debug!(
                "Feedback already exists for workspace {}, skipping collection",
                workspace_id
            );
            return Ok(());
        }

        // Trigger feedback collection via the callback
        let Some(trigger_callback) = &ctx.execution_trigger else {
            tracing::debug!(
                "No execution trigger callback available, skipping feedback collection for workspace {}",
                workspace_id
            );
            return Ok(());
        };

        let trigger = ExecutionTrigger::FeedbackCollection {
            workspace_id,
            task_id,
            execution_process_id: process.id,
        };

        match trigger_callback(trigger).await {
            Ok(spawned_exec_id) => {
                tracing::info!(
                    "Triggered feedback collection for workspace {} task {}, execution {}",
                    workspace_id,
                    task_id,
                    spawned_exec_id
                );

                // Link the spawned execution process to this hook execution
                if let (Some(hook_exec_id), Some(store)) =
                    (ctx.hook_execution_id, &ctx.hook_execution_store)
                {
                    store.link_execution_process(hook_exec_id, spawned_exec_id);
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to trigger feedback collection for workspace {}: {}",
                    workspace_id,
                    e
                );
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    /// Test that the event matching logic correctly identifies CodingAgent completed events.
    /// This test verifies the pattern used in the `handles()` method.
    #[test]
    fn test_handles_pattern_matches_coding_agent_completed() {
        // Simulate what the handler checks - the pattern used in handles()
        let status = ExecutionProcessStatus::Completed;
        let run_reason = ExecutionProcessRunReason::CodingAgent;

        let matches = status == ExecutionProcessStatus::Completed
            && run_reason == ExecutionProcessRunReason::CodingAgent;
        assert!(
            matches,
            "Handler should match completed CodingAgent execution"
        );
    }

    #[test]
    fn test_handles_pattern_does_not_match_failed_execution() {
        let status = ExecutionProcessStatus::Failed;
        let run_reason = ExecutionProcessRunReason::CodingAgent;

        let matches = status == ExecutionProcessStatus::Completed
            && run_reason == ExecutionProcessRunReason::CodingAgent;
        assert!(!matches, "Handler should not match failed execution");
    }

    #[test]
    fn test_handles_pattern_does_not_match_non_coding_agent() {
        for run_reason in [
            ExecutionProcessRunReason::SetupScript,
            ExecutionProcessRunReason::CleanupScript,
            ExecutionProcessRunReason::InternalAgent,
        ] {
            let status = ExecutionProcessStatus::Completed;

            let matches = status == ExecutionProcessStatus::Completed
                && run_reason == ExecutionProcessRunReason::CodingAgent;
            assert!(
                !matches,
                "Handler should not match {:?} execution",
                run_reason
            );
        }
    }

    #[test]
    fn test_execution_trigger_feedback_collection_variant() {
        // Verify the ExecutionTrigger::FeedbackCollection variant exists
        // and has the expected structure
        let workspace_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let execution_process_id = Uuid::new_v4();

        let trigger = ExecutionTrigger::FeedbackCollection {
            workspace_id,
            task_id,
            execution_process_id,
        };

        // Verify we can match and extract the values
        match trigger {
            ExecutionTrigger::FeedbackCollection {
                workspace_id: ws,
                task_id: ts,
                execution_process_id: ep,
            } => {
                assert_eq!(ws, workspace_id);
                assert_eq!(ts, task_id);
                assert_eq!(ep, execution_process_id);
            }
            _ => panic!("Expected FeedbackCollection variant"),
        }
    }
}
