pub mod executor_approvals;

use std::{collections::HashMap, sync::Arc, time::Duration as StdDuration};

use dashmap::DashMap;
use db::models::{
    execution_process::ExecutionProcess,
    task::{Task, TaskStatus},
    user_question::{CreateUserQuestion, UserQuestion},
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

use super::domain_events::{
    ApprovalEventKind, ApprovalResolution, DomainEvent, DomainEventEntityIds, EventDispatchCallback,
};

#[derive(Debug)]
struct PendingApproval {
    /// Index and entry of the matching tool use. May be None if the entry was
    /// not found at creation time (race condition with async log normalization).
    entry_info: Option<(usize, NormalizedEntry)>,
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
    /// True if the executor was dead and a follow-up should be triggered
    pub needs_follow_up: bool,
}

#[derive(Clone)]
pub struct Approvals {
    db: SqlitePool,
    pending: Arc<DashMap<String, PendingApproval>>,
    requests: Arc<DashMap<String, ApprovalRequest>>,
    completed: Arc<DashMap<String, ApprovalStatus>>,
    msg_stores: Arc<RwLock<HashMap<Uuid, Arc<MsgStore>>>>,
    protocol_peers: Arc<RwLock<HashMap<Uuid, ProtocolPeer>>>,
    event_dispatcher: Arc<RwLock<Option<EventDispatchCallback>>>,
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
        db: SqlitePool,
        msg_stores: Arc<RwLock<HashMap<Uuid, Arc<MsgStore>>>>,
        protocol_peers: Arc<RwLock<HashMap<Uuid, ProtocolPeer>>>,
    ) -> Self {
        Self {
            db,
            pending: Arc::new(DashMap::new()),
            requests: Arc::new(DashMap::new()),
            completed: Arc::new(DashMap::new()),
            msg_stores,
            protocol_peers,
            event_dispatcher: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn set_event_dispatcher(&self, dispatcher: EventDispatchCallback) {
        let mut guard = self.event_dispatcher.write().await;
        *guard = Some(dispatcher);
    }

    /// Register a protocol peer for an execution process
    pub async fn register_protocol_peer(&self, execution_process_id: Uuid, peer: ProtocolPeer) {
        let mut map = self.protocol_peers.write().await;
        map.insert(execution_process_id, peer);
    }

    /// Unregister a protocol peer when execution completes
    pub async fn unregister_protocol_peer(&self, execution_process_id: &Uuid) {
        let mut map = self.protocol_peers.write().await;
        map.remove(execution_process_id);
    }

    /// Get the protocol peers map for external access
    pub fn protocol_peers(&self) -> &Arc<RwLock<HashMap<Uuid, ProtocolPeer>>> {
        &self.protocol_peers
    }

    pub fn find_request(&self, id: &str) -> Option<ApprovalRequest> {
        self.requests.get(id).map(|request| request.clone())
    }

    pub fn find_status(&self, id: &str) -> Option<ApprovalStatus> {
        if self.pending.contains_key(id) {
            Some(ApprovalStatus::Pending)
        } else {
            self.completed.get(id).map(|status| status.clone())
        }
    }

    pub fn list_pending_requests_for_execution_process(
        &self,
        execution_process_id: Uuid,
    ) -> Vec<ApprovalRequest> {
        self.pending
            .iter()
            .filter(|entry| entry.execution_process_id == execution_process_id)
            .filter_map(|entry| {
                self.requests
                    .get(entry.key())
                    .map(|request| request.clone())
            })
            .collect()
    }

    async fn dispatch_event(&self, event: DomainEvent) {
        let dispatcher = { self.event_dispatcher.read().await.clone() };
        if let Some(dispatcher) = dispatcher {
            dispatcher(event).await;
        }
    }

    async fn event_entity_ids(&self, execution_process_id: Uuid) -> DomainEventEntityIds {
        match ExecutionProcess::load_context(&self.db, execution_process_id).await {
            Ok(ctx) => DomainEventEntityIds {
                task_id: Some(ctx.task.id),
                workspace_id: Some(ctx.workspace.id),
                session_id: Some(ctx.session.id),
                execution_process_id: Some(execution_process_id),
                task_group_id: ctx.task.task_group_id,
            },
            Err(error) => {
                tracing::warn!(
                    ?error,
                    execution_process_id = %execution_process_id,
                    "Failed to load approval event context"
                );
                DomainEventEntityIds {
                    execution_process_id: Some(execution_process_id),
                    ..DomainEventEntityIds::default()
                }
            }
        }
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

        self.requests.insert(req_id.clone(), request.clone());

        // For user questions, insert into DB for persistence
        if let ApprovalRequestType::UserQuestion { ref questions } = request.request_type {
            let questions_json = serde_json::to_string(questions).unwrap_or_default();
            let create_data = CreateUserQuestion {
                approval_id: req_id.clone(),
                execution_process_id: request.execution_process_id,
                questions: questions_json,
            };
            if let Err(e) = UserQuestion::create(&self.db, &create_data, Uuid::new_v4()).await {
                tracing::error!("Failed to persist user question to DB: {}", e);
            }
        }

        // Try to find the matching tool use entry and update its status in MsgStore.
        // Due to race conditions between control protocol and log normalization, the
        // entry may not exist yet. In that case, we still insert into pending with
        // entry_info=None to ensure the waiter doesn't immediately resolve.
        let entry_info = if let Some(store) =
            self.msg_store_by_id(&request.execution_process_id).await
        {
            if let Some((idx, matching_tool)) =
                find_matching_tool_use(store.clone(), &request.tool_call_id)
            {
                let pending_status = match &request.request_type {
                    ApprovalRequestType::ToolApproval { .. } => ToolStatus::PendingApproval {
                        approval_id: req_id.clone(),
                        requested_at: request.created_at,
                        timeout_at: request.timeout_at,
                    },
                    ApprovalRequestType::UserQuestion { questions } => {
                        ToolStatus::PendingUserInput {
                            approval_id: req_id.clone(),
                            requested_at: request.created_at,
                            timeout_at: None, // User questions don't timeout
                            questions: questions.clone(),
                        }
                    }
                };
                if let Some(approval_entry) = matching_tool.with_tool_status(pending_status) {
                    store.push_patch(ConversationPatch::replace(idx, approval_entry));
                }
                Some((idx, matching_tool))
            } else {
                tracing::debug!(
                    "No matching tool use entry found yet for approval request: tool='{}', tool_call_id='{}', execution_process_id={}. Entry will be matched on respond.",
                    request.tool_name().unwrap_or("unknown"),
                    request.tool_call_id,
                    request.execution_process_id
                );
                None
            }
        } else {
            tracing::warn!(
                "No msg_store found for execution_process_id: {}",
                request.execution_process_id
            );
            None
        };

        let tool_name = request.tool_name().unwrap_or("unknown").to_string();
        if let Some((idx, _)) = &entry_info {
            tracing::debug!(
                "Created approval {} for tool '{}' at entry index {}",
                req_id,
                tool_name,
                idx
            );
        }

        // Always insert into pending to ensure the waiter doesn't immediately resolve
        self.pending.insert(
            req_id.clone(),
            PendingApproval {
                entry_info,
                execution_process_id: request.execution_process_id,
                tool_name,
                tool_call_id: request.tool_call_id.clone(),
                response_tx: tx,
            },
        );

        // Only spawn timeout watcher if there's a timeout configured
        if let Some(timeout_at) = request.timeout_at {
            self.spawn_timeout_watcher(req_id.clone(), timeout_at, waiter.clone());
        }

        let entity_ids = self.event_entity_ids(request.execution_process_id).await;
        let (kind, tool_name, question_count) = match &request.request_type {
            ApprovalRequestType::ToolApproval { tool_name, .. } => (
                ApprovalEventKind::ToolApproval,
                Some(tool_name.clone()),
                None,
            ),
            ApprovalRequestType::UserQuestion { questions } => {
                (ApprovalEventKind::UserQuestion, None, Some(questions.len()))
            }
        };

        self.dispatch_event(DomainEvent::ApprovalRequested {
            approval_id: request.id.clone(),
            kind,
            tool_call_id: request.tool_call_id.clone(),
            tool_name,
            question_count,
            entity_ids,
            occurred_at: request.created_at,
        })
        .await;

        Ok((request, waiter))
    }

    #[tracing::instrument(skip(self, id, req))]
    pub async fn respond(
        &self,
        pool: &SqlitePool,
        id: &str,
        req: ApprovalResponse,
    ) -> Result<(ApprovalStatus, ToolContext), ApprovalError> {
        // If answers are provided and status is Approved, convert to Answered
        let final_status = match (&req.status, &req.answers) {
            (ApprovalStatus::Approved, Some(answers)) if !answers.is_empty() => {
                ApprovalStatus::Answered {
                    answers: answers.clone(),
                }
            }
            _ => req.status.clone(),
        };

        // Check if we have an active channel (executor alive)
        if let Some((_, p)) = self.pending.remove(id) {
            self.completed.insert(id.to_string(), final_status.clone());
            let _ = p.response_tx.send(final_status.clone());

            // Update MsgStore with the response status
            if let Some(store) = self.msg_store_by_id(&p.execution_process_id).await {
                // Get entry info, either from when we created the pending approval or by
                // searching now (handles race condition where entry wasn't available yet)
                let entry_info = p.entry_info.or_else(|| {
                    tracing::debug!(
                        "Entry info was None at creation, searching for tool_call_id='{}' now",
                        p.tool_call_id
                    );
                    find_matching_tool_use(store.clone(), &p.tool_call_id)
                });

                if let Some((idx, entry)) = entry_info {
                    let status = ToolStatus::from_approval_status(&final_status).ok_or(
                        ApprovalError::Custom(anyhow::anyhow!("Invalid approval status")),
                    )?;
                    if let Some(updated_entry) = entry.with_tool_status(status) {
                        store.push_patch(ConversationPatch::replace(idx, updated_entry));
                    }
                } else {
                    tracing::warn!(
                        "Could not find matching tool use entry for approval response: tool='{}', tool_call_id='{}'",
                        p.tool_name,
                        p.tool_call_id
                    );
                }
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
                needs_follow_up: false, // Executor alive, tool_result sent directly
            };

            // If this is an Answered status with answers, save to DB
            // Note: The answer is delivered to Claude via PermissionResult::Allow { updated_input }
            // in the canUseTool callback (client.rs), not via send_tool_result here.
            if let ApprovalStatus::Answered { ref answers } = final_status {
                let answers_json = serde_json::to_string(answers).unwrap_or_default();
                if let Err(e) = UserQuestion::update_answer(&self.db, id, &answers_json).await {
                    tracing::error!("Failed to save user question answer to DB: {}", e);
                }
            }

            // If approved, answered, or denied, and task is still InReview, move back to InProgress
            if matches!(
                final_status,
                ApprovalStatus::Approved
                    | ApprovalStatus::Answered { .. }
                    | ApprovalStatus::Denied { .. }
            ) && let Ok(ctx) =
                ExecutionProcess::load_context(pool, tool_ctx.execution_process_id).await
                && ctx.task.status == TaskStatus::InReview
            {
                let previous_status = ctx.task.status.clone();
                match Task::update_status(pool, ctx.task.id, TaskStatus::InProgress).await {
                    Ok(_) => {
                        let mut updated_task = ctx.task.clone();
                        updated_task.status = TaskStatus::InProgress;
                        self.dispatch_event(DomainEvent::TaskStatusChanged {
                            task: updated_task,
                            previous_status,
                        })
                        .await;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to update task status to InProgress after approval response: {}",
                            e
                        );
                    }
                }
            }

            let entity_ids = self.event_entity_ids(tool_ctx.execution_process_id).await;
            self.dispatch_event(DomainEvent::ApprovalResolved {
                approval_id: id.to_string(),
                resolution: approval_resolution(&final_status),
                tool_call_id: Some(tool_ctx.tool_call_id.clone()),
                entity_ids,
                occurred_at: chrono::Utc::now(),
            })
            .await;

            Ok((final_status, tool_ctx))
        } else if self.completed.contains_key(id) {
            Err(ApprovalError::AlreadyCompleted)
        } else {
            // No channel exists - check if this is a persisted user question
            if let Ok(Some(user_question)) = UserQuestion::get_by_approval_id(&self.db, id).await {
                // Save answer to DB
                if let ApprovalStatus::Answered { ref answers } = final_status {
                    let answers_json = serde_json::to_string(answers).unwrap_or_default();
                    if let Err(e) = UserQuestion::update_answer(&self.db, id, &answers_json).await {
                        tracing::error!("Failed to save user question answer to DB: {}", e);
                        return Err(ApprovalError::Custom(anyhow::anyhow!(
                            "Failed to save answer: {}",
                            e
                        )));
                    }
                }

                self.completed.insert(id.to_string(), final_status.clone());

                // Return a tool context with the stored execution_process_id
                // The caller (follow-up trigger) will use this to start a new execution
                let tool_ctx = ToolContext {
                    tool_name: "AskUserQuestion".to_string(),
                    tool_call_id: id.to_string(),
                    execution_process_id: user_question.execution_process_id,
                    needs_follow_up: true, // Executor was dead, needs follow-up
                };

                let entity_ids = self.event_entity_ids(tool_ctx.execution_process_id).await;
                self.dispatch_event(DomainEvent::ApprovalResolved {
                    approval_id: id.to_string(),
                    resolution: approval_resolution(&final_status),
                    tool_call_id: Some(tool_ctx.tool_call_id.clone()),
                    entity_ids,
                    occurred_at: chrono::Utc::now(),
                })
                .await;

                Ok((final_status, tool_ctx))
            } else {
                Err(ApprovalError::NotFound)
            }
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
        let approvals = self.clone();

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
                    // Get entry info, either from when we created the pending approval or by
                    // searching now (handles race condition where entry wasn't available yet)
                    let entry_info = pending_approval.entry_info.or_else(|| {
                        find_matching_tool_use(store.clone(), &pending_approval.tool_call_id)
                    });

                    if let Some((idx, entry)) = entry_info {
                        if let Some(updated_entry) = entry.with_tool_status(ToolStatus::TimedOut) {
                            store.push_patch(ConversationPatch::replace(idx, updated_entry));
                        }
                    } else {
                        tracing::warn!(
                            "Timed out approval '{}' but couldn't find matching tool use entry.",
                            id
                        );
                    }
                } else {
                    tracing::warn!(
                        "No msg_store found for execution_process_id: {}",
                        pending_approval.execution_process_id
                    );
                }

                let entity_ids = approvals
                    .event_entity_ids(pending_approval.execution_process_id)
                    .await;
                approvals
                    .dispatch_event(DomainEvent::ApprovalResolved {
                        approval_id: id.clone(),
                        resolution: ApprovalResolution::TimedOut,
                        tool_call_id: Some(pending_approval.tool_call_id.clone()),
                        entity_ids,
                        occurred_at: chrono::Utc::now(),
                    })
                    .await;
            }
        });
    }

    async fn msg_store_by_id(&self, execution_process_id: &Uuid) -> Option<Arc<MsgStore>> {
        let map = self.msg_stores.read().await;
        map.get(execution_process_id).cloned()
    }
}

fn approval_resolution(status: &ApprovalStatus) -> ApprovalResolution {
    match status {
        ApprovalStatus::Approved => ApprovalResolution::Approved,
        ApprovalStatus::Denied { .. } => ApprovalResolution::Denied,
        ApprovalStatus::Answered { .. } => ApprovalResolution::Answered,
        ApprovalStatus::TimedOut | ApprovalStatus::Pending => ApprovalResolution::TimedOut,
    }
}

pub(crate) async fn ensure_task_in_review(
    approvals: &Approvals,
    pool: &SqlitePool,
    execution_process_id: Uuid,
) {
    if let Ok(ctx) = ExecutionProcess::load_context(pool, execution_process_id).await
        && ctx.task.status == TaskStatus::InProgress
    {
        let previous_status = ctx.task.status.clone();
        match Task::update_status(pool, ctx.task.id, TaskStatus::InReview).await {
            Ok(_) => {
                let mut updated_task = ctx.task.clone();
                updated_task.status = TaskStatus::InReview;
                approvals
                    .dispatch_event(DomainEvent::TaskStatusChanged {
                        task: updated_task,
                        previous_status,
                    })
                    .await;
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to update task status to InReview for approval request: {}",
                    e
                );
            }
        }
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
    use std::{collections::HashMap, sync::Arc, time::Duration};

    use db::{
        DBService,
        models::{
            execution_process::{
                CreateExecutionProcess, ExecutionProcess, ExecutionProcessRunReason,
            },
            project::{CreateProject, Project},
            session::{CreateSession, Session},
            task::{CreateTask, Task, TaskStatus},
            workspace::{CreateWorkspace, Workspace},
        },
    };
    use executors::{
        actions::{
            ExecutorAction, ExecutorActionType,
            script::{ScriptContext, ScriptRequest, ScriptRequestLanguage},
        },
        logs::{ActionType, NormalizedEntry, NormalizedEntryType, ToolStatus},
    };
    use utils::{
        approvals::{CreateApprovalRequest, QuestionAnswer, QuestionData, QuestionOption},
        msg_store::MsgStore,
    };

    use super::*;
    use crate::services::domain_events::{
        EventDispatchCallback, OrchestrationEventMapper, OrchestrationEventPublisher,
        OrchestrationEventType, RecordingOrchestrationEventPublisher, default_topic_namespace,
    };

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
                timeout_at: Some(chrono::Utc::now()),
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

    fn recording_dispatcher(
        db: &DBService,
        publisher: RecordingOrchestrationEventPublisher,
    ) -> EventDispatchCallback {
        let db = db.clone();
        Arc::new(move |event| {
            let db = db.clone();
            let publisher = publisher.clone();
            Box::pin(async move {
                let mapper = OrchestrationEventMapper::new(db.pool.clone());
                let envelopes = mapper
                    .map_event(&event)
                    .await
                    .expect("map orchestration event");
                for envelope in envelopes {
                    let event_name = serde_json::to_string(&envelope.event_type())
                        .expect("event type serialization cannot fail")
                        .trim_matches('"')
                        .to_string();
                    publisher
                        .publish(
                            format!("{}/{}", default_topic_namespace(), event_name),
                            envelope,
                        )
                        .await
                        .expect("publish orchestration event");
                }
            })
        })
    }

    async fn wait_for_event_types(
        publisher: &RecordingOrchestrationEventPublisher,
        minimum: usize,
    ) -> Vec<OrchestrationEventType> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            let event_types = publisher
                .published()
                .into_iter()
                .map(|(_, envelope)| envelope.event_type())
                .collect::<Vec<_>>();
            if event_types.len() >= minimum || tokio::time::Instant::now() >= deadline {
                return event_types;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    async fn create_execution_context(
        db: &DBService,
        status: TaskStatus,
    ) -> (Task, Workspace, Session, ExecutionProcess) {
        let project = Project::create(
            &db.pool,
            &CreateProject {
                name: "approval-events".to_string(),
                repositories: vec![],
            },
            Uuid::new_v4(),
        )
        .await
        .expect("create project");

        let task = Task::create(
            &db.pool,
            &CreateTask {
                project_id: project.id,
                title: "approval-task".to_string(),
                description: None,
                status: Some(status),
                parent_workspace_id: None,
                image_ids: None,
                shared_task_id: None,
                task_group_id: None,
            },
            Uuid::new_v4(),
        )
        .await
        .expect("create task");

        let workspace = Workspace::create(
            &db.pool,
            &CreateWorkspace {
                branch: "approval-task".to_string(),
                agent_working_dir: None,
            },
            Uuid::new_v4(),
            task.id,
        )
        .await
        .expect("create workspace");

        let session = Session::create(
            &db.pool,
            &CreateSession {
                executor: Some("\"claude-code\"".to_string()),
            },
            Uuid::new_v4(),
            workspace.id,
        )
        .await
        .expect("create session");

        let execution = ExecutionProcess::create(
            &db.pool,
            &CreateExecutionProcess {
                session_id: session.id,
                executor_action: ExecutorAction::new(
                    ExecutorActionType::ScriptRequest(ScriptRequest {
                        script: "echo approval".to_string(),
                        language: ScriptRequestLanguage::Bash,
                        context: ScriptContext::SetupScript,
                        working_dir: None,
                    }),
                    None,
                ),
                run_reason: ExecutionProcessRunReason::CodingAgent,
            },
            Uuid::new_v4(),
            &[],
        )
        .await
        .expect("create execution process");

        (task, workspace, session, execution)
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn approvals_emit_request_resolution_and_status_events() {
        let _lock = crate::services::TEST_DB_LOCK.lock().expect("lock poisoned");
        let db = DBService::new().await.expect("db service");
        let msg_stores = Arc::new(tokio::sync::RwLock::new(HashMap::new()));
        let publisher = RecordingOrchestrationEventPublisher::default();
        let approvals = Approvals::new(
            db.pool.clone(),
            msg_stores,
            Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        );
        approvals
            .set_event_dispatcher(recording_dispatcher(&db, publisher.clone()))
            .await;

        let (task, _workspace, _session, execution) =
            create_execution_context(&db, TaskStatus::InProgress).await;

        ensure_task_in_review(&approvals, &db.pool, execution.id).await;

        let request = ApprovalRequest::from_create(
            CreateApprovalRequest {
                tool_name: "Write".to_string(),
                tool_input: serde_json::json!({ "path": "src/main.rs" }),
                tool_call_id: "tool-call-1".to_string(),
            },
            execution.id,
        );
        let (request, _waiter) = approvals
            .create_with_waiter(request)
            .await
            .expect("create approval");

        approvals
            .respond(
                &db.pool,
                &request.id,
                ApprovalResponse {
                    execution_process_id: execution.id,
                    status: ApprovalStatus::Approved,
                    answers: None,
                },
            )
            .await
            .expect("respond approval");

        let event_types = wait_for_event_types(&publisher, 4).await;
        assert!(event_types.contains(&OrchestrationEventType::ApprovalRequested));
        assert!(event_types.contains(&OrchestrationEventType::ApprovalResolved));
        assert_eq!(
            event_types
                .iter()
                .filter(|event_type| **event_type == OrchestrationEventType::TaskStatusChanged)
                .count(),
            2,
            "expected one task status change into review and one back into progress"
        );

        let updated_task = Task::find_by_id(&db.pool, task.id)
            .await
            .expect("load task")
            .expect("task exists");
        assert_eq!(updated_task.status, TaskStatus::InProgress);
    }
}
