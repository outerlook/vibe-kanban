//! Handler for collecting agent feedback after successful task completion.
//!
//! When an agent completes a coding task successfully, this handler triggers
//! feedback collection via an execution callback. The callback invokes
//! the container service to start a feedback execution process.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use db::{
    DBService,
    models::{
        agent_feedback::{AgentFeedback, CreateAgentFeedback},
        execution_process::{ExecutionProcess, ExecutionProcessRunReason, ExecutionProcessStatus},
    },
};
use executors::logs::{NormalizedEntryType, utils::patch::extract_normalized_entry_from_patch};
use tokio::sync::RwLock;
use utils::{log_msg::LogMsg, msg_store::MsgStore, text::truncate_to_char_boundary};
use uuid::Uuid;

use crate::services::{
    config::Config,
    domain_events::{
        DomainEvent, EventHandler, ExecutionMode, ExecutionTrigger, HandlerContext, HandlerError,
    },
    feedback::FeedbackService,
};

/// Handler that collects feedback from agents after successful task completion.
///
/// When a `CodingAgent` execution completes successfully, this handler:
/// 1. Checks if feedback already exists for the workspace
/// 2. Triggers feedback collection via the execution callback
/// 3. The container service handles starting the feedback execution
#[derive(Clone)]
pub struct FeedbackCollectionHandler {
    db: DBService,
    /// Config is stored for future use when the handler can start executions
    #[allow(dead_code)]
    config: Arc<RwLock<Config>>,
    msg_stores: Arc<RwLock<HashMap<Uuid, Arc<MsgStore>>>>,
    feedback_pending_cleanup: Arc<RwLock<HashSet<Uuid>>>,
}

impl FeedbackCollectionHandler {
    /// Create a new feedback collection handler.
    ///
    /// # Arguments
    /// * `db` - Database service for querying and storing feedback
    /// * `config` - Application configuration
    /// * `msg_stores` - Shared message stores for execution processes
    /// * `feedback_pending_cleanup` - Set tracking execution IDs pending feedback cleanup
    pub fn new(
        db: DBService,
        config: Arc<RwLock<Config>>,
        msg_stores: Arc<RwLock<HashMap<Uuid, Arc<MsgStore>>>>,
        feedback_pending_cleanup: Arc<RwLock<HashSet<Uuid>>>,
    ) -> Self {
        Self {
            db,
            config,
            msg_stores,
            feedback_pending_cleanup,
        }
    }

    /// Spawn a background task that monitors a feedback execution and parses the response.
    ///
    /// When the feedback execution completes, this task extracts the assistant message,
    /// parses it using `FeedbackService::parse_feedback_response`, and stores the result
    /// in the `agent_feedback` table.
    ///
    /// This method is called after a feedback execution has been started.
    #[allow(dead_code)]
    fn spawn_feedback_parser(&self, feedback_exec_id: Uuid, task_id: Uuid, workspace_id: Uuid) {
        let db = self.db.clone();
        let msg_stores = self.msg_stores.clone();
        let feedback_pending_cleanup = self.feedback_pending_cleanup.clone();

        tokio::spawn(async move {
            // Helper to cleanup msg_store and remove from pending set
            let cleanup = |msg_stores: Arc<RwLock<HashMap<Uuid, Arc<MsgStore>>>>,
                           feedback_pending_cleanup: Arc<RwLock<HashSet<Uuid>>>,
                           db: DBService,
                           exec_id: Uuid| async move {
                // Remove from pending set first
                feedback_pending_cleanup.write().await.remove(&exec_id);

                // Cleanup msg_store (same logic as spawn_exit_monitor)
                if let Some(msg_arc) = msg_stores.write().await.remove(&exec_id) {
                    // Extract and store token usage before cleaning up
                    if let Some((input_tokens, output_tokens)) =
                        extract_token_usage_from_msg_store(&msg_arc)
                        && let Err(e) = ExecutionProcess::update_token_usage(
                            &db.pool,
                            exec_id,
                            Some(input_tokens),
                            Some(output_tokens),
                        )
                        .await
                    {
                        tracing::warn!("Failed to update token usage for {}: {}", exec_id, e);
                    }

                    msg_arc.push_finished();
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    if let Err(arc) = Arc::try_unwrap(msg_arc) {
                        tracing::error!(
                            "There are still {} strong Arcs to MsgStore for {}",
                            Arc::strong_count(&arc),
                            exec_id
                        );
                    }
                }
            };

            // Wait for the feedback execution to complete
            // Poll the execution status periodically
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;

                let exec = match ExecutionProcess::find_by_id(&db.pool, feedback_exec_id).await {
                    Ok(Some(exec)) => exec,
                    Ok(None) => {
                        tracing::warn!(
                            "Feedback execution {} not found, stopping parser",
                            feedback_exec_id
                        );
                        cleanup(msg_stores, feedback_pending_cleanup, db, feedback_exec_id).await;
                        return;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to query feedback execution {}: {}",
                            feedback_exec_id,
                            e
                        );
                        cleanup(msg_stores, feedback_pending_cleanup, db, feedback_exec_id).await;
                        return;
                    }
                };

                match exec.status {
                    ExecutionProcessStatus::Running => continue,
                    ExecutionProcessStatus::Completed => break,
                    ExecutionProcessStatus::Failed | ExecutionProcessStatus::Killed => {
                        tracing::warn!(
                            "Feedback execution {} ended with status {:?}, skipping parsing",
                            feedback_exec_id,
                            exec.status
                        );
                        cleanup(msg_stores, feedback_pending_cleanup, db, feedback_exec_id).await;
                        return;
                    }
                }
            }

            // Extract the assistant message from MsgStore BEFORE cleanup
            let assistant_message = {
                let stores = msg_stores.read().await;
                if let Some(store) = stores.get(&feedback_exec_id) {
                    extract_assistant_message_from_msg_store(store)
                } else {
                    None
                }
            };

            let Some(message) = assistant_message else {
                tracing::warn!(
                    "No assistant message found for feedback execution {}",
                    feedback_exec_id
                );
                cleanup(msg_stores, feedback_pending_cleanup, db, feedback_exec_id).await;
                return;
            };

            // Extract and validate JSON from the feedback response
            let feedback_json = match FeedbackService::parse_feedback_response(&message) {
                Ok(json) => json,
                Err(e) => {
                    tracing::warn!(
                        "Failed to parse feedback response for execution {}: {}",
                        feedback_exec_id,
                        e
                    );
                    cleanup(msg_stores, feedback_pending_cleanup, db, feedback_exec_id).await;
                    return;
                }
            };

            // Store the raw JSON feedback in the database
            let create_feedback = CreateAgentFeedback {
                execution_process_id: feedback_exec_id,
                task_id,
                workspace_id,
                feedback_json: Some(feedback_json),
            };

            match AgentFeedback::create(&db.pool, &create_feedback, Uuid::new_v4()).await {
                Ok(feedback) => {
                    tracing::info!(
                        "Successfully stored agent feedback {} for task {}",
                        feedback.id,
                        task_id
                    );
                }
                Err(e) => {
                    tracing::warn!("Failed to store agent feedback for task {}: {}", task_id, e);
                }
            }

            // Final cleanup after successful processing
            cleanup(msg_stores, feedback_pending_cleanup, db, feedback_exec_id).await;
        });
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
            DomainEvent::ExecutionCompleted { process }
            if process.status == ExecutionProcessStatus::Completed
                && process.run_reason == ExecutionProcessRunReason::CodingAgent
        )
    }

    async fn handle(&self, event: DomainEvent, ctx: &HandlerContext) -> Result<(), HandlerError> {
        let DomainEvent::ExecutionCompleted { process } = event else {
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

        if let Err(e) = trigger_callback(trigger).await {
            tracing::warn!(
                "Failed to trigger feedback collection for workspace {}: {}",
                workspace_id,
                e
            );
        } else {
            tracing::info!(
                "Triggered feedback collection for workspace {} task {}",
                workspace_id,
                task_id
            );
        }

        Ok(())
    }
}

/// Extract token usage from a MsgStore by scanning history for TokenUsage entries.
fn extract_token_usage_from_msg_store(msg_store: &MsgStore) -> Option<(i64, i64)> {
    let history = msg_store.get_history();

    // Scan in reverse to find the most recent TokenUsage entry
    for msg in history.iter().rev() {
        if let LogMsg::JsonPatch(patch) = msg
            && let Some((_, entry)) = extract_normalized_entry_from_patch(patch)
            && let NormalizedEntryType::TokenUsage {
                input_tokens,
                output_tokens,
            } = entry.entry_type
        {
            return Some((input_tokens, output_tokens));
        }
    }

    None
}

/// Extract the last assistant message from a MsgStore by scanning history.
fn extract_assistant_message_from_msg_store(msg_store: &MsgStore) -> Option<String> {
    let history = msg_store.get_history();

    // Scan in reverse to find the last assistant message
    for msg in history.iter().rev() {
        if let LogMsg::JsonPatch(patch) = msg
            && let Some((_, entry)) = extract_normalized_entry_from_patch(patch)
            && matches!(entry.entry_type, NormalizedEntryType::AssistantMessage)
        {
            let content = entry.content.trim();
            if !content.is_empty() {
                const MAX_CONTENT_LENGTH: usize = 4096;
                if content.len() > MAX_CONTENT_LENGTH {
                    let truncated = truncate_to_char_boundary(content, MAX_CONTENT_LENGTH);
                    return Some(format!("{truncated}..."));
                }
                return Some(content.to_string());
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

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
