pub mod executor_approvals;

use std::{collections::HashMap, sync::Arc, time::Duration as StdDuration};

use dashmap::DashMap;
use db::models::{
    execution_process::ExecutionProcess,
    task::{Task, TaskStatus},
};
use executors::{
    approvals::ToolCallMetadata,
    executors::claude::protocol::ProtocolPeer,
    logs::{
        NormalizedEntry, NormalizedEntryType, ToolStatus,
        utils::patch::{ConversationPatch, extract_normalized_entry_from_patch},
    },
};
use futures::future::{BoxFuture, FutureExt, Shared};
use sqlx::{Error as SqlxError, SqlitePool};
use thiserror::Error;
use tokio::sync::{RwLock, oneshot};
use utils::{
    approvals::{ApprovalRequest, ApprovalRequestType, ApprovalResponse, ApprovalStatus},
    log_msg::LogMsg,
    msg_store::MsgStore,
};
use uuid::Uuid;

#[derive(Debug)]
struct PendingApproval {
    entry_index: usize,
    entry: NormalizedEntry,
    execution_process_id: Uuid,
    tool_name: String,
    tool_call_id: String,
    response_tx: oneshot::Sender<ApprovalStatus>,
}

type ApprovalWaiter = Shared<BoxFuture<'static, ApprovalStatus>>;

#[derive(Debug)]
pub struct ToolContext {
    pub tool_name: String,
    pub tool_call_id: String,
    pub execution_process_id: Uuid,
}

#[derive(Clone)]
pub struct Approvals {
    pending: Arc<DashMap<String, PendingApproval>>,
    completed: Arc<DashMap<String, ApprovalStatus>>,
    msg_stores: Arc<RwLock<HashMap<Uuid, Arc<MsgStore>>>>,
    protocol_peers: Arc<RwLock<HashMap<Uuid, ProtocolPeer>>>,
}

#[derive(Debug, Error)]
pub enum ApprovalError {
    #[error("approval request not found")]
    NotFound,
    #[error("approval request already completed")]
    AlreadyCompleted,
    #[error("no executor session found for session_id: {0}")]
    NoExecutorSession(String),
    #[error("corresponding tool use entry not found for approval request")]
    NoToolUseEntry,
    #[error(transparent)]
    Custom(#[from] anyhow::Error),
    #[error(transparent)]
    Sqlx(#[from] SqlxError),
}

impl Approvals {
    pub fn new(
        msg_stores: Arc<RwLock<HashMap<Uuid, Arc<MsgStore>>>>,
        protocol_peers: Arc<RwLock<HashMap<Uuid, ProtocolPeer>>>,
    ) -> Self {
        Self {
            pending: Arc::new(DashMap::new()),
            completed: Arc::new(DashMap::new()),
            msg_stores,
            protocol_peers,
        }
    }

    /// Register a protocol peer for an execution process
    pub async fn register_protocol_peer(
        &self,
        execution_process_id: Uuid,
        peer: ProtocolPeer,
    ) {
        let mut map = self.protocol_peers.write().await;
        map.insert(execution_process_id, peer);
    }

    /// Unregister a protocol peer when execution completes
    pub async fn unregister_protocol_peer(&self, execution_process_id: &Uuid) {
        let mut map = self.protocol_peers.write().await;
        map.remove(execution_process_id);
    }

    /// Get a protocol peer by execution process ID
    async fn protocol_peer_by_id(&self, execution_process_id: &Uuid) -> Option<ProtocolPeer> {
        let map = self.protocol_peers.read().await;
        map.get(execution_process_id).cloned()
    }

    /// Get the protocol peers map for external access
    pub fn protocol_peers(&self) -> &Arc<RwLock<HashMap<Uuid, ProtocolPeer>>> {
        &self.protocol_peers
    }

    pub async fn create_with_waiter(
        &self,
        request: ApprovalRequest,
    ) -> Result<(ApprovalRequest, ApprovalWaiter), ApprovalError> {
        let (tx, rx) = oneshot::channel();
        let waiter: ApprovalWaiter = rx
            .map(|result| result.unwrap_or(ApprovalStatus::TimedOut))
            .boxed()
            .shared();
        let req_id = request.id.clone();

        if let Some(store) = self.msg_store_by_id(&request.execution_process_id).await {
            // Find the matching tool use entry by name and input
            let matching_tool = find_matching_tool_use(store.clone(), &request.tool_call_id);

            if let Some((idx, matching_tool)) = matching_tool {
                let pending_status = match &request.request_type {
                    ApprovalRequestType::ToolApproval { .. } => ToolStatus::PendingApproval {
                        approval_id: req_id.clone(),
                        requested_at: request.created_at,
                        timeout_at: request.timeout_at,
                    },
                    ApprovalRequestType::UserQuestion { questions } => ToolStatus::PendingUserInput {
                        approval_id: req_id.clone(),
                        requested_at: request.created_at,
                        timeout_at: request.timeout_at,
                        questions: questions.clone(),
                    },
                };
                let approval_entry = matching_tool
                    .with_tool_status(pending_status)
                    .ok_or(ApprovalError::NoToolUseEntry)?;
                store.push_patch(ConversationPatch::replace(idx, approval_entry));

                let tool_name = request.tool_name().unwrap_or("unknown").to_string();
                self.pending.insert(
                    req_id.clone(),
                    PendingApproval {
                        entry_index: idx,
                        entry: matching_tool,
                        execution_process_id: request.execution_process_id,
                        tool_name: tool_name.clone(),
                        tool_call_id: request.tool_call_id.clone(),
                        response_tx: tx,
                    },
                );
                tracing::debug!(
                    "Created approval {} for tool '{}' at entry index {}",
                    req_id,
                    tool_name,
                    idx
                );
            } else {
                tracing::warn!(
                    "No matching tool use entry found for approval request: tool='{}', execution_process_id={}",
                    request.tool_name().unwrap_or("unknown"),
                    request.execution_process_id
                );
            }
        } else {
            tracing::warn!(
                "No msg_store found for execution_process_id: {}",
                request.execution_process_id
            );
        }

        self.spawn_timeout_watcher(req_id.clone(), request.timeout_at, waiter.clone());
        Ok((request, waiter))
    }

    #[tracing::instrument(skip(self, id, req))]
    pub async fn respond(
        &self,
        pool: &SqlitePool,
        id: &str,
        req: ApprovalResponse,
    ) -> Result<(ApprovalStatus, ToolContext), ApprovalError> {
        if let Some((_, p)) = self.pending.remove(id) {
            // If answers are provided and status is Approved, convert to Answered
            let final_status = match (&req.status, &req.answers) {
                (ApprovalStatus::Approved, Some(answers)) if !answers.is_empty() => {
                    ApprovalStatus::Answered {
                        answers: answers.clone(),
                    }
                }
                _ => req.status.clone(),
            };

            self.completed.insert(id.to_string(), final_status.clone());
            let _ = p.response_tx.send(final_status.clone());

            if let Some(store) = self.msg_store_by_id(&p.execution_process_id).await {
                let status = ToolStatus::from_approval_status(&final_status).ok_or(
                    ApprovalError::Custom(anyhow::anyhow!("Invalid approval status")),
                )?;
                let updated_entry = p
                    .entry
                    .with_tool_status(status)
                    .ok_or(ApprovalError::NoToolUseEntry)?;

                store.push_patch(ConversationPatch::replace(p.entry_index, updated_entry));
            } else {
                tracing::warn!(
                    "No msg_store found for execution_process_id: {}",
                    p.execution_process_id
                );
            }

            let tool_ctx = ToolContext {
                tool_name: p.tool_name,
                tool_call_id: p.tool_call_id.clone(),
                execution_process_id: p.execution_process_id,
            };

            // If this is an Answered status with answers, send tool_result to Claude
            if let ApprovalStatus::Answered { ref answers } = final_status {
                if let Some(peer) = self.protocol_peer_by_id(&p.execution_process_id).await {
                    let answers_json = serde_json::to_value(answers).unwrap_or_default();
                    if let Err(e) = peer
                        .send_tool_result(p.tool_call_id, answers_json, false)
                        .await
                    {
                        tracing::error!(
                            "Failed to send tool_result for answered question: {}",
                            e
                        );
                    }
                } else {
                    tracing::warn!(
                        "No protocol_peer found for execution_process_id: {}, cannot send tool_result",
                        p.execution_process_id
                    );
                }
            }

            // If approved, answered, or denied, and task is still InReview, move back to InProgress
            if matches!(
                final_status,
                ApprovalStatus::Approved | ApprovalStatus::Answered { .. } | ApprovalStatus::Denied { .. }
            ) && let Ok(ctx) =
                ExecutionProcess::load_context(pool, tool_ctx.execution_process_id).await
                && ctx.task.status == TaskStatus::InReview
                && let Err(e) = Task::update_status(pool, ctx.task.id, TaskStatus::InProgress).await
            {
                tracing::warn!(
                    "Failed to update task status to InProgress after approval response: {}",
                    e
                );
            }

            Ok((final_status, tool_ctx))
        } else if self.completed.contains_key(id) {
            Err(ApprovalError::AlreadyCompleted)
        } else {
            Err(ApprovalError::NotFound)
        }
    }

    #[tracing::instrument(skip(self, id, timeout_at, waiter))]
    fn spawn_timeout_watcher(
        &self,
        id: String,
        timeout_at: chrono::DateTime<chrono::Utc>,
        waiter: ApprovalWaiter,
    ) {
        let pending = self.pending.clone();
        let completed = self.completed.clone();
        let msg_stores = self.msg_stores.clone();

        let now = chrono::Utc::now();
        let to_wait = (timeout_at - now)
            .to_std()
            .unwrap_or_else(|_| StdDuration::from_secs(0));
        let deadline = tokio::time::Instant::now() + to_wait;

        tokio::spawn(async move {
            let status = tokio::select! {
                biased;

                resolved = waiter.clone() => resolved,
                _ = tokio::time::sleep_until(deadline) => ApprovalStatus::TimedOut,
            };

            let is_timeout = matches!(&status, ApprovalStatus::TimedOut);
            completed.insert(id.clone(), status.clone());

            if is_timeout && let Some((_, pending_approval)) = pending.remove(&id) {
                if pending_approval.response_tx.send(status.clone()).is_err() {
                    tracing::debug!("approval '{}' timeout notification receiver dropped", id);
                }

                let store = {
                    let map = msg_stores.read().await;
                    map.get(&pending_approval.execution_process_id).cloned()
                };

                if let Some(store) = store {
                    if let Some(updated_entry) = pending_approval
                        .entry
                        .with_tool_status(ToolStatus::TimedOut)
                    {
                        store.push_patch(ConversationPatch::replace(
                            pending_approval.entry_index,
                            updated_entry,
                        ));
                    } else {
                        tracing::warn!(
                            "Timed out approval '{}' but couldn't update tool status (no tool-use entry).",
                            id
                        );
                    }
                } else {
                    tracing::warn!(
                        "No msg_store found for execution_process_id: {}",
                        pending_approval.execution_process_id
                    );
                }
            }
        });
    }

    async fn msg_store_by_id(&self, execution_process_id: &Uuid) -> Option<Arc<MsgStore>> {
        let map = self.msg_stores.read().await;
        map.get(execution_process_id).cloned()
    }
}

pub(crate) async fn ensure_task_in_review(pool: &SqlitePool, execution_process_id: Uuid) {
    if let Ok(ctx) = ExecutionProcess::load_context(pool, execution_process_id).await
        && ctx.task.status == TaskStatus::InProgress
        && let Err(e) = Task::update_status(pool, ctx.task.id, TaskStatus::InReview).await
    {
        tracing::warn!(
            "Failed to update task status to InReview for approval request: {}",
            e
        );
    }
}

/// Find a matching tool use entry that hasn't been assigned to an approval yet
/// Matches by tool call id from tool metadata
fn find_matching_tool_use(
    store: Arc<MsgStore>,
    tool_call_id: &str,
) -> Option<(usize, NormalizedEntry)> {
    let history = store.get_history();

    // Single loop through history
    for msg in history.iter().rev() {
        if let LogMsg::JsonPatch(patch) = msg
            && let Some((idx, entry)) = extract_normalized_entry_from_patch(patch)
            && let NormalizedEntryType::ToolUse { status, .. } = &entry.entry_type
        {
            // Only match tools that are in Created state
            if !matches!(status, ToolStatus::Created) {
                continue;
            }

            // Match by tool call id from metadata
            if let Some(metadata) = &entry.metadata
                && let Ok(ToolCallMetadata {
                    tool_call_id: entry_call_id,
                    ..
                }) = serde_json::from_value::<ToolCallMetadata>(metadata.clone())
                && entry_call_id == tool_call_id
            {
                tracing::debug!(
                    "Matched tool use entry at index {idx} for tool call id '{tool_call_id}'"
                );
                return Some((idx, entry));
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use executors::logs::{ActionType, NormalizedEntry, NormalizedEntryType, ToolStatus};
    use utils::{
        approvals::{QuestionAnswer, QuestionData, QuestionOption},
        msg_store::MsgStore,
    };

    use super::*;

    fn create_tool_use_entry(
        tool_name: &str,
        file_path: &str,
        id: &str,
        status: ToolStatus,
    ) -> NormalizedEntry {
        NormalizedEntry {
            timestamp: None,
            entry_type: NormalizedEntryType::ToolUse {
                tool_name: tool_name.to_string(),
                action_type: ActionType::FileRead {
                    path: file_path.to_string(),
                },
                status,
            },
            content: format!("Reading {file_path}"),
            metadata: Some(
                serde_json::to_value(ToolCallMetadata {
                    tool_call_id: id.to_string(),
                })
                .unwrap(),
            ),
        }
    }

    #[test]
    fn test_parallel_tool_call_approval_matching() {
        let store = Arc::new(MsgStore::new());

        // Setup: Simulate 3 parallel Read tool calls with different files
        let read_foo = create_tool_use_entry("Read", "foo.rs", "foo-id", ToolStatus::Created);
        let read_bar = create_tool_use_entry("Read", "bar.rs", "bar-id", ToolStatus::Created);
        let read_baz = create_tool_use_entry("Read", "baz.rs", "baz-id", ToolStatus::Created);

        store.push_patch(
            executors::logs::utils::patch::ConversationPatch::add_normalized_entry(0, read_foo),
        );
        store.push_patch(
            executors::logs::utils::patch::ConversationPatch::add_normalized_entry(1, read_bar),
        );
        store.push_patch(
            executors::logs::utils::patch::ConversationPatch::add_normalized_entry(2, read_baz),
        );

        let (idx_foo, _) =
            find_matching_tool_use(store.clone(), "foo-id").expect("Should match foo.rs");
        let (idx_bar, _) =
            find_matching_tool_use(store.clone(), "bar-id").expect("Should match bar.rs");
        let (idx_baz, _) =
            find_matching_tool_use(store.clone(), "baz-id").expect("Should match baz.rs");

        assert_eq!(idx_foo, 0, "foo.rs should match first entry");
        assert_eq!(idx_bar, 1, "bar.rs should match second entry");
        assert_eq!(idx_baz, 2, "baz.rs should match third entry");

        // Test 2: Already pending tools are skipped
        let read_pending = create_tool_use_entry(
            "Read",
            "pending.rs",
            "pending-id",
            ToolStatus::PendingApproval {
                approval_id: "test-id".to_string(),
                requested_at: chrono::Utc::now(),
                timeout_at: chrono::Utc::now(),
            },
        );
        store.push_patch(
            executors::logs::utils::patch::ConversationPatch::add_normalized_entry(3, read_pending),
        );

        assert!(
            find_matching_tool_use(store.clone(), "pending-id").is_none(),
            "Should not match tools in PendingApproval state"
        );

        // Test 3: Wrong tool id returns None
        assert!(
            find_matching_tool_use(store.clone(), "wrong-id").is_none(),
            "Should not match different tool ids"
        );
    }

    #[test]
    fn test_user_question_approval_request_creation() {
        let questions = vec![QuestionData {
            question: "What is your preferred color?".to_string(),
            header: Some("Color Selection".to_string()),
            multi_select: false,
            options: vec![
                QuestionOption {
                    label: "Red".to_string(),
                    description: Some("A warm color".to_string()),
                },
                QuestionOption {
                    label: "Blue".to_string(),
                    description: None,
                },
            ],
        }];

        let request = ApprovalRequest::from_user_question(
            questions.clone(),
            "tool-call-123".to_string(),
            Uuid::new_v4(),
        );

        // Verify request type is UserQuestion
        match &request.request_type {
            ApprovalRequestType::UserQuestion { questions: q } => {
                assert_eq!(q.len(), 1);
                assert_eq!(q[0].question, "What is your preferred color?");
                assert_eq!(q[0].options.len(), 2);
            }
            ApprovalRequestType::ToolApproval { .. } => {
                panic!("Expected UserQuestion, got ToolApproval");
            }
        }

        // Verify tool_name() returns None for user questions
        assert!(request.tool_name().is_none());
    }

    #[test]
    fn test_approval_status_answered_variant() {
        let answers = vec![QuestionAnswer {
            question_index: 0,
            selected_indices: vec![1],
            other_text: None,
        }];

        let status = ApprovalStatus::Answered {
            answers: answers.clone(),
        };

        // Verify it converts to Created tool status (like Approved)
        let tool_status = ToolStatus::from_approval_status(&status);
        assert!(matches!(tool_status, Some(ToolStatus::Created)));
    }

    #[test]
    fn test_tool_status_from_approval_status_exhaustive() {
        // Test Approved -> Created
        assert!(matches!(
            ToolStatus::from_approval_status(&ApprovalStatus::Approved),
            Some(ToolStatus::Created)
        ));

        // Test Answered -> Created
        assert!(matches!(
            ToolStatus::from_approval_status(&ApprovalStatus::Answered { answers: vec![] }),
            Some(ToolStatus::Created)
        ));

        // Test Denied -> Denied
        let denied_status = ToolStatus::from_approval_status(&ApprovalStatus::Denied {
            reason: Some("test".to_string()),
        });
        assert!(matches!(denied_status, Some(ToolStatus::Denied { .. })));

        // Test TimedOut -> TimedOut
        assert!(matches!(
            ToolStatus::from_approval_status(&ApprovalStatus::TimedOut),
            Some(ToolStatus::TimedOut)
        ));

        // Test Pending -> None
        assert!(ToolStatus::from_approval_status(&ApprovalStatus::Pending).is_none());
    }
}
