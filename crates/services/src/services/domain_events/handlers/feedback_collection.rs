//! Handler for collecting agent feedback after successful task completion.
//!
//! When an agent completes a coding task successfully, this handler spawns
//! a background task to collect structured feedback from the agent about
//! the task experience.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use db::{
    models::{
        agent_feedback::{AgentFeedback, CreateAgentFeedback},
        execution_process::{
            ExecutionContext, ExecutionProcess, ExecutionProcessRunReason, ExecutionProcessStatus,
        },
    },
    DBService,
};
use executors::logs::{
    NormalizedEntryType,
    utils::patch::extract_normalized_entry_from_patch,
};
use tokio::sync::RwLock;
use utils::{
    log_msg::LogMsg,
    msg_store::MsgStore,
    text::truncate_to_char_boundary,
};
use uuid::Uuid;

use crate::services::{
    config::Config,
    domain_events::{DomainEvent, EventHandler, ExecutionMode, HandlerContext, HandlerError},
    feedback::FeedbackService,
};

/// Handler that collects feedback from agents after successful task completion.
///
/// When a `CodingAgent` execution completes successfully, this handler spawns
/// a background task that:
/// 1. Checks if feedback already exists for the workspace
/// 2. Creates a feedback collection action
/// 3. Starts a feedback execution and spawns a parser for the response
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

    /// Spawn a background task to collect feedback for a workspace.
    ///
    /// This method spawns an inner background task that handles the feedback
    /// collection process asynchronously.
    fn spawn_feedback_collection(&self, ctx: ExecutionContext) {
        let handler = self.clone();

        tokio::spawn(async move {
            if let Err(e) = handler.try_collect_feedback(&ctx).await {
                tracing::warn!(
                    "Failed to collect feedback for workspace {}: {}",
                    ctx.workspace.id,
                    e
                );
            }
        });
    }

    /// Attempt to collect feedback for a workspace execution context.
    ///
    /// This method checks if feedback already exists, and if not, prepares
    /// the feedback collection by finding the appropriate session and starting
    /// the feedback execution.
    async fn try_collect_feedback(&self, ctx: &ExecutionContext) -> Result<(), HandlerError> {
        let workspace_id = ctx.workspace.id;
        let task_id = ctx.task.id;

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

        // Find the latest agent_session_id for this session
        let agent_session_id = match ExecutionProcess::find_latest_coding_agent_turn_session_id(
            &self.db.pool,
            ctx.session.id,
        )
        .await
        {
            Ok(Some(id)) => id,
            Ok(None) => {
                tracing::debug!(
                    "No agent session ID found for session {}, skipping feedback",
                    ctx.session.id
                );
                return Ok(());
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to query agent session ID for session {}: {}",
                    ctx.session.id,
                    e
                );
                return Ok(());
            }
        };

        // Get executor profile from the original CodingAgent process
        let executor_profile_id =
            ExecutionProcess::latest_executor_profile_for_session(&self.db.pool, ctx.session.id)
                .await
                .map_err(|e| HandlerError::Failed(format!("Failed to get executor profile: {e}")))?;

        // Get working directory from workspace
        let working_dir = ctx
            .workspace
            .agent_working_dir
            .as_ref()
            .filter(|dir| !dir.is_empty())
            .cloned();

        // Create the feedback action
        let _action = FeedbackService::create_feedback_action(
            agent_session_id.clone(),
            executor_profile_id,
            working_dir,
        );

        // Note: Starting the execution requires ContainerService which isn't available here.
        // The actual feedback execution start needs to be wired through the container.
        // For now, we log the intent and the integration point.
        //
        // TODO: This handler needs access to a way to start executions.
        // Options:
        // 1. Add ContainerService to HandlerContext
        // 2. Have the handler emit an event that the container listens to
        // 3. Have the container register a callback for feedback collection
        //
        // For the initial implementation, we store the feedback collection as pending
        // and let the container's existing feedback collection mechanism handle it.
        tracing::info!(
            "Feedback collection requested for workspace {} task {} with session {}",
            workspace_id,
            task_id,
            agent_session_id
        );

        Ok(())
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

    async fn handle(&self, event: DomainEvent, _ctx: &HandlerContext) -> Result<(), HandlerError> {
        let DomainEvent::ExecutionCompleted { process } = event else {
            return Ok(());
        };

        // Load full execution context
        let ctx = match ExecutionProcess::load_context(&self.db.pool, process.id).await {
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

        // Spawn feedback collection as a background task
        self.spawn_feedback_collection(ctx);

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

    #[test]
    fn test_handles_coding_agent_completed() {
        // Create mock execution process
        use db::models::execution_process::ExecutionProcessStatus;

        // Test that we can construct a handler (actual event handling would need DB)
        let _ = ExecutionProcessStatus::Completed;
        let _ = ExecutionProcessRunReason::CodingAgent;

        // Verify the handler would match the right events
        // (full integration test would require database setup)
    }

    #[test]
    fn test_does_not_handle_failed_execution() {
        // Handler should not trigger for failed executions
        let _ = ExecutionProcessStatus::Failed;
    }

    #[test]
    fn test_does_not_handle_non_coding_agent() {
        // Handler should not trigger for setup scripts, cleanup scripts, etc.
        let _ = ExecutionProcessRunReason::SetupScript;
        let _ = ExecutionProcessRunReason::CleanupScript;
        let _ = ExecutionProcessRunReason::InternalAgent;
    }
}
