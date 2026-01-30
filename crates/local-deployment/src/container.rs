use std::{
    collections::{HashMap, HashSet},
    io,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};

use dashmap::DashSet;

/// Tracks which projects have an active merge queue processor.
/// This prevents multiple processors from being spawned for the same project,
/// which could cause race conditions when processing the queue.
static ACTIVE_MERGE_PROCESSORS: LazyLock<Mutex<HashSet<Uuid>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

/// Default prompt template for generating commit messages via AI.
/// Mirrors the constant in `crates/server/src/routes/task_attempts/pr.rs`.
const DEFAULT_COMMIT_MESSAGE_PROMPT: &str = r#"Generate a concise git commit message for the following changes.

Task: {task_title}
Description: {task_description}

Diff:
{diff}

Write a commit message following these guidelines:
- First line: imperative mood summary (50 chars max)
- Blank line
- Body: explain what and why (wrap at 72 chars)

Respond with ONLY the commit message, no other text."#;

use anyhow::anyhow;
use async_trait::async_trait;
use command_group::AsyncGroupChild;
use db::{
    DBService,
    models::{
        agent_feedback::{AgentFeedback, CreateAgentFeedback},
        coding_agent_turn::CodingAgentTurn,
        conversation_session::ConversationSession,
        execution_process::{
            ExecutionContext, ExecutionProcess, ExecutionProcessRunReason, ExecutionProcessStatus,
        },
        execution_process_normalized_entry::ExecutionProcessNormalizedEntry,
        execution_process_repo_state::ExecutionProcessRepoState,
        project_repo::ProjectRepo,
        repo::Repo,
        review_attention::{CreateReviewAttention, ReviewAttention},
        scratch::{DraftFollowUpData, Scratch, ScratchType},
        session::{CreateSession, Session},
        task::{Task, TaskStatus},
        workspace::Workspace,
        workspace_repo::WorkspaceRepo,
    },
};
use deployment::{DeploymentError, RemoteClientNotConfigured};
use executors::{
    actions::{
        Executable, ExecutorAction, ExecutorActionType,
        coding_agent_follow_up::CodingAgentFollowUpRequest,
        coding_agent_initial::CodingAgentInitialRequest,
    },
    approvals::{ExecutorApprovalService, NoopExecutorApprovalService},
    env::ExecutionEnv,
    executors::{BaseCodingAgent, ExecutorExitResult, ExecutorExitSignal, InterruptSender},
    logs::{NormalizedEntryType, utils::patch::extract_normalized_entry_from_patch},
    profile::ExecutorProfileId,
};
use futures::{FutureExt, TryStreamExt, stream::select};
use serde_json::json;
use services::services::{
    analytics::AnalyticsContext,
    approvals::{Approvals, executor_approvals::ExecutorApprovalBridge},
    config::Config,
    container::{ContainerError, ContainerRef, ContainerService},
    conversation::ConversationService,
    diff_stream::{self, DiffStreamHandle},
    domain_events::{
        AutopilotHandler, DispatcherBuilder, DomainEvent, DomainEventDispatcher,
        ExecutionTrigger, ExecutionTriggerCallback, FeedbackCollectionHandler, HandlerContext,
        NotificationHandler, RemoteSyncHandler, ReviewAttentionHandler, WebSocketBroadcastHandler,
    },
    feedback::FeedbackService,
    git::{Commit, DiffTarget, GitCli, GitService},
    image::ImageService,
    merge_queue_processor::MergeQueueProcessor,
    merge_queue_store::MergeQueueStore,
    notification::NotificationService,
    operation_status::{OperationStatus, OperationStatusStore, OperationStatusType},
    queued_message::QueuedMessageService,
    review_attention::ReviewAttentionService,
    share::SharePublisher,
    skills_cache::GlobalSkillsCache,
    watcher_manager::WatcherManager,
    workspace_manager::{RepoWorkspaceInput, WorkspaceManager},
};
use tokio::{sync::RwLock, task::JoinHandle};
use tokio_util::io::ReaderStream;
use utils::{
    diff::create_unified_diff,
    log_msg::LogMsg,
    msg_store::MsgStore,
    text::{git_branch_id, short_uuid, truncate_to_char_boundary},
};
use uuid::Uuid;

use crate::{command, copy};
use utils::assets::ClaudeCodeHookAssets;

/// Extract token usage from a MsgStore by scanning history for TokenUsage entries
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

/// Extract the last assistant message from a MsgStore by scanning history
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

#[derive(Clone)]
pub struct LocalContainerService {
    db: DBService,
    child_store: Arc<RwLock<HashMap<Uuid, Arc<RwLock<AsyncGroupChild>>>>>,
    interrupt_senders: Arc<RwLock<HashMap<Uuid, InterruptSender>>>,
    msg_stores: Arc<RwLock<HashMap<Uuid, Arc<MsgStore>>>>,
    config: Arc<RwLock<Config>>,
    git: GitService,
    image_service: ImageService,
    analytics: Option<AnalyticsContext>,
    approvals: Approvals,
    queued_message_service: QueuedMessageService,
    publisher: Result<SharePublisher, RemoteClientNotConfigured>,
    notification_service: NotificationService,
    watcher_manager: WatcherManager,
    skills_cache: GlobalSkillsCache,
    /// Execution IDs for which feedback parser is pending - skip msg_store cleanup in exit monitor
    feedback_pending_cleanup: Arc<RwLock<HashSet<Uuid>>>,
    /// Workspace IDs that currently have a running agent - used to prevent duplicate spawns
    running_workspaces: Arc<DashSet<Uuid>>,
    /// MergeQueueStore for autopilot merge functionality - set after construction
    merge_queue_store: Arc<RwLock<Option<MergeQueueStore>>>,
    /// OperationStatusStore for tracking merge operations - set after construction
    operation_status: Arc<RwLock<Option<OperationStatusStore>>>,
    /// Domain event dispatcher for routing events to handlers
    event_dispatcher: Arc<DomainEventDispatcher>,
}

impl LocalContainerService {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        db: DBService,
        msg_stores: Arc<RwLock<HashMap<Uuid, Arc<MsgStore>>>>,
        config: Arc<RwLock<Config>>,
        git: GitService,
        image_service: ImageService,
        analytics: Option<AnalyticsContext>,
        approvals: Approvals,
        queued_message_service: QueuedMessageService,
        publisher: Result<SharePublisher, RemoteClientNotConfigured>,
        skills_cache: GlobalSkillsCache,
    ) -> Self {
        let child_store = Arc::new(RwLock::new(HashMap::new()));
        let interrupt_senders = Arc::new(RwLock::new(HashMap::new()));
        let notification_service = NotificationService::new(config.clone());
        let feedback_pending_cleanup = Arc::new(RwLock::new(HashSet::new()));
        let running_workspaces = Arc::new(DashSet::new());

        // Create a global MsgStore for WebSocket broadcasts (shared across all handlers)
        let global_msg_store = Arc::new(MsgStore::default());

        // Create late-bound container reference for the execution trigger callback.
        // This allows handlers to trigger executions via the callback without circular
        // dependencies during construction.
        let container_ref: Arc<RwLock<Option<LocalContainerService>>> =
            Arc::new(RwLock::new(None));

        // Create execution trigger callback that routes triggers to container methods
        let callback_container_ref = Arc::clone(&container_ref);
        let execution_trigger_callback: ExecutionTriggerCallback =
            Arc::new(move |trigger: ExecutionTrigger| {
                let container_ref = Arc::clone(&callback_container_ref);
                async move {
                    let container = {
                        let guard = container_ref.read().await;
                        guard.clone().ok_or_else(|| {
                            anyhow!("Container not initialized for execution trigger")
                        })?
                    };

                    match trigger {
                        ExecutionTrigger::FeedbackCollection {
                            workspace_id: _,
                            task_id,
                            execution_process_id,
                        } => {
                            // Load context from execution process ID
                            let ctx = ExecutionProcess::load_context(
                                &container.db.pool,
                                execution_process_id,
                            )
                            .await
                            .map_err(|e| anyhow!("Failed to load execution context: {e}"))?;

                            // Find the agent session ID
                            let agent_session_id =
                                ExecutionProcess::find_latest_coding_agent_turn_session_id(
                                    &container.db.pool,
                                    ctx.session.id,
                                )
                                .await
                                .map_err(|e| anyhow!("Failed to query agent session ID: {e}"))?
                                .ok_or_else(|| {
                                    anyhow!(
                                        "No agent session ID found for session {}, cannot collect feedback",
                                        ctx.session.id
                                    )
                                })?;

                            // Spawn feedback collection as background task (non-blocking)
                            let feedback_container = container.clone();
                            tokio::spawn(async move {
                                if let Err(e) = feedback_container
                                    .collect_agent_feedback(&ctx, &agent_session_id)
                                    .await
                                {
                                    tracing::warn!(
                                        "Failed to start feedback collection for task {}: {}",
                                        task_id,
                                        e
                                    );
                                }
                            });

                            Ok(())
                        }
                        ExecutionTrigger::ReviewAttention {
                            task_id,
                            execution_process_id,
                        } => {
                            // Check if review attention is enabled
                            let is_enabled = {
                                let config = container.config.read().await;
                                config.review_attention_executor_profile.is_some()
                            };

                            if !is_enabled {
                                tracing::debug!(
                                    "Review attention is disabled, skipping for task {}",
                                    task_id
                                );
                                return Ok(());
                            }

                            // Load context from execution process ID
                            let ctx = ExecutionProcess::load_context(
                                &container.db.pool,
                                execution_process_id,
                            )
                            .await
                            .map_err(|e| anyhow!("Failed to load execution context: {e}"))?;

                            // Find the agent session ID
                            let agent_session_id =
                                ExecutionProcess::find_latest_coding_agent_turn_session_id(
                                    &container.db.pool,
                                    ctx.session.id,
                                )
                                .await
                                .map_err(|e| anyhow!("Failed to query agent session ID: {e}"))?
                                .ok_or_else(|| {
                                    anyhow!(
                                        "No agent session ID found for session {}, cannot collect review attention",
                                        ctx.session.id
                                    )
                                })?;

                            // Spawn review attention collection as background task (non-blocking)
                            let review_container = container.clone();
                            tokio::spawn(async move {
                                if let Err(e) = review_container
                                    .collect_review_attention(&ctx, &agent_session_id)
                                    .await
                                {
                                    tracing::debug!(
                                        "Failed to start review attention for task {}: {}",
                                        task_id,
                                        e
                                    );
                                }
                            });

                            Ok(())
                        }
                    }
                }
                .boxed()
            });

        // Build the domain event dispatcher with all handlers
        let event_dispatcher = Arc::new(
            DispatcherBuilder::new()
                .with_handler(WebSocketBroadcastHandler::new())
                .with_handler(NotificationHandler::new(notification_service.clone()))
                .with_handler(AutopilotHandler::new())
                .with_handler(RemoteSyncHandler::new(publisher.clone().ok()))
                .with_handler(ReviewAttentionHandler::new())
                .with_handler(FeedbackCollectionHandler::new(
                    db.clone(),
                    config.clone(),
                    msg_stores.clone(),
                    feedback_pending_cleanup.clone(),
                ))
                .with_context(HandlerContext::new(
                    db.clone(),
                    config.clone(),
                    global_msg_store,
                    None, // Will be overridden by with_execution_trigger
                ))
                .with_execution_trigger(execution_trigger_callback)
                .build(),
        );

        let container = LocalContainerService {
            db,
            child_store,
            interrupt_senders,
            msg_stores,
            config,
            git,
            image_service,
            analytics,
            approvals,
            queued_message_service,
            publisher,
            notification_service,
            watcher_manager: WatcherManager::new(),
            skills_cache,
            feedback_pending_cleanup,
            running_workspaces,
            merge_queue_store: Arc::new(RwLock::new(None)),
            operation_status: Arc::new(RwLock::new(None)),
            event_dispatcher,
        };

        // Initialize the late-bound container reference so the callback can use it
        *container_ref.write().await = Some(container.clone());

        container.spawn_workspace_cleanup().await;

        container
    }

    /// Set the MergeQueueStore and OperationStatusStore for autopilot merge functionality.
    /// This must be called after the deployment is constructed, as these services are
    /// created after the container.
    pub async fn set_merge_services(
        &self,
        merge_queue_store: MergeQueueStore,
        operation_status: OperationStatusStore,
    ) {
        *self.merge_queue_store.write().await = Some(merge_queue_store);
        *self.operation_status.write().await = Some(operation_status);
    }

    pub async fn get_child_from_store(&self, id: &Uuid) -> Option<Arc<RwLock<AsyncGroupChild>>> {
        let map = self.child_store.read().await;
        map.get(id).cloned()
    }

    pub async fn add_child_to_store(&self, id: Uuid, exec: AsyncGroupChild) {
        let mut map = self.child_store.write().await;
        map.insert(id, Arc::new(RwLock::new(exec)));
    }

    pub async fn remove_child_from_store(&self, id: &Uuid) {
        let mut map = self.child_store.write().await;
        map.remove(id);
    }

    async fn add_interrupt_sender(&self, id: Uuid, sender: InterruptSender) {
        let mut map = self.interrupt_senders.write().await;
        map.insert(id, sender);
    }

    async fn take_interrupt_sender(&self, id: &Uuid) -> Option<InterruptSender> {
        let mut map = self.interrupt_senders.write().await;
        map.remove(id)
    }

    pub async fn cleanup_workspace(db: &DBService, workspace: &Workspace) {
        let Some(container_ref) = &workspace.container_ref else {
            return;
        };
        let workspace_dir = PathBuf::from(container_ref);

        let repositories = WorkspaceRepo::find_repos_for_workspace(&db.pool, workspace.id)
            .await
            .unwrap_or_default();

        if repositories.is_empty() {
            tracing::warn!(
                "No repositories found for workspace {}, cleaning up workspace directory only",
                workspace.id
            );
            if workspace_dir.exists()
                && let Err(e) = tokio::fs::remove_dir_all(&workspace_dir).await
            {
                tracing::warn!("Failed to remove workspace directory: {}", e);
            }
        } else {
            WorkspaceManager::cleanup_workspace(&workspace_dir, &repositories)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(
                        "Failed to clean up workspace for workspace {}: {}",
                        workspace.id,
                        e
                    );
                });
        }

        // Clear container_ref so this workspace won't be picked up again
        let _ = Workspace::clear_container_ref(&db.pool, workspace.id).await;
    }

    pub async fn cleanup_expired_workspaces(db: &DBService) -> Result<(), DeploymentError> {
        let expired_workspaces = Workspace::find_expired_for_cleanup(&db.pool).await?;
        if expired_workspaces.is_empty() {
            tracing::debug!("No expired workspaces found");
            return Ok(());
        }
        tracing::info!(
            "Found {} expired workspaces to clean up",
            expired_workspaces.len()
        );
        for workspace in &expired_workspaces {
            Self::cleanup_workspace(db, workspace).await;
        }
        Ok(())
    }

    pub async fn spawn_workspace_cleanup(&self) {
        let db = self.db.clone();
        let mut cleanup_interval = tokio::time::interval(tokio::time::Duration::from_secs(1800)); // 30 minutes
        WorkspaceManager::cleanup_orphan_workspaces(&self.db.pool).await;
        tokio::spawn(async move {
            loop {
                cleanup_interval.tick().await;
                tracing::info!("Starting periodic workspace cleanup...");
                Self::cleanup_expired_workspaces(&db)
                    .await
                    .unwrap_or_else(|e| {
                        tracing::error!("Failed to clean up expired workspaces: {}", e)
                    });
            }
        });
    }

    /// Record the current HEAD commit for each repository as the "after" state.
    /// Errors are silently ignored since this runs after the main execution completes
    /// and failure should not block process finalization.
    async fn update_after_head_commits(&self, exec_id: Uuid) {
        if let Ok(ctx) = ExecutionProcess::load_context(&self.db.pool, exec_id).await {
            let workspace_root = self.workspace_to_current_dir(&ctx.workspace);
            for repo in &ctx.repos {
                let repo_path = workspace_root.join(&repo.name);
                if let Ok(head) = self.git().get_head_info(&repo_path) {
                    let _ = ExecutionProcessRepoState::update_after_head_commit(
                        &self.db.pool,
                        exec_id,
                        repo.id,
                        &head.oid,
                    )
                    .await;
                }
            }
        }
    }

    /// Get the commit message based on the execution run reason.
    async fn get_commit_message(&self, ctx: &ExecutionContext) -> String {
        match ctx.execution_process.run_reason {
            ExecutionProcessRunReason::CodingAgent => {
                // Try to retrieve the task summary from the coding agent turn
                // otherwise fallback to default message
                match CodingAgentTurn::find_by_execution_process_id(
                    &self.db().pool,
                    ctx.execution_process.id,
                )
                .await
                {
                    Ok(Some(turn)) if turn.summary.is_some() => turn.summary.unwrap(),
                    Ok(_) => {
                        tracing::debug!(
                            "No summary found for execution process {}, using default message",
                            ctx.execution_process.id
                        );
                        format!(
                            "Commit changes from coding agent for workspace {}",
                            ctx.workspace.id
                        )
                    }
                    Err(e) => {
                        tracing::debug!(
                            "Failed to retrieve summary for execution process {}: {}",
                            ctx.execution_process.id,
                            e
                        );
                        format!(
                            "Commit changes from coding agent for workspace {}",
                            ctx.workspace.id
                        )
                    }
                }
            }
            ExecutionProcessRunReason::CleanupScript => {
                format!("Cleanup script changes for workspace {}", ctx.workspace.id)
            }
            _ => format!(
                "Changes from execution process {}",
                ctx.execution_process.id
            ),
        }
    }

    /// Check which repos have uncommitted changes. Fails if any repo is inaccessible.
    fn check_repos_for_changes(
        &self,
        workspace_root: &Path,
        repos: &[Repo],
    ) -> Result<Vec<(Repo, PathBuf)>, ContainerError> {
        let git = GitCli::new();
        let mut repos_with_changes = Vec::new();

        for repo in repos {
            let worktree_path = workspace_root.join(&repo.name);

            match git.has_changes(&worktree_path) {
                Ok(true) => {
                    repos_with_changes.push((repo.clone(), worktree_path));
                }
                Ok(false) => {
                    tracing::debug!("No changes in repo '{}'", repo.name);
                }
                Err(e) => {
                    return Err(ContainerError::Other(anyhow!(
                        "Pre-flight check failed for repo '{}': {}",
                        repo.name,
                        e
                    )));
                }
            }
        }

        Ok(repos_with_changes)
    }

    /// Commit changes to each repo. Logs failures but continues with other repos.
    fn commit_repos(&self, repos_with_changes: Vec<(Repo, PathBuf)>, message: &str) -> bool {
        let mut any_committed = false;

        for (repo, worktree_path) in repos_with_changes {
            tracing::debug!(
                "Committing changes for repo '{}' at {:?}",
                repo.name,
                &worktree_path
            );

            match self.git().commit(&worktree_path, message) {
                Ok(true) => {
                    any_committed = true;
                    tracing::info!("Committed changes in repo '{}'", repo.name);
                }
                Ok(false) => {
                    tracing::warn!("No changes committed in repo '{}' (unexpected)", repo.name);
                }
                Err(e) => {
                    tracing::warn!("Failed to commit in repo '{}': {}", repo.name, e);
                }
            }
        }

        any_committed
    }

    /// Spawn a background task that polls the child process for completion and
    /// cleans up the execution entry when it exits.
    pub fn spawn_exit_monitor(
        &self,
        exec_id: &Uuid,
        exit_signal: Option<ExecutorExitSignal>,
        workspace_id: Uuid,
        run_reason: ExecutionProcessRunReason,
    ) -> JoinHandle<()> {
        let exec_id = *exec_id;
        let child_store = self.child_store.clone();
        let msg_stores = self.msg_stores.clone();
        let db = self.db.clone();
        let config = self.config.clone();
        let container = self.clone();
        let analytics = self.analytics.clone();
        let publisher = self.publisher.clone();
        let feedback_pending_cleanup = self.feedback_pending_cleanup.clone();
        let running_workspaces = self.running_workspaces.clone();

        let mut process_exit_rx = self.spawn_os_exit_watcher(exec_id);

        tokio::spawn(async move {
            let mut exit_signal_future = exit_signal
                .map(|rx| rx.boxed()) // wait for result
                .unwrap_or_else(|| std::future::pending().boxed()); // no signal, stall forever

            let status_result: std::io::Result<std::process::ExitStatus>;

            // Wait for process to exit, or exit signal from executor
            tokio::select! {
                // Exit signal with result.
                // Some coding agent processes do not automatically exit after processing the user request; instead the executor
                // signals when processing has finished to gracefully kill the process.
                exit_result = &mut exit_signal_future => {
                    // Executor signaled completion: try graceful shutdown first to allow hooks to run
                    if let Some(child_lock) = child_store.read().await.get(&exec_id).cloned() {
                        // Try graceful interrupt first (allows Claude Code to run Stop hooks)
                        if let Some(interrupt_sender) = container.take_interrupt_sender(&exec_id).await {
                            let _ = interrupt_sender.send(());
                            tracing::debug!("Sent interrupt signal to process {}, waiting for graceful exit", exec_id);

                            // Wait up to 5 seconds for graceful exit
                            let graceful_exit = {
                                let mut child_guard = child_lock.write().await;
                                tokio::time::timeout(
                                    std::time::Duration::from_secs(5),
                                    child_guard.wait()
                                ).await
                            };

                            match graceful_exit {
                                Ok(Ok(_)) => {
                                    tracing::debug!("Process {} exited gracefully after interrupt", exec_id);
                                }
                                _ => {
                                    // Graceful exit failed or timed out, force kill
                                    tracing::debug!("Process {} did not exit gracefully, force killing", exec_id);
                                    let mut child = child_lock.write().await;
                                    if let Err(err) = command::kill_process_group(&mut child).await {
                                        tracing::error!("Failed to kill process group after exit signal: {} {}", exec_id, err);
                                    }
                                }
                            }
                        } else {
                            // No interrupt sender, just kill the process
                            let mut child = child_lock.write().await;
                            if let Err(err) = command::kill_process_group(&mut child).await {
                                tracing::error!("Failed to kill process group after exit signal: {} {}", exec_id, err);
                            }
                        }
                    }

                    // Map the exit result to appropriate exit status
                    status_result = match exit_result {
                        Ok(ExecutorExitResult::Success) => Ok(success_exit_status()),
                        Ok(ExecutorExitResult::Failure) => Ok(failure_exit_status()),
                        Err(_) => Ok(success_exit_status()), // Channel closed, assume success
                    };
                }
                // Process exit
                exit_status_result = &mut process_exit_rx => {
                    status_result = exit_status_result.unwrap_or_else(|e| Err(std::io::Error::other(e)));
                }
            }

            let (exit_code, status) = match status_result {
                Ok(exit_status) => {
                    let code = exit_status.code().unwrap_or(-1) as i64;
                    let status = if exit_status.success() {
                        ExecutionProcessStatus::Completed
                    } else {
                        ExecutionProcessStatus::Failed
                    };
                    (Some(code), status)
                }
                Err(_) => (None, ExecutionProcessStatus::Failed),
            };

            if !ExecutionProcess::was_stopped(&db.pool, exec_id).await
                && let Err(e) =
                    ExecutionProcess::update_completion(&db.pool, exec_id, status, exit_code).await
            {
                tracing::error!("Failed to update execution process completion: {}", e);
            }

            if let Ok(ctx) = ExecutionProcess::load_context(&db.pool, exec_id).await {
                // Emit ExecutionCompleted event for handlers
                container
                    .event_dispatcher
                    .dispatch(DomainEvent::ExecutionCompleted {
                        process: ctx.execution_process.clone(),
                    })
                    .await;

                // Update executor session summary if available
                if let Err(e) = container.update_executor_session_summary(&exec_id).await {
                    tracing::warn!("Failed to update executor session summary: {}", e);
                }

                let success = matches!(
                    ctx.execution_process.status,
                    ExecutionProcessStatus::Completed
                ) && exit_code == Some(0);

                let cleanup_done = matches!(
                    ctx.execution_process.run_reason,
                    ExecutionProcessRunReason::CleanupScript
                ) && !matches!(
                    ctx.execution_process.status,
                    ExecutionProcessStatus::Running
                );

                if success || cleanup_done {
                    // Commit changes (if any) and get feedback about whether changes were made
                    let changes_committed = match container.try_commit_changes(&ctx).await {
                        Ok(committed) => committed,
                        Err(e) => {
                            tracing::error!("Failed to commit changes after execution: {}", e);
                            // Treat commit failures as if changes were made to be safe
                            true
                        }
                    };

                    let should_start_next = if matches!(
                        ctx.execution_process.run_reason,
                        ExecutionProcessRunReason::CodingAgent
                    ) {
                        changes_committed
                    } else {
                        true
                    };

                    if should_start_next {
                        // If the process exited successfully, start the next action
                        if let Err(e) = container.try_start_next_action(&ctx).await {
                            tracing::error!("Failed to start next action after completion: {}", e);
                        }
                    } else {
                        tracing::info!(
                            "Skipping cleanup script for workspace {} - no changes made by coding agent",
                            ctx.workspace.id
                        );

                        // Manually finalize task since we're bypassing normal execution flow
                        container.finalize_task(publisher.as_ref().ok(), &ctx).await;
                    }
                }

                if container.should_finalize(&ctx) {
                    // Only execute queued messages if the execution succeeded
                    // If it failed or was killed, just clear the queue and finalize
                    let should_execute_queued = !matches!(
                        ctx.execution_process.status,
                        ExecutionProcessStatus::Failed | ExecutionProcessStatus::Killed
                    );

                    if let Some(queued_msg) =
                        container.queued_message_service.take_queued(ctx.session.id)
                    {
                        if should_execute_queued {
                            tracing::info!(
                                "Found queued message for session {}, starting follow-up execution",
                                ctx.session.id
                            );

                            // Delete the scratch since we're consuming the queued message
                            if let Err(e) = Scratch::delete(
                                &db.pool,
                                ctx.session.id,
                                &ScratchType::DraftFollowUp,
                            )
                            .await
                            {
                                tracing::warn!(
                                    "Failed to delete scratch after consuming queued message: {}",
                                    e
                                );
                            }

                            // Execute the queued follow-up
                            if let Err(e) = container
                                .start_queued_follow_up(&ctx, &queued_msg.data)
                                .await
                            {
                                tracing::error!("Failed to start queued follow-up: {}", e);
                                // Fall back to finalization if follow-up fails
                                container.finalize_task(publisher.as_ref().ok(), &ctx).await;
                            }
                        } else {
                            // Execution failed or was killed - discard the queued message and finalize
                            tracing::info!(
                                "Discarding queued message for session {} due to execution status {:?}",
                                ctx.session.id,
                                ctx.execution_process.status
                            );
                            container.finalize_task(publisher.as_ref().ok(), &ctx).await;
                        }
                    } else {
                        container.finalize_task(publisher.as_ref().ok(), &ctx).await;
                    }
                }

                // Fire analytics event when CodingAgent execution has finished
                if config.read().await.analytics_enabled
                    && matches!(
                        &ctx.execution_process.run_reason,
                        ExecutionProcessRunReason::CodingAgent
                    )
                    && let Some(analytics) = &analytics
                {
                    analytics.analytics_service.track_event(&analytics.user_id, "task_attempt_finished", Some(json!({
                        "task_id": ctx.task.id.to_string(),
                        "project_id": ctx.task.project_id.to_string(),
                        "workspace_id": ctx.workspace.id.to_string(),
                        "session_id": ctx.session.id.to_string(),
                        "execution_success": matches!(ctx.execution_process.status, ExecutionProcessStatus::Completed),
                        "exit_code": ctx.execution_process.exit_code,
                    })));
                }
            }

            // Now that commit/next-action/finalization steps for this process are complete,
            // capture the HEAD OID as the definitive "after" state (best-effort).
            container.update_after_head_commits(exec_id).await;

            // Process the execution queue in case there are waiting tasks
            // Use tokio::spawn so queue processing doesn't block cleanup
            let container_clone = container.clone();
            tokio::spawn(async move {
                if let Err(e) = container_clone.process_queue().await {
                    tracing::error!("Failed to process execution queue: {}", e);
                }
            });

            // Cleanup msg store - skip if feedback parser is pending (it will do cleanup)
            let is_feedback_pending = feedback_pending_cleanup.read().await.contains(&exec_id);
            if !is_feedback_pending {
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
                    tokio::time::sleep(Duration::from_millis(50)).await; // Wait for the finish message to propogate
                    match Arc::try_unwrap(msg_arc) {
                        Ok(inner) => drop(inner),
                        Err(arc) => tracing::error!(
                            "There are still {} strong Arcs to MsgStore for {}",
                            Arc::strong_count(&arc),
                            exec_id
                        ),
                    }
                }
            }

            // Cleanup child handle
            child_store.write().await.remove(&exec_id);

            // Cleanup protocol peer (releases stdin reference, allowing Claude to receive EOF)
            container.approvals.unregister_protocol_peer(&exec_id).await;

            // Remove workspace from running set (unless it was a dev server)
            if !matches!(run_reason, ExecutionProcessRunReason::DevServer) {
                running_workspaces.remove(&workspace_id);
            }
        })
    }

    pub fn spawn_os_exit_watcher(
        &self,
        exec_id: Uuid,
    ) -> tokio::sync::oneshot::Receiver<std::io::Result<std::process::ExitStatus>> {
        let (tx, rx) = tokio::sync::oneshot::channel::<std::io::Result<std::process::ExitStatus>>();
        let child_store = self.child_store.clone();
        tokio::spawn(async move {
            loop {
                let child_lock = {
                    let map = child_store.read().await;
                    map.get(&exec_id).cloned()
                };
                if let Some(child_lock) = child_lock {
                    let mut child_handler = child_lock.write().await;
                    match child_handler.try_wait() {
                        Ok(Some(status)) => {
                            let _ = tx.send(Ok(status));
                            break;
                        }
                        Ok(None) => {}
                        Err(e) => {
                            let _ = tx.send(Err(e));
                            break;
                        }
                    }
                } else {
                    let _ = tx.send(Err(io::Error::other(format!(
                        "Child handle missing for {exec_id}"
                    ))));
                    break;
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        });
        rx
    }

    pub fn dir_name_from_workspace(workspace_id: &Uuid, task_title: &str) -> String {
        let task_title_id = git_branch_id(task_title);
        format!("{}-{}", short_uuid(workspace_id), task_title_id)
    }

    async fn track_child_msgs_in_store(&self, id: Uuid, child: &mut AsyncGroupChild) {
        let store = Arc::new(MsgStore::new());

        let out = child.inner().stdout.take().expect("no stdout");
        let err = child.inner().stderr.take().expect("no stderr");

        // Map stdout bytes -> LogMsg::Stdout
        let out = ReaderStream::new(out)
            .map_ok(|chunk| LogMsg::Stdout(String::from_utf8_lossy(&chunk).into_owned()));

        // Map stderr bytes -> LogMsg::Stderr
        let err = ReaderStream::new(err)
            .map_ok(|chunk| LogMsg::Stderr(String::from_utf8_lossy(&chunk).into_owned()));

        // If you have a JSON Patch source, map it to LogMsg::JsonPatch too, then select all three.

        // Merge and forward into the store
        let merged = select(out, err); // Stream<Item = Result<LogMsg, io::Error>>
        store.clone().spawn_forwarder(merged);

        let mut map = self.msg_stores().write().await;
        map.insert(id, store);
    }

    /// Create a live diff log stream for ongoing attempts for WebSocket
    /// Returns a stream that owns the filesystem watcher - when dropped, watcher is cleaned up
    /// Uses shared watcher manager to avoid "too many open files" errors when multiple
    /// browser tabs connect to the same workspace.
    async fn create_live_diff_stream(
        &self,
        worktree_path: &Path,
        base_commit: &Commit,
        stats_only: bool,
        path_prefix: Option<String>,
    ) -> Result<DiffStreamHandle, ContainerError> {
        diff_stream::create(
            self.git().clone(),
            worktree_path.to_path_buf(),
            base_commit.clone(),
            stats_only,
            path_prefix,
            Some(&self.watcher_manager),
        )
        .await
        .map_err(|e| ContainerError::Other(anyhow!("{e}")))
    }

    /// Extract the last assistant message from the MsgStore history
    fn extract_last_assistant_message(&self, exec_id: &Uuid) -> Option<String> {
        // Get the MsgStore for this execution
        let msg_stores = self.msg_stores.try_read().ok()?;
        let msg_store = msg_stores.get(exec_id)?;

        // Get the history and scan in reverse for the last assistant message
        let history = msg_store.get_history();

        for msg in history.iter().rev() {
            if let LogMsg::JsonPatch(patch) = msg {
                // Try to extract a NormalizedEntry from the patch
                if let Some((_, entry)) = extract_normalized_entry_from_patch(patch)
                    && matches!(entry.entry_type, NormalizedEntryType::AssistantMessage)
                {
                    let content = entry.content.trim();
                    if !content.is_empty() {
                        const MAX_SUMMARY_LENGTH: usize = 4096;
                        if content.len() > MAX_SUMMARY_LENGTH {
                            let truncated = truncate_to_char_boundary(content, MAX_SUMMARY_LENGTH);
                            return Some(format!("{truncated}..."));
                        }
                        return Some(content.to_string());
                    }
                }
            }
        }

        None
    }

    /// Update the coding agent turn summary with the final assistant message
    async fn update_executor_session_summary(&self, exec_id: &Uuid) -> Result<(), anyhow::Error> {
        // Check if there's a coding agent turn for this execution process
        let turn = CodingAgentTurn::find_by_execution_process_id(&self.db.pool, *exec_id).await?;

        if let Some(turn) = turn {
            // Only update if summary is not already set
            if turn.summary.is_none() {
                if let Some(summary) = self.extract_last_assistant_message(exec_id) {
                    CodingAgentTurn::update_summary(&self.db.pool, *exec_id, &summary).await?;
                } else {
                    tracing::debug!("No assistant message found for execution {}", exec_id);
                }
            }
        }

        Ok(())
    }

    /// Copy project files and images to the workspace.
    /// Skips files/images that already exist (fast no-op if all exist).
    async fn copy_files_and_images(
        &self,
        workspace_dir: &Path,
        workspace: &Workspace,
    ) -> Result<(), ContainerError> {
        let repos = WorkspaceRepo::find_repos_with_copy_files(&self.db.pool, workspace.id).await?;

        for repo in &repos {
            if let Some(copy_files) = &repo.copy_files
                && !copy_files.trim().is_empty()
            {
                let worktree_path = workspace_dir.join(&repo.name);
                self.copy_project_files(&repo.path, &worktree_path, copy_files)
                    .await
                    .unwrap_or_else(|e| {
                        tracing::warn!(
                            "Failed to copy project files for repo '{}': {}",
                            repo.name,
                            e
                        );
                    });
            }
        }

        if let Err(e) = self
            .image_service
            .copy_images_by_task_to_worktree(workspace_dir, workspace.task_id)
            .await
        {
            tracing::warn!("Failed to copy task images to workspace: {}", e);
        }

        Ok(())
    }

    /// Create workspace-level CLAUDE.md and AGENTS.md files that import from each repo.
    /// Uses the @import syntax to reference each repo's config files.
    /// Skips creating files if they already exist or if no repos have the source file.
    async fn create_workspace_config_files(
        workspace_dir: &Path,
        repos: &[Repo],
    ) -> Result<(), ContainerError> {
        const CONFIG_FILES: [&str; 2] = ["CLAUDE.md", "AGENTS.md"];

        for config_file in CONFIG_FILES {
            let workspace_config_path = workspace_dir.join(config_file);

            if workspace_config_path.exists() {
                tracing::debug!(
                    "Workspace config file {} already exists, skipping",
                    config_file
                );
                continue;
            }

            let mut import_lines = Vec::new();
            for repo in repos {
                let repo_config_path = workspace_dir.join(&repo.name).join(config_file);
                if repo_config_path.exists() {
                    import_lines.push(format!("@{}/{}", repo.name, config_file));
                }
            }

            if import_lines.is_empty() {
                tracing::debug!(
                    "No repos have {}, skipping workspace config creation",
                    config_file
                );
                continue;
            }

            let content = import_lines.join("\n") + "\n";
            if let Err(e) = tokio::fs::write(&workspace_config_path, &content).await {
                tracing::warn!(
                    "Failed to create workspace config file {}: {}",
                    config_file,
                    e
                );
                continue;
            }

            tracing::info!(
                "Created workspace {} with {} import(s)",
                config_file,
                import_lines.len()
            );
        }

        Ok(())
    }

    /// Deploy Claude Code hooks to each repository worktree and the workspace root.
    ///
    /// This copies hook files from embedded assets to:
    /// 1. Each repo's `.claude/` directory (for when agent_working_dir is set)
    /// 2. The workspace root `.claude/` directory (for when agent_working_dir is not set)
    ///
    /// Also adds `.claude/` to `.git/info/exclude` to prevent git tracking.
    ///
    /// The deployment is idempotent - existing files are overwritten to ensure the latest
    /// hook versions are used. If assets don't exist, the function completes successfully
    /// (graceful degradation).
    async fn deploy_claude_code_hooks(
        workspace_dir: &Path,
        repos: &[Repo],
    ) -> Result<(), ContainerError> {
        // Check if we have any hook assets to deploy, filtering out __pycache__ files
        // (these can get embedded at compile time if they exist locally)
        let asset_files: Vec<_> = ClaudeCodeHookAssets::iter()
            .filter(|name| !name.contains("__pycache__"))
            .collect();
        tracing::info!(
            "deploy_claude_code_hooks: found {} assets: {:?}",
            asset_files.len(),
            asset_files
        );
        if asset_files.is_empty() {
            tracing::debug!("No Claude Code hook assets found, skipping deployment");
            return Ok(());
        }

        // Deploy to workspace root first (for when agent_working_dir is not set)
        let workspace_claude_dir = workspace_dir.join(".claude");
        if let Err(e) = tokio::fs::create_dir_all(&workspace_claude_dir).await {
            tracing::warn!(
                "Failed to create .claude directory at workspace root: {}",
                e
            );
        } else {
            for asset_name in &asset_files {
                if let Some(asset) = ClaudeCodeHookAssets::get(asset_name) {
                    let target_path = workspace_claude_dir.join(asset_name.as_ref());
                    if let Err(e) = tokio::fs::write(&target_path, &asset.data).await {
                        tracing::warn!(
                            "Failed to write hook file '{}' to workspace root: {}",
                            asset_name,
                            e
                        );
                    }
                }
            }
            tracing::debug!("Deployed Claude Code hooks to workspace root");
        }

        // Deploy to each repo's worktree
        for repo in repos {
            let worktree_path = workspace_dir.join(&repo.name);
            let claude_dir = worktree_path.join(".claude");

            // Create .claude directory
            if let Err(e) = tokio::fs::create_dir_all(&claude_dir).await {
                tracing::warn!(
                    "Failed to create .claude directory for repo '{}': {}",
                    repo.name,
                    e
                );
                continue;
            }

            // Copy all hook assets to .claude/
            for asset_name in &asset_files {
                if let Some(asset) = ClaudeCodeHookAssets::get(asset_name) {
                    let target_path = claude_dir.join(asset_name.as_ref());

                    if let Err(e) = tokio::fs::write(&target_path, &asset.data).await {
                        tracing::warn!(
                            "Failed to write hook file '{}' for repo '{}': {}",
                            asset_name,
                            repo.name,
                            e
                        );
                        continue;
                    }

                    tracing::debug!(
                        "Deployed hook file '{}' to repo '{}'",
                        asset_name,
                        repo.name
                    );
                }
            }

            // Add .claude/ to .git/info/exclude
            // Handle git worktrees where .git is a file pointing to the real git dir
            let dot_git_path = worktree_path.join(".git");
            let git_dir = if dot_git_path.is_file() {
                // In a worktree, .git is a file containing "gitdir: /path/to/real/.git/worktrees/..."
                match tokio::fs::read_to_string(&dot_git_path).await {
                    Ok(content) => {
                        if let Some(gitdir) = content.strip_prefix("gitdir: ") {
                            PathBuf::from(gitdir.trim())
                        } else {
                            tracing::warn!(
                                "Unexpected .git file format for repo '{}', skipping exclude",
                                repo.name
                            );
                            continue;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to read .git file for repo '{}': {}",
                            repo.name,
                            e
                        );
                        continue;
                    }
                }
            } else {
                dot_git_path
            };

            let git_info_dir = git_dir.join("info");
            let exclude_path = git_info_dir.join("exclude");

            // Create .git/info directory if it doesn't exist
            if let Err(e) = tokio::fs::create_dir_all(&git_info_dir).await {
                tracing::warn!(
                    "Failed to create .git/info directory for repo '{}': {}",
                    repo.name,
                    e
                );
                continue;
            }

            // Read existing exclude content or start fresh
            let existing_content = tokio::fs::read_to_string(&exclude_path)
                .await
                .unwrap_or_default();

            // Check if .claude/ is already excluded
            let claude_pattern = ".claude/";
            if !existing_content.lines().any(|line| line.trim() == claude_pattern) {
                // Append .claude/ to exclude file
                let new_content = if existing_content.is_empty() || existing_content.ends_with('\n')
                {
                    format!("{}{}\n", existing_content, claude_pattern)
                } else {
                    format!("{}\n{}\n", existing_content, claude_pattern)
                };

                if let Err(e) = tokio::fs::write(&exclude_path, &new_content).await {
                    tracing::warn!(
                        "Failed to update .git/info/exclude for repo '{}': {}",
                        repo.name,
                        e
                    );
                    continue;
                }

                tracing::debug!("Added .claude/ to .git/info/exclude for repo '{}'", repo.name);
            }
        }

        tracing::info!(
            "Deployed Claude Code hooks to {} repos",
            repos.len()
        );

        Ok(())
    }

    /// Spawn exit monitor for conversation executions (no workspace context needed)
    pub fn spawn_conversation_exit_monitor(
        &self,
        exec_id: &Uuid,
        exit_signal: Option<ExecutorExitSignal>,
    ) -> JoinHandle<()> {
        let exec_id = *exec_id;
        let child_store = self.child_store.clone();
        let msg_stores = self.msg_stores.clone();
        let db = self.db.clone();
        let container = self.clone();

        let mut process_exit_rx = self.spawn_os_exit_watcher(exec_id);

        tokio::spawn(async move {
            let mut exit_signal_future = exit_signal
                .map(|rx| rx.boxed())
                .unwrap_or_else(|| std::future::pending().boxed());

            let status_result: std::io::Result<std::process::ExitStatus>;

            tokio::select! {
                exit_result = &mut exit_signal_future => {
                    if let Some(child_lock) = child_store.read().await.get(&exec_id).cloned() {
                        // Try graceful interrupt first (allows hooks to run)
                        if let Some(interrupt_sender) = container.take_interrupt_sender(&exec_id).await {
                            let _ = interrupt_sender.send(());
                            tracing::debug!("Sent interrupt signal to conversation process {}, waiting for graceful exit", exec_id);

                            // Wait up to 5 seconds for graceful exit
                            let graceful_exit = {
                                let mut child_guard = child_lock.write().await;
                                tokio::time::timeout(
                                    std::time::Duration::from_secs(5),
                                    child_guard.wait()
                                ).await
                            };

                            match graceful_exit {
                                Ok(Ok(_)) => {
                                    tracing::debug!("Conversation process {} exited gracefully", exec_id);
                                }
                                _ => {
                                    let mut child = child_lock.write().await;
                                    if let Err(err) = command::kill_process_group(&mut child).await {
                                        tracing::error!("Failed to kill process group after exit signal: {} {}", exec_id, err);
                                    }
                                }
                            }
                        } else {
                            let mut child = child_lock.write().await;
                            if let Err(err) = command::kill_process_group(&mut child).await {
                                tracing::error!("Failed to kill process group after exit signal: {} {}", exec_id, err);
                            }
                        }
                    }
                    status_result = match exit_result {
                        Ok(ExecutorExitResult::Success) => Ok(success_exit_status()),
                        Ok(ExecutorExitResult::Failure) => Ok(failure_exit_status()),
                        Err(_) => Ok(success_exit_status()),
                    };
                }
                exit_status_result = &mut process_exit_rx => {
                    status_result = exit_status_result.unwrap_or_else(|e| Err(std::io::Error::other(e)));
                }
            }

            let (exit_code, status) = match status_result {
                Ok(exit_status) => {
                    let code = exit_status.code().unwrap_or(-1) as i64;
                    let status = if exit_status.success() {
                        ExecutionProcessStatus::Completed
                    } else {
                        ExecutionProcessStatus::Failed
                    };
                    (Some(code), status)
                }
                Err(_) => (None, ExecutionProcessStatus::Failed),
            };

            if !ExecutionProcess::was_stopped(&db.pool, exec_id).await
                && let Err(e) = ExecutionProcess::update_completion(
                    &db.pool,
                    exec_id,
                    status.clone(),
                    exit_code,
                )
                .await
            {
                tracing::error!("Failed to update execution process completion: {}", e);
            }

            // Cleanup msg store and extract assistant message before cleanup
            let assistant_message = if let Some(msg_arc) = msg_stores.write().await.remove(&exec_id)
            {
                // Extract assistant message before cleanup
                let assistant_content = extract_assistant_message_from_msg_store(&msg_arc);

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
                match Arc::try_unwrap(msg_arc) {
                    Ok(inner) => drop(inner),
                    Err(arc) => tracing::error!(
                        "There are still {} strong Arcs to MsgStore for {}",
                        Arc::strong_count(&arc),
                        exec_id
                    ),
                }

                assistant_content
            } else {
                None
            };

            // Cleanup child handle
            child_store.write().await.remove(&exec_id);

            // Store assistant message and send notification on successful completion
            if matches!(status, ExecutionProcessStatus::Completed)
                && let Ok(Some(execution_process)) =
                    ExecutionProcess::find_by_id(&db.pool, exec_id).await
                && let Some(conversation_session_id) = execution_process.conversation_session_id
                && let Ok(Some(conversation)) =
                    ConversationSession::find_by_id(&db.pool, conversation_session_id).await
            {
                // Store the assistant message
                if let Some(ref content) = assistant_message
                    && let Err(e) = ConversationService::add_assistant_message(
                        &db.pool,
                        conversation_session_id,
                        exec_id,
                        content.clone(),
                    )
                    .await
                {
                    tracing::error!("Failed to store assistant message: {}", e);
                }

                // Send notification
                if let Err(e) = NotificationService::notify_conversation_response(
                    &db.pool,
                    conversation.project_id,
                    conversation_session_id,
                    assistant_message.as_deref(),
                )
                .await
                {
                    tracing::error!("Failed to send conversation response notification: {}", e);
                }
            }
        })
    }

    /// Start a follow-up execution from a queued message
    async fn start_queued_follow_up(
        &self,
        ctx: &ExecutionContext,
        queued_data: &DraftFollowUpData,
    ) -> Result<ExecutionProcess, ContainerError> {
        // Get executor profile from the latest CodingAgent process in this session
        let initial_executor_profile_id =
            ExecutionProcess::latest_executor_profile_for_session(&self.db.pool, ctx.session.id)
                .await
                .map_err(|e| {
                    ContainerError::Other(anyhow!("Failed to get executor profile: {e}"))
                })?;

        let executor_profile_id = ExecutorProfileId {
            executor: initial_executor_profile_id.executor,
            variant: queued_data.variant.clone(),
        };

        // Get latest agent session ID for session continuity (from coding agent turns)
        let latest_agent_session_id = ExecutionProcess::find_latest_coding_agent_turn_session_id(
            &self.db.pool,
            ctx.session.id,
        )
        .await?;

        let project_repos =
            ProjectRepo::find_by_project_id_with_names(&self.db.pool, ctx.project.id).await?;
        let cleanup_action = self.cleanup_actions_for_repos(&project_repos);

        let working_dir = ctx
            .workspace
            .agent_working_dir
            .as_ref()
            .filter(|dir| !dir.is_empty())
            .cloned();

        let action_type = if let Some(agent_session_id) = latest_agent_session_id {
            ExecutorActionType::CodingAgentFollowUpRequest(CodingAgentFollowUpRequest {
                prompt: queued_data.message.clone(),
                session_id: agent_session_id,
                executor_profile_id: executor_profile_id.clone(),
                working_dir: working_dir.clone(),
            })
        } else {
            ExecutorActionType::CodingAgentInitialRequest(CodingAgentInitialRequest {
                prompt: queued_data.message.clone(),
                executor_profile_id: executor_profile_id.clone(),
                working_dir,
            })
        };

        let action = ExecutorAction::new(action_type, cleanup_action.map(Box::new));

        self.start_execution(
            &ctx.workspace,
            &ctx.session,
            &action,
            &ExecutionProcessRunReason::CodingAgent,
            None,
        )
        .await
    }

    /// Collect feedback from the coding agent after successful execution.
    ///
    /// This sends a follow-up prompt to the agent asking for structured feedback
    /// about the task, and spawns a background task to parse and store the response.
    ///
    /// # Arguments
    /// * `ctx` - The execution context from the completed CodingAgent execution
    /// * `agent_session_id` - The session ID to continue the conversation with
    ///
    /// # Returns
    /// The execution process for the feedback collection, or an error if starting fails
    async fn collect_agent_feedback(
        &self,
        ctx: &ExecutionContext,
        agent_session_id: &str,
    ) -> Result<ExecutionProcess, ContainerError> {
        // Get executor profile from the original CodingAgent process
        let executor_profile_id =
            ExecutionProcess::latest_executor_profile_for_session(&self.db.pool, ctx.session.id)
                .await
                .map_err(|e| {
                    ContainerError::Other(anyhow!("Failed to get executor profile: {e}"))
                })?;

        // Get working directory from workspace
        let working_dir = ctx
            .workspace
            .agent_working_dir
            .as_ref()
            .filter(|dir| !dir.is_empty())
            .cloned();

        // Create the feedback action
        let action = FeedbackService::create_feedback_action(
            agent_session_id.to_string(),
            executor_profile_id,
            working_dir,
        );

        // Start the feedback execution with InternalAgent run reason and "feedback" purpose
        let feedback_exec = self
            .start_execution(
                &ctx.workspace,
                &ctx.session,
                &action,
                &ExecutionProcessRunReason::InternalAgent,
                Some("feedback"),
            )
            .await?;

        // Mark this execution as pending feedback cleanup - exit monitor will skip msg_store cleanup
        self.feedback_pending_cleanup
            .write()
            .await
            .insert(feedback_exec.id);

        // Spawn background task to monitor and parse the feedback response
        self.spawn_feedback_parser(feedback_exec.id, ctx.task.id, ctx.workspace.id);

        Ok(feedback_exec)
    }

    /// Spawn a background task that monitors a feedback execution and parses the response.
    ///
    /// When the feedback execution completes, this task extracts the assistant message,
    /// parses it using `FeedbackService::parse_feedback_response`, and stores the result
    /// in the `agent_feedback` table.
    ///
    /// Failures are logged but don't affect task finalization.
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
                        cleanup(
                            msg_stores,
                            feedback_pending_cleanup,
                            db,
                            feedback_exec_id,
                        )
                        .await;
                        return;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to query feedback execution {}: {}",
                            feedback_exec_id,
                            e
                        );
                        cleanup(
                            msg_stores,
                            feedback_pending_cleanup,
                            db,
                            feedback_exec_id,
                        )
                        .await;
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
                        cleanup(
                            msg_stores,
                            feedback_pending_cleanup,
                            db,
                            feedback_exec_id,
                        )
                        .await;
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
                cleanup(
                    msg_stores,
                    feedback_pending_cleanup,
                    db,
                    feedback_exec_id,
                )
                .await;
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
                    cleanup(
                        msg_stores,
                        feedback_pending_cleanup,
                        db,
                        feedback_exec_id,
                    )
                    .await;
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
            cleanup(
                msg_stores,
                feedback_pending_cleanup,
                db,
                feedback_exec_id,
            )
            .await;
        });
    }

    /// Collect review attention analysis when a task moves to InReview status.
    ///
    /// This sends a follow-up prompt to the agent asking it to analyze whether
    /// its completed work needs human attention, then spawns a parser to handle
    /// the response.
    ///
    /// # Arguments
    /// * `ctx` - The execution context from the completed CodingAgent execution
    /// * `agent_session_id` - The session ID to continue the conversation with
    ///
    /// # Returns
    /// The execution process for the review attention collection, or an error if starting fails
    async fn collect_review_attention(
        &self,
        ctx: &ExecutionContext,
        agent_session_id: &str,
    ) -> Result<ExecutionProcess, ContainerError> {
        // Check if review attention is enabled in config
        let review_attention_profile = {
            let config = self.config.read().await;
            config.review_attention_executor_profile.clone()
        };

        let Some(executor_profile_id) = review_attention_profile else {
            return Err(ContainerError::Other(anyhow!(
                "Review attention is disabled (no executor profile configured)"
            )));
        };

        // Get the CodingAgentTurn to retrieve prompt and summary
        let turn = CodingAgentTurn::find_by_execution_process_id(
            &self.db.pool,
            ctx.execution_process.id,
        )
        .await
        .map_err(|e| ContainerError::Other(anyhow!("Failed to get coding agent turn: {e}")))?
        .ok_or_else(|| {
            ContainerError::Other(anyhow!(
                "No coding agent turn found for execution {}",
                ctx.execution_process.id
            ))
        })?;

        // If no summary, we can't analyze
        let summary = turn.summary.ok_or_else(|| {
            ContainerError::Other(anyhow!(
                "No summary available for execution {}, cannot analyze",
                ctx.execution_process.id
            ))
        })?;

        // If no prompt, use a default description
        let task_description = turn.prompt.unwrap_or_else(|| ctx.task.title.clone());

        // Get working directory from workspace
        let working_dir = ctx
            .workspace
            .agent_working_dir
            .as_ref()
            .filter(|dir| !dir.is_empty())
            .cloned();

        // Create the review attention action
        let action = ReviewAttentionService::create_review_attention_action(
            agent_session_id.to_string(),
            executor_profile_id,
            working_dir,
            &task_description,
            &summary,
        );

        // Start the execution with InternalAgent run reason and "review_attention" purpose
        let review_exec = self
            .start_execution(
                &ctx.workspace,
                &ctx.session,
                &action,
                &ExecutionProcessRunReason::InternalAgent,
                Some("review_attention"),
            )
            .await?;

        // Mark this execution as pending cleanup - exit monitor will skip msg_store cleanup
        self.feedback_pending_cleanup
            .write()
            .await
            .insert(review_exec.id);

        // Spawn background task to monitor and parse the review attention response
        self.spawn_review_attention_parser(review_exec.id, ctx.task.id, ctx.workspace.id);

        Ok(review_exec)
    }

    /// Spawn a background task that monitors a review attention execution and parses the response.
    ///
    /// When the execution completes, this task extracts the assistant message,
    /// parses it using `ReviewAttentionService::parse_review_attention_response`,
    /// creates a `ReviewAttention` record, and updates `Task.needs_attention`.
    ///
    /// Failures are logged but don't affect task finalization.
    fn spawn_review_attention_parser(
        &self,
        review_exec_id: Uuid,
        task_id: Uuid,
        workspace_id: Uuid,
    ) {
        let db = self.db.clone();
        let msg_stores = self.msg_stores.clone();
        let feedback_pending_cleanup = self.feedback_pending_cleanup.clone();

        // Clone services needed for autopilot merge
        let git = self.git.clone();
        let config = self.config.clone();
        let merge_queue_store = self.merge_queue_store.clone();
        let operation_status = self.operation_status.clone();
        // Clone the container for AI commit message generation
        let container = self.clone();

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

            // Wait for the review attention execution to complete
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;

                let exec = match ExecutionProcess::find_by_id(&db.pool, review_exec_id).await {
                    Ok(Some(exec)) => exec,
                    Ok(None) => {
                        tracing::warn!(
                            "Review attention execution {} not found, stopping parser",
                            review_exec_id
                        );
                        cleanup(
                            msg_stores,
                            feedback_pending_cleanup,
                            db,
                            review_exec_id,
                        )
                        .await;
                        return;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to query review attention execution {}: {}",
                            review_exec_id,
                            e
                        );
                        cleanup(
                            msg_stores,
                            feedback_pending_cleanup,
                            db,
                            review_exec_id,
                        )
                        .await;
                        return;
                    }
                };

                match exec.status {
                    ExecutionProcessStatus::Running => continue,
                    ExecutionProcessStatus::Completed => break,
                    ExecutionProcessStatus::Failed | ExecutionProcessStatus::Killed => {
                        tracing::warn!(
                            "Review attention execution {} ended with status {:?}, skipping parsing",
                            review_exec_id,
                            exec.status
                        );
                        cleanup(
                            msg_stores,
                            feedback_pending_cleanup,
                            db,
                            review_exec_id,
                        )
                        .await;
                        return;
                    }
                }
            }

            // Extract the assistant message from MsgStore BEFORE cleanup
            let assistant_message = {
                let stores = msg_stores.read().await;
                if let Some(store) = stores.get(&review_exec_id) {
                    extract_assistant_message_from_msg_store(store)
                } else {
                    None
                }
            };

            let Some(message) = assistant_message else {
                tracing::warn!(
                    "No assistant message found for review attention execution {}",
                    review_exec_id
                );
                cleanup(
                    msg_stores,
                    feedback_pending_cleanup,
                    db,
                    review_exec_id,
                )
                .await;
                return;
            };

            // Parse the review attention response
            let result = match ReviewAttentionService::parse_review_attention_response(&message) {
                Ok(result) => result,
                Err(e) => {
                    tracing::warn!(
                        "Failed to parse review attention response for execution {}: {}",
                        review_exec_id,
                        e
                    );
                    cleanup(
                        msg_stores,
                        feedback_pending_cleanup,
                        db,
                        review_exec_id,
                    )
                    .await;
                    return;
                }
            };

            // Store the ReviewAttention record
            let create_data = CreateReviewAttention {
                execution_process_id: review_exec_id,
                task_id,
                workspace_id,
                needs_attention: result.needs_attention,
                reasoning: result.reasoning.clone(),
            };

            match ReviewAttention::create(&db.pool, &create_data, Uuid::new_v4()).await {
                Ok(review_attention) => {
                    tracing::info!(
                        "Stored review attention {} for task {} (needs_attention: {})",
                        review_attention.id,
                        task_id,
                        result.needs_attention
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to store review attention for task {}: {}",
                        task_id,
                        e
                    );
                }
            }

            // Update Task.needs_attention field
            let update_succeeded = match Task::update_needs_attention(&db.pool, task_id, Some(result.needs_attention)).await {
                Ok(_) => {
                    tracing::info!(
                        "Updated task {} needs_attention to {}",
                        task_id,
                        result.needs_attention
                    );
                    true
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to update task {} needs_attention to {}: {}",
                        task_id,
                        result.needs_attention,
                        e
                    );
                    false
                }
            };

            // Trigger autopilot merge if update succeeded and task doesn't need attention
            if update_succeeded && !result.needs_attention {
                // Inline the autopilot merge logic to avoid borrowing issues
                let autopilot_enabled = {
                    let config = config.read().await;
                    config.autopilot_enabled
                };

                if autopilot_enabled {
                    tracing::info!(
                        task_id = %task_id,
                        workspace_id = %workspace_id,
                        "Autopilot: task doesn't need attention, triggering merge"
                    );

                    // Spawn autopilot merge in a separate task
                    let db_clone = db.clone();
                    let git_clone = git.clone();
                    let merge_queue_store_clone = merge_queue_store.clone();
                    let operation_status_clone = operation_status.clone();
                    let config_clone = config.clone();
                    let container_clone = container.clone();

                    tokio::spawn(async move {
                        // Get the merge_queue_store
                        let merge_queue_store = {
                            let guard = merge_queue_store_clone.read().await;
                            match guard.clone() {
                                Some(store) => store,
                                None => {
                                    tracing::warn!(
                                        task_id = %task_id,
                                        "Autopilot merge skipped: merge_queue_store not set"
                                    );
                                    return;
                                }
                            }
                        };

                        // Get the operation_status
                        let operation_status = {
                            let guard = operation_status_clone.read().await;
                            match guard.clone() {
                                Some(status) => status,
                                None => {
                                    tracing::warn!(
                                        task_id = %task_id,
                                        "Autopilot merge skipped: operation_status not set"
                                    );
                                    return;
                                }
                            }
                        };

                        // Load task
                        let task = match Task::find_by_id(&db_clone.pool, task_id).await {
                            Ok(Some(t)) => t,
                            Ok(None) => {
                                tracing::warn!(
                                    task_id = %task_id,
                                    "Autopilot merge skipped: task not found"
                                );
                                return;
                            }
                            Err(e) => {
                                tracing::warn!(
                                    task_id = %task_id,
                                    error = %e,
                                    "Autopilot merge skipped: failed to load task"
                                );
                                return;
                            }
                        };

                        // Load workspace
                        let workspace = match Workspace::find_by_id(&db_clone.pool, workspace_id).await {
                            Ok(Some(w)) => w,
                            Ok(None) => {
                                tracing::warn!(
                                    task_id = %task_id,
                                    workspace_id = %workspace_id,
                                    "Autopilot merge skipped: workspace not found"
                                );
                                return;
                            }
                            Err(e) => {
                                tracing::warn!(
                                    task_id = %task_id,
                                    workspace_id = %workspace_id,
                                    error = %e,
                                    "Autopilot merge skipped: failed to load workspace"
                                );
                                return;
                            }
                        };

                        // Find workspace repos
                        let workspace_repos = match WorkspaceRepo::find_by_workspace_id(&db_clone.pool, workspace_id).await {
                            Ok(repos) if !repos.is_empty() => repos,
                            Ok(_) => {
                                tracing::warn!(
                                    task_id = %task_id,
                                    workspace_id = %workspace_id,
                                    "Autopilot merge skipped: no workspace repos found"
                                );
                                return;
                            }
                            Err(e) => {
                                tracing::warn!(
                                    task_id = %task_id,
                                    error = %e,
                                    "Autopilot merge skipped: failed to load workspace repos"
                                );
                                return;
                            }
                        };

                        // Build fallback commit message from task title and description
                        let fallback_commit_message = {
                            let mut msg = task.title.clone();
                            if let Some(description) = &task.description {
                                let trimmed = description.trim();
                                if !trimmed.is_empty() {
                                    msg.push_str("\n\n");
                                    msg.push_str(trimmed);
                                }
                            }
                            msg
                        };

                        // Enqueue each repo for merge
                        let project_id = task.project_id;
                        for workspace_repo in &workspace_repos {
                            // Load the repo for AI commit message generation
                            let repo = match Repo::find_by_id(&db_clone.pool, workspace_repo.repo_id).await {
                                Ok(Some(r)) => r,
                                Ok(None) => {
                                    tracing::warn!(
                                        task_id = %task_id,
                                        repo_id = %workspace_repo.repo_id,
                                        "Autopilot: repo not found, using fallback commit message"
                                    );
                                    merge_queue_store.enqueue(
                                        project_id,
                                        workspace_id,
                                        workspace_repo.repo_id,
                                        fallback_commit_message.clone(),
                                    );
                                    continue;
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        task_id = %task_id,
                                        repo_id = %workspace_repo.repo_id,
                                        error = %e,
                                        "Autopilot: failed to load repo, using fallback commit message"
                                    );
                                    merge_queue_store.enqueue(
                                        project_id,
                                        workspace_id,
                                        workspace_repo.repo_id,
                                        fallback_commit_message.clone(),
                                    );
                                    continue;
                                }
                            };

                            // Try AI commit message generation, fall back if it fails or is disabled
                            let commit_message = match container_clone
                                .generate_autopilot_commit_message(
                                    &task,
                                    &workspace,
                                    &repo,
                                    workspace_repo,
                                    &operation_status,
                                )
                                .await
                            {
                                Ok(Some(ai_message)) => ai_message,
                                Ok(None) => {
                                    // AI generation disabled or returned no result, use fallback
                                    fallback_commit_message.clone()
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        task_id = %task_id,
                                        repo_id = %workspace_repo.repo_id,
                                        error = %e,
                                        "Autopilot: commit message generation failed, using fallback"
                                    );
                                    fallback_commit_message.clone()
                                }
                            };

                            tracing::info!(
                                task_id = %task_id,
                                workspace_id = %workspace_id,
                                repo_id = %workspace_repo.repo_id,
                                "Autopilot: enqueueing task for merge"
                            );

                            merge_queue_store.enqueue(
                                project_id,
                                workspace_id,
                                workspace_repo.repo_id,
                                commit_message,
                            );
                        }

                        // Spawn the merge queue processor if not already running
                        let should_spawn = {
                            let mut active = match ACTIVE_MERGE_PROCESSORS.lock() {
                                Ok(guard) => guard,
                                Err(poisoned) => {
                                    tracing::error!("ACTIVE_MERGE_PROCESSORS mutex poisoned");
                                    poisoned.into_inner()
                                }
                            };
                            if active.contains(&project_id) {
                                false
                            } else {
                                active.insert(project_id);
                                true
                            }
                        };

                        if should_spawn {
                            tracing::info!(
                                project_id = %project_id,
                                "Autopilot: spawning merge queue processor"
                            );

                            let processor_pool = db_clone.pool.clone();
                            let processor_git = git_clone.clone();
                            let processor_store = merge_queue_store.clone();
                            let processor_op_status = operation_status.clone();
                            let processor_config = config_clone.clone();

                            tokio::spawn(async move {
                                let processor = MergeQueueProcessor::with_operation_status(
                                    processor_pool,
                                    processor_git,
                                    processor_store,
                                    processor_op_status,
                                    processor_config,
                                );
                                if let Err(e) = processor.process_project_queue(project_id).await {
                                    tracing::error!(
                                        %project_id,
                                        error = %e,
                                        "Autopilot: failed to process merge queue"
                                    );
                                }
                                // Always remove from active set when done
                                if let Ok(mut active) = ACTIVE_MERGE_PROCESSORS.lock() {
                                    active.remove(&project_id);
                                }
                            });
                        } else {
                            tracing::debug!(
                                project_id = %project_id,
                                "Autopilot: merge queue processor already running for project"
                            );
                        }
                    });
                } else {
                    tracing::debug!(
                        task_id = %task_id,
                        "Autopilot merge skipped: autopilot is disabled"
                    );
                }
            }

            // Final cleanup after successful processing
            cleanup(
                msg_stores,
                feedback_pending_cleanup,
                db,
                review_exec_id,
            )
            .await;
        });
    }

    /// Generate a commit message using AI for autopilot merge.
    ///
    /// This method:
    /// 1. Sets OperationStatus to GeneratingCommit
    /// 2. Gets the diff between task branch and target branch
    /// 3. Starts an AI execution to generate the commit message
    /// 4. Waits for completion and extracts the message
    /// 5. Clears OperationStatus
    ///
    /// Returns Ok(Some(message)) if AI generation succeeds,
    /// Ok(None) if generation is disabled or fails (caller should use fallback),
    /// Err only for critical errors that should abort the autopilot flow.
    pub async fn generate_autopilot_commit_message(
        &self,
        task: &Task,
        workspace: &Workspace,
        repo: &Repo,
        workspace_repo: &WorkspaceRepo,
        operation_status: &OperationStatusStore,
    ) -> Result<Option<String>, ContainerError> {
        // Check if commit message auto-generation is enabled
        let (auto_generate_enabled, prompt_template, executor_profile_from_config) = {
            let config = self.config.read().await;
            (
                config.commit_message_auto_generate_enabled,
                config
                    .commit_message_prompt
                    .clone()
                    .unwrap_or_else(|| DEFAULT_COMMIT_MESSAGE_PROMPT.to_string()),
                config.commit_message_executor_profile.clone(),
            )
        };

        if !auto_generate_enabled {
            tracing::debug!(
                task_id = %task.id,
                "Commit message auto-generation disabled, will use fallback"
            );
            return Ok(None);
        }

        // Set operation status to GeneratingCommit
        operation_status.set(OperationStatus::new(
            workspace.id,
            task.id,
            OperationStatusType::GeneratingCommit,
        ));

        let result = self
            .generate_autopilot_commit_message_inner(
                task,
                workspace,
                repo,
                workspace_repo,
                &prompt_template,
                executor_profile_from_config,
            )
            .await;

        // Clear operation status after completion (success or failure)
        operation_status.clear(workspace.id);

        result
    }

    /// Inner implementation of commit message generation.
    /// Separated to ensure operation_status is always cleared via the wrapper.
    async fn generate_autopilot_commit_message_inner(
        &self,
        task: &Task,
        workspace: &Workspace,
        repo: &Repo,
        workspace_repo: &WorkspaceRepo,
        prompt_template: &str,
        executor_profile_from_config: Option<executors::profile::ExecutorProfileId>,
    ) -> Result<Option<String>, ContainerError> {
        let repo_path = PathBuf::from(&repo.path);

        // Get diff between task branch and base branch
        let diffs = match self.git.get_diffs(
            DiffTarget::Branch {
                repo_path: &repo_path,
                branch_name: &workspace.branch,
                base_branch: &workspace_repo.target_branch,
            },
            None,
        ) {
            Ok(diffs) => diffs,
            Err(e) => {
                tracing::warn!(
                    task_id = %task.id,
                    error = %e,
                    "Failed to get diffs for commit message generation, using fallback"
                );
                return Ok(None);
            }
        };

        // Convert diffs to a unified diff string
        let diff_string = diffs
            .iter()
            .filter_map(|diff| {
                let file_path = diff.new_path.as_ref().or(diff.old_path.as_ref())?.as_str();
                let old_content = diff.old_content.as_deref().unwrap_or("");
                let new_content = diff.new_content.as_deref().unwrap_or("");

                // Skip if content was omitted (too large)
                if diff.content_omitted {
                    return Some(format!(
                        "--- a/{file_path}\n+++ b/{file_path}\n[Content too large, omitted]\n"
                    ));
                }

                Some(create_unified_diff(file_path, old_content, new_content))
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Build the prompt with task context
        let task_description = task
            .description
            .as_deref()
            .unwrap_or("No description provided");
        let prompt = prompt_template
            .replace("{task_title}", &task.title)
            .replace("{task_description}", task_description)
            .replace("{diff}", &diff_string);

        // Get or create a session for this operation
        let session = match Session::find_latest_by_workspace_id(&self.db.pool, workspace.id).await
        {
            Ok(Some(s)) => s,
            Ok(None) => {
                Session::create(
                    &self.db.pool,
                    &CreateSession { executor: None },
                    Uuid::new_v4(),
                    workspace.id,
                )
                .await?
            }
            Err(e) => {
                tracing::warn!(
                    task_id = %task.id,
                    error = %e,
                    "Failed to find/create session for commit message generation"
                );
                return Ok(None);
            }
        };

        // Determine executor profile: config override > latest from session > fallback to default
        let executor_profile_id = if let Some(profile) = executor_profile_from_config {
            profile
        } else {
            match ExecutionProcess::latest_executor_profile_for_session(&self.db.pool, session.id)
                .await
            {
                Ok(profile) => profile,
                Err(e) => {
                    tracing::warn!(
                        task_id = %task.id,
                        error = %e,
                        "Failed to get executor profile for commit message generation, using fallback"
                    );
                    return Ok(None);
                }
            }
        };

        // Get latest agent session ID for the SAME executor type
        let latest_agent_session_id =
            ExecutionProcess::find_latest_coding_agent_turn_session_id_by_executor(
                &self.db.pool,
                session.id,
                &executor_profile_id,
            )
            .await
            .ok()
            .flatten();

        let working_dir = workspace
            .agent_working_dir
            .as_ref()
            .filter(|dir| !dir.is_empty())
            .cloned();

        // Build the action type (follow-up if session exists with same executor, otherwise initial)
        let action_type = if let Some(agent_session_id) = latest_agent_session_id {
            ExecutorActionType::CodingAgentFollowUpRequest(CodingAgentFollowUpRequest {
                prompt,
                session_id: agent_session_id,
                executor_profile_id: executor_profile_id.clone(),
                working_dir,
            })
        } else {
            ExecutorActionType::CodingAgentInitialRequest(CodingAgentInitialRequest {
                prompt,
                executor_profile_id: executor_profile_id.clone(),
                working_dir,
            })
        };

        let action = ExecutorAction::new(action_type, None);

        // Start the execution
        let execution_process = match self
            .start_execution(
                workspace,
                &session,
                &action,
                &ExecutionProcessRunReason::InternalAgent,
                Some("merge_message"),
            )
            .await
        {
            Ok(ep) => ep,
            Err(e) => {
                tracing::warn!(
                    task_id = %task.id,
                    error = %e,
                    "Failed to start commit message generation execution, using fallback"
                );
                return Ok(None);
            }
        };

        // Wait for the agent to complete (60s timeout)
        if let Err(e) = self
            .wait_for_execution_completion(execution_process.id, Duration::from_secs(60))
            .await
        {
            tracing::warn!(
                task_id = %task.id,
                execution_id = %execution_process.id,
                error = %e,
                "Commit message generation execution failed/timed out, using fallback"
            );
            return Ok(None);
        }

        // Fetch all normalized entries for this execution
        let entries =
            match ExecutionProcessNormalizedEntry::fetch_all_for_execution(
                &self.db.pool,
                execution_process.id,
            )
            .await
            {
                Ok(entries) => entries,
                Err(e) => {
                    tracing::warn!(
                        task_id = %task.id,
                        execution_id = %execution_process.id,
                        error = %e,
                        "Failed to fetch commit message generation output, using fallback"
                    );
                    return Ok(None);
                }
            };

        // Find the last AssistantMessage entry and extract its content
        let commit_message = entries
            .iter()
            .rev()
            .find(|e| matches!(e.entry.entry_type, NormalizedEntryType::AssistantMessage))
            .map(|e| e.entry.content.trim().to_string())
            .filter(|s: &String| !s.is_empty());

        match commit_message {
            Some(msg) => {
                tracing::info!(
                    task_id = %task.id,
                    "Generated AI commit message for autopilot merge"
                );
                Ok(Some(msg))
            }
            None => {
                tracing::warn!(
                    task_id = %task.id,
                    execution_id = %execution_process.id,
                    "Agent did not produce a commit message, using fallback"
                );
                Ok(None)
            }
        }
    }
}

fn failure_exit_status() -> std::process::ExitStatus {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        ExitStatusExt::from_raw(256) // Exit code 1 (shifted by 8 bits)
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::ExitStatusExt;
        ExitStatusExt::from_raw(1)
    }
}

#[async_trait]
impl ContainerService for LocalContainerService {
    fn msg_stores(&self) -> &Arc<RwLock<HashMap<Uuid, Arc<MsgStore>>>> {
        &self.msg_stores
    }

    fn db(&self) -> &DBService {
        &self.db
    }

    fn git(&self) -> &GitService {
        &self.git
    }

    fn share_publisher(&self) -> Option<&SharePublisher> {
        self.publisher.as_ref().ok()
    }

    fn notification_service(&self) -> &NotificationService {
        &self.notification_service
    }

    fn watcher_manager(&self) -> Option<&WatcherManager> {
        Some(&self.watcher_manager)
    }

    fn config(&self) -> &Arc<RwLock<Config>> {
        &self.config
    }

    fn skills_cache(&self) -> &GlobalSkillsCache {
        &self.skills_cache
    }

    async fn git_branch_prefix(&self) -> String {
        self.config.read().await.git_branch_prefix.clone()
    }

    fn workspace_to_current_dir(&self, workspace: &Workspace) -> PathBuf {
        PathBuf::from(workspace.container_ref.clone().unwrap_or_default())
    }

    async fn try_collect_feedback_for_workspace(
        &self,
        workspace_id: Uuid,
    ) -> Result<(), ContainerError> {
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

        // Find the latest session for this workspace
        let session = match Session::find_latest_by_workspace_id(&self.db.pool, workspace_id).await
        {
            Ok(Some(s)) => s,
            Ok(None) => {
                tracing::debug!(
                    "No session found for workspace {}, skipping feedback",
                    workspace_id
                );
                return Ok(());
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to query session for workspace {}: {}",
                    workspace_id,
                    e
                );
                return Ok(());
            }
        };

        // Find the latest agent_session_id for this session
        let agent_session_id = match ExecutionProcess::find_latest_coding_agent_turn_session_id(
            &self.db.pool,
            session.id,
        )
        .await
        {
            Ok(Some(id)) => id,
            Ok(None) => {
                tracing::debug!(
                    "No agent session ID found for session {}, skipping feedback",
                    session.id
                );
                return Ok(());
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to query agent session ID for session {}: {}",
                    session.id,
                    e
                );
                return Ok(());
            }
        };

        // Find the latest CodingAgent execution process to load context from
        let latest_exec = match ExecutionProcess::find_latest_by_session_and_run_reason(
            &self.db.pool,
            session.id,
            &ExecutionProcessRunReason::CodingAgent,
        )
        .await
        {
            Ok(Some(exec)) => exec,
            Ok(None) => {
                tracing::debug!(
                    "No CodingAgent execution found for session {}, skipping feedback",
                    session.id
                );
                return Ok(());
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to find latest execution for session {}: {}",
                    session.id,
                    e
                );
                return Ok(());
            }
        };

        // Load full execution context
        let ctx = match ExecutionProcess::load_context(&self.db.pool, latest_exec.id).await {
            Ok(ctx) => ctx,
            Err(e) => {
                tracing::warn!(
                    "Failed to load execution context for exec {}: {}",
                    latest_exec.id,
                    e
                );
                return Ok(());
            }
        };

        // Spawn feedback collection as a non-blocking background task
        let feedback_container = self.clone();
        tokio::spawn(async move {
            if let Err(e) = feedback_container
                .collect_agent_feedback(&ctx, &agent_session_id)
                .await
            {
                tracing::warn!(
                    "Failed to start feedback collection for workspace {}: {}",
                    workspace_id,
                    e
                );
            }
        });

        Ok(())
    }

    /// Finalize task execution by updating status to InReview and emitting events.
    ///
    /// Notifications and remote sync are handled by domain event handlers:
    /// - `NotificationHandler` sends OS and in-app notifications via `ExecutionCompleted` event
    /// - `RemoteSyncHandler` syncs to remote via `TaskStatusChanged` event
    async fn finalize_task(
        &self,
        _share_publisher: Option<&SharePublisher>,
        ctx: &ExecutionContext,
    ) {
        let previous_status = ctx.task.status.clone();

        match Task::update_status(&self.db.pool, ctx.task.id, TaskStatus::InReview).await {
            Ok(_) => {
                // Emit TaskStatusChanged event for handlers (remote sync, websocket broadcast)
                let mut updated_task = ctx.task.clone();
                updated_task.status = TaskStatus::InReview;

                self.event_dispatcher
                    .dispatch(DomainEvent::TaskStatusChanged {
                        task: updated_task,
                        previous_status,
                    })
                    .await;
            }
            Err(e) => {
                tracing::error!("Failed to update task status to InReview: {e}");
            }
        }

        // Note: Notifications are now handled by NotificationHandler via ExecutionCompleted event
        // (emitted earlier in spawn_exit_monitor). No duplicate notification logic needed here.
    }

    async fn create(&self, workspace: &Workspace) -> Result<ContainerRef, ContainerError> {
        let task = workspace
            .parent_task(&self.db.pool)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;

        let workspace_dir_name =
            LocalContainerService::dir_name_from_workspace(&workspace.id, &task.title);
        let workspace_dir = WorkspaceManager::get_workspace_base_dir().join(&workspace_dir_name);

        let workspace_repos =
            WorkspaceRepo::find_by_workspace_id(&self.db.pool, workspace.id).await?;
        if workspace_repos.is_empty() {
            return Err(ContainerError::Other(anyhow!(
                "Workspace has no repositories configured"
            )));
        }

        let repositories =
            WorkspaceRepo::find_repos_for_workspace(&self.db.pool, workspace.id).await?;

        let target_branches: HashMap<_, _> = workspace_repos
            .iter()
            .map(|wr| (wr.repo_id, wr.target_branch.clone()))
            .collect();

        let workspace_inputs: Vec<RepoWorkspaceInput> = repositories
            .iter()
            .map(|repo| {
                let target_branch = target_branches.get(&repo.id).cloned().unwrap_or_default();
                RepoWorkspaceInput::new(repo.clone(), target_branch)
            })
            .collect();

        let created_workspace = WorkspaceManager::create_workspace(
            &workspace_dir,
            &workspace_inputs,
            &workspace.branch,
        )
        .await?;

        // Copy project files and images to workspace
        self.copy_files_and_images(&created_workspace.workspace_dir, workspace)
            .await?;

        Self::create_workspace_config_files(&created_workspace.workspace_dir, &repositories)
            .await?;

        // Deploy Claude Code hooks (idempotent, graceful if assets don't exist)
        Self::deploy_claude_code_hooks(&created_workspace.workspace_dir, &repositories).await?;

        Workspace::update_container_ref(
            &self.db.pool,
            workspace.id,
            &created_workspace.workspace_dir.to_string_lossy(),
        )
        .await?;

        Ok(created_workspace
            .workspace_dir
            .to_string_lossy()
            .to_string())
    }

    async fn delete(&self, workspace: &Workspace) -> Result<(), ContainerError> {
        self.try_stop(workspace, true).await;
        Self::cleanup_workspace(&self.db, workspace).await;
        Ok(())
    }

    async fn ensure_container_exists(
        &self,
        workspace: &Workspace,
    ) -> Result<ContainerRef, ContainerError> {
        let repositories =
            WorkspaceRepo::find_repos_for_workspace(&self.db.pool, workspace.id).await?;

        if repositories.is_empty() {
            return Err(ContainerError::Other(anyhow!(
                "Workspace has no repositories configured"
            )));
        }

        let workspace_dir = if let Some(container_ref) = &workspace.container_ref {
            PathBuf::from(container_ref)
        } else {
            let task = workspace
                .parent_task(&self.db.pool)
                .await?
                .ok_or(sqlx::Error::RowNotFound)?;
            let workspace_dir_name =
                LocalContainerService::dir_name_from_workspace(&workspace.id, &task.title);
            WorkspaceManager::get_workspace_base_dir().join(&workspace_dir_name)
        };

        WorkspaceManager::ensure_workspace_exists(&workspace_dir, &repositories, &workspace.branch)
            .await?;

        if workspace.container_ref.is_none() {
            Workspace::update_container_ref(
                &self.db.pool,
                workspace.id,
                &workspace_dir.to_string_lossy(),
            )
            .await?;
        }

        // Copy project files and images (fast no-op if already exist)
        self.copy_files_and_images(&workspace_dir, workspace)
            .await?;

        Self::create_workspace_config_files(&workspace_dir, &repositories).await?;

        // Deploy Claude Code hooks (idempotent, graceful if assets don't exist)
        Self::deploy_claude_code_hooks(&workspace_dir, &repositories).await?;

        Ok(workspace_dir.to_string_lossy().to_string())
    }

    async fn is_container_clean(&self, workspace: &Workspace) -> Result<bool, ContainerError> {
        let Some(container_ref) = &workspace.container_ref else {
            return Ok(true);
        };

        let workspace_dir = PathBuf::from(container_ref);
        if !workspace_dir.exists() {
            return Ok(true);
        }

        let repositories =
            WorkspaceRepo::find_repos_for_workspace(&self.db.pool, workspace.id).await?;

        for repo in &repositories {
            let worktree_path = workspace_dir.join(&repo.name);
            if worktree_path.exists() && !self.git().is_worktree_clean(&worktree_path)? {
                return Ok(false);
            }
        }

        Ok(true)
    }

    async fn start_execution_inner(
        &self,
        workspace: &Workspace,
        execution_process: &ExecutionProcess,
        executor_action: &ExecutorAction,
        purpose: &str,
    ) -> Result<(), ContainerError> {
        // Guard against duplicate agent spawns for the same workspace.
        // Dev servers are exempt - they're allowed to run concurrently with agents.
        if !matches!(
            execution_process.run_reason,
            ExecutionProcessRunReason::DevServer
        ) {
            if !self.running_workspaces.insert(workspace.id) {
                // Workspace ID was already present - another agent is running
                return Err(ContainerError::WorkspaceAlreadyRunning(workspace.id));
            }
        }

        // Get the worktree path
        let container_ref = workspace
            .container_ref
            .as_ref()
            .ok_or(ContainerError::Other(anyhow!(
                "Container ref not found for workspace"
            )))?;
        let current_dir = PathBuf::from(container_ref);

        let approvals_service: Arc<dyn ExecutorApprovalService> =
            match executor_action.base_executor() {
                Some(
                    BaseCodingAgent::Codex
                    | BaseCodingAgent::ClaudeCode
                    | BaseCodingAgent::Gemini
                    | BaseCodingAgent::QwenCode
                    | BaseCodingAgent::Opencode,
                ) => ExecutorApprovalBridge::new(
                    self.approvals.clone(),
                    self.db.clone(),
                    self.notification_service.clone(),
                    execution_process.id,
                ),
                _ => Arc::new(NoopExecutorApprovalService {}),
            };

        // Build ExecutionEnv with VK_* variables
        let mut env = ExecutionEnv::new();

        // Load task and project context for environment variables
        let task = workspace
            .parent_task(&self.db.pool)
            .await?
            .ok_or(ContainerError::Other(anyhow!(
                "Task not found for workspace"
            )))?;
        let project = task
            .parent_project(&self.db.pool)
            .await?
            .ok_or(ContainerError::Other(anyhow!("Project not found for task")))?;

        env.insert("VK_PROJECT_NAME", &project.name);
        env.insert("VK_PROJECT_ID", project.id.to_string());
        env.insert("VK_TASK_ID", task.id.to_string());
        env.insert("VK_WORKSPACE_ID", workspace.id.to_string());
        env.insert("VK_WORKSPACE_BRANCH", &workspace.branch);
        env.insert("VK_EXECUTION_PURPOSE", purpose);

        // Add repo names for observability (comma-separated list)
        let workspace_repos = WorkspaceRepo::find_repos_for_workspace(&self.db.pool, workspace.id)
            .await
            .unwrap_or_default();
        let repo_names: Vec<&str> = workspace_repos.iter().map(|r| r.name.as_str()).collect();
        env.insert("VK_REPO_NAMES", repo_names.join(","));

        // Inject Langfuse credentials if enabled (for executors with hook support)
        {
            let config_guard = self.config.read().await;
            if config_guard.langfuse_enabled {
                env.insert("TRACE_TO_LANGFUSE", "true");
                if let Some(ref key) = config_guard.langfuse_public_key {
                    env.insert("LANGFUSE_PUBLIC_KEY", key);
                }
                if let Some(ref key) = config_guard.langfuse_secret_key {
                    env.insert("LANGFUSE_SECRET_KEY", key);
                }
                if let Some(ref host) = config_guard.langfuse_host {
                    env.insert("LANGFUSE_HOST", host);
                }
            }
        }

        // Create the child and stream, add to execution tracker with timeout
        let mut spawned = tokio::time::timeout(
            Duration::from_secs(30),
            executor_action.spawn(&current_dir, approvals_service, &env),
        )
        .await
        .map_err(|_| {
            ContainerError::Other(anyhow!(
                "Timeout: process took more than 30 seconds to start"
            ))
        })??;

        self.track_child_msgs_in_store(execution_process.id, &mut spawned.child)
            .await;

        self.add_child_to_store(execution_process.id, spawned.child)
            .await;

        // Store interrupt sender for graceful shutdown
        if let Some(interrupt_sender) = spawned.interrupt_sender {
            self.add_interrupt_sender(execution_process.id, interrupt_sender)
                .await;
        }

        // Spawn unified exit monitor: watches OS exit and optional executor signal
        let _hn = self.spawn_exit_monitor(
            &execution_process.id,
            spawned.exit_signal,
            workspace.id,
            execution_process.run_reason.clone(),
        );

        Ok(())
    }

    async fn stop_execution(
        &self,
        execution_process: &ExecutionProcess,
        status: ExecutionProcessStatus,
    ) -> Result<(), ContainerError> {
        let exit_code = if status == ExecutionProcessStatus::Completed {
            Some(0)
        } else {
            None
        };

        // Try to get the child process from the in-memory store
        let child = self.get_child_from_store(&execution_process.id).await;

        // If child not found but process is still marked as running in DB,
        // this is an orphaned process (e.g., server crashed). Just update DB status.
        if child.is_none() {
            tracing::info!(
                "Child process not found in store for execution {}, marking as orphaned and updating DB status",
                execution_process.id
            );
            ExecutionProcess::update_completion(
                &self.db.pool,
                execution_process.id,
                status,
                exit_code,
            )
            .await?;

            // Clean up any stale msg_store entry
            if let Some(msg) = self.msg_stores.write().await.remove(&execution_process.id) {
                msg.push_finished();
            }

            // Update task status to InReview for orphaned processes too
            if let Ok(ctx) =
                ExecutionProcess::load_context(&self.db.pool, execution_process.id).await
                && !matches!(
                    ctx.execution_process.run_reason,
                    ExecutionProcessRunReason::DevServer
                )
            {
                if let Err(e) =
                    Task::update_status(&self.db.pool, ctx.task.id, TaskStatus::InReview).await
                {
                    tracing::error!("Failed to update task status to InReview: {e}");
                } else if let Some(publisher) = self.share_publisher()
                    && let Err(err) = publisher.update_shared_task_by_id(ctx.task.id).await
                {
                    tracing::warn!(
                        ?err,
                        "Failed to propagate shared task update for {}",
                        ctx.task.id
                    );
                }
            }

            // Try to capture after-head commits if worktree still exists
            self.update_after_head_commits(execution_process.id).await;

            return Ok(());
        }

        let child = child.unwrap();

        ExecutionProcess::update_completion(&self.db.pool, execution_process.id, status, exit_code)
            .await?;

        // Try graceful interrupt first, then force kill
        if let Some(interrupt_sender) = self.take_interrupt_sender(&execution_process.id).await {
            // Send interrupt signal (ignore error if receiver dropped)
            let _ = interrupt_sender.send(());

            // Wait for graceful exit with timeout
            let graceful_exit = {
                let mut child_guard = child.write().await;
                tokio::time::timeout(Duration::from_secs(5), child_guard.wait()).await
            };

            match graceful_exit {
                Ok(Ok(_)) => {
                    tracing::debug!(
                        "Process {} exited gracefully after interrupt",
                        execution_process.id
                    );
                }
                Ok(Err(e)) => {
                    tracing::info!("Error waiting for process {}: {}", execution_process.id, e);
                }
                Err(_) => {
                    tracing::debug!(
                        "Graceful shutdown timed out for process {}, force killing",
                        execution_process.id
                    );
                }
            }
        }

        // Kill the child process and remove from the store
        {
            let mut child_guard = child.write().await;
            if let Err(e) = command::kill_process_group(&mut child_guard).await {
                tracing::error!(
                    "Failed to stop execution process {}: {}",
                    execution_process.id,
                    e
                );
                return Err(e);
            }
        }
        self.remove_child_from_store(&execution_process.id).await;

        // Mark the process finished in the MsgStore
        if let Some(msg) = self.msg_stores.write().await.remove(&execution_process.id) {
            msg.push_finished();
        }

        // Cleanup protocol peer (releases stdin reference)
        self.approvals
            .unregister_protocol_peer(&execution_process.id)
            .await;

        // Update task status to InReview when execution is stopped
        if let Ok(ctx) = ExecutionProcess::load_context(&self.db.pool, execution_process.id).await
            && !matches!(
                ctx.execution_process.run_reason,
                ExecutionProcessRunReason::DevServer
            )
        {
            if let Err(e) =
                Task::update_status(&self.db.pool, ctx.task.id, TaskStatus::InReview).await
            {
                tracing::error!("Failed to update task status to InReview: {e}");
            } else if let Some(publisher) = self.share_publisher()
                && let Err(err) = publisher.update_shared_task_by_id(ctx.task.id).await
            {
                tracing::warn!(
                    ?err,
                    "Failed to propagate shared task update for {}",
                    ctx.task.id
                );
            }
        }

        tracing::debug!(
            "Execution process {} stopped successfully",
            execution_process.id
        );

        // Record after-head commit OID (best-effort)
        self.update_after_head_commits(execution_process.id).await;

        Ok(())
    }

    async fn stream_diff(
        &self,
        workspace: &Workspace,
        stats_only: bool,
    ) -> Result<futures::stream::BoxStream<'static, Result<LogMsg, std::io::Error>>, ContainerError>
    {
        let workspace_repos =
            WorkspaceRepo::find_by_workspace_id(&self.db.pool, workspace.id).await?;
        let target_branches: HashMap<_, _> = workspace_repos
            .iter()
            .map(|wr| (wr.repo_id, wr.target_branch.clone()))
            .collect();

        let repositories =
            WorkspaceRepo::find_repos_for_workspace(&self.db.pool, workspace.id).await?;

        let mut streams = Vec::new();

        let container_ref = self.ensure_container_exists(workspace).await?;
        let workspace_root = PathBuf::from(container_ref);

        for repo in repositories {
            let worktree_path = workspace_root.join(&repo.name);
            let branch = &workspace.branch;

            let Some(target_branch) = target_branches.get(&repo.id) else {
                tracing::warn!(
                    "Skipping diff stream for repo {}: no target branch configured",
                    repo.name
                );
                continue;
            };

            let base_commit = match self
                .git()
                .get_base_commit(&repo.path, branch, target_branch)
            {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(
                        "Skipping diff stream for repo {}: failed to get base commit: {}",
                        repo.name,
                        e
                    );
                    continue;
                }
            };

            let stream = self
                .create_live_diff_stream(
                    &worktree_path,
                    &base_commit,
                    stats_only,
                    Some(repo.name.clone()),
                )
                .await?;

            streams.push(Box::pin(stream));
        }

        if streams.is_empty() {
            return Ok(Box::pin(futures::stream::empty()));
        }

        // Merge all streams into one
        Ok(Box::pin(futures::stream::select_all(streams)))
    }

    async fn try_commit_changes(&self, ctx: &ExecutionContext) -> Result<bool, ContainerError> {
        if !matches!(
            ctx.execution_process.run_reason,
            ExecutionProcessRunReason::CodingAgent | ExecutionProcessRunReason::CleanupScript,
        ) {
            return Ok(false);
        }

        let message = self.get_commit_message(ctx).await;

        let container_ref = ctx
            .workspace
            .container_ref
            .as_ref()
            .ok_or_else(|| ContainerError::Other(anyhow!("Container reference not found")))?;
        let workspace_root = PathBuf::from(container_ref);

        let repos_with_changes = self.check_repos_for_changes(&workspace_root, &ctx.repos)?;
        if repos_with_changes.is_empty() {
            tracing::debug!("No changes to commit in any repository");
            return Ok(false);
        }

        Ok(self.commit_repos(repos_with_changes, &message))
    }

    /// Copy files from the original project directory to the worktree.
    /// Skips files that already exist at target with same size.
    async fn copy_project_files(
        &self,
        source_dir: &Path,
        target_dir: &Path,
        copy_files: &str,
    ) -> Result<(), ContainerError> {
        let source_dir = source_dir.to_path_buf();
        let target_dir = target_dir.to_path_buf();
        let copy_files = copy_files.to_string();

        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            tokio::task::spawn_blocking(move || {
                copy::copy_project_files_impl(&source_dir, &target_dir, &copy_files)
            }),
        )
        .await
        .map_err(|_| ContainerError::Other(anyhow!("Copy project files timed out after 30s")))?
        .map_err(|e| ContainerError::Other(anyhow!("Copy files task failed: {e}")))?
    }

    async fn kill_all_running_processes(&self) -> Result<(), ContainerError> {
        tracing::info!("Killing all running processes");
        let running_processes = ExecutionProcess::find_running(&self.db.pool).await?;

        for process in running_processes {
            if let Err(error) = self
                .stop_execution(&process, ExecutionProcessStatus::Killed)
                .await
            {
                tracing::error!(
                    "Failed to cleanly kill running execution process {:?}: {:?}",
                    process,
                    error
                );
            }
        }

        Ok(())
    }

    async fn start_conversation_execution_inner(
        &self,
        execution_process: &ExecutionProcess,
        executor_action: &ExecutorAction,
        working_dir: &Path,
    ) -> Result<(), ContainerError> {
        // Use noop approvals service for conversation executions
        let approvals_service: Arc<dyn ExecutorApprovalService> =
            Arc::new(NoopExecutorApprovalService {});

        // Build ExecutionEnv with minimal context (no VK_* workspace variables)
        let env = ExecutionEnv::new();

        // Spawn the executor in the specified working directory
        let mut spawned = tokio::time::timeout(
            Duration::from_secs(30),
            executor_action.spawn(working_dir, approvals_service, &env),
        )
        .await
        .map_err(|_| {
            ContainerError::Other(anyhow!(
                "Timeout: process took more than 30 seconds to start"
            ))
        })??;

        self.track_child_msgs_in_store(execution_process.id, &mut spawned.child)
            .await;

        self.add_child_to_store(execution_process.id, spawned.child)
            .await;

        // Store interrupt sender for graceful shutdown
        if let Some(interrupt_sender) = spawned.interrupt_sender {
            self.add_interrupt_sender(execution_process.id, interrupt_sender)
                .await;
        }

        // Spawn exit monitor for conversation execution
        self.spawn_conversation_exit_monitor(&execution_process.id, spawned.exit_signal);

        Ok(())
    }
}
fn success_exit_status() -> std::process::ExitStatus {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        ExitStatusExt::from_raw(0)
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::ExitStatusExt;
        ExitStatusExt::from_raw(0)
    }
}

#[cfg(test)]
mod tests {
    use dashmap::DashSet;
    use std::sync::Arc;
    use uuid::Uuid;

    /// Tests that the DashSet-based guard correctly blocks duplicate workspace spawns.
    /// This verifies the core logic used in start_execution() at line ~1898.
    #[test]
    fn running_workspaces_guard_blocks_duplicate() {
        let running_workspaces: Arc<DashSet<Uuid>> = Arc::new(DashSet::new());
        let workspace_id = Uuid::new_v4();

        // First insert should succeed (returns true = was not present)
        assert!(
            running_workspaces.insert(workspace_id),
            "First insert should succeed"
        );

        // Second insert should fail (returns false = already present)
        assert!(
            !running_workspaces.insert(workspace_id),
            "Second insert should fail - workspace already running"
        );

        // Verify the workspace is tracked
        assert!(
            running_workspaces.contains(&workspace_id),
            "Workspace should be in the running set"
        );
    }

    /// Tests that removing a workspace from the guard allows it to be started again.
    /// This verifies the cleanup logic used in spawn_exit_monitor() at line ~688.
    #[test]
    fn running_workspaces_guard_clears_on_completion() {
        let running_workspaces: Arc<DashSet<Uuid>> = Arc::new(DashSet::new());
        let workspace_id = Uuid::new_v4();

        // Insert workspace (simulates start_execution)
        assert!(running_workspaces.insert(workspace_id));

        // Remove workspace (simulates spawn_exit_monitor cleanup)
        running_workspaces.remove(&workspace_id);

        // Workspace should no longer be tracked
        assert!(
            !running_workspaces.contains(&workspace_id),
            "Workspace should be removed from the running set"
        );

        // Should be able to insert again (simulates restart)
        assert!(
            running_workspaces.insert(workspace_id),
            "Should be able to start workspace again after completion"
        );
    }

    /// Tests that concurrent access to the guard is thread-safe.
    /// DashSet provides lock-free concurrent access.
    #[tokio::test]
    async fn running_workspaces_guard_concurrent_access() {
        let running_workspaces: Arc<DashSet<Uuid>> = Arc::new(DashSet::new());
        let workspace_id = Uuid::new_v4();

        // Spawn multiple tasks trying to insert the same workspace concurrently
        let mut handles = Vec::new();
        for _ in 0..10 {
            let guard = running_workspaces.clone();
            let id = workspace_id;
            handles.push(tokio::spawn(async move { guard.insert(id) }));
        }

        // Collect results
        let results: Vec<bool> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        // Exactly one insert should succeed (return true)
        let successes = results.iter().filter(|&&r| r).count();
        assert_eq!(
            successes, 1,
            "Exactly one concurrent insert should succeed"
        );

        // The rest should fail (return false)
        let failures = results.iter().filter(|&&r| !r).count();
        assert_eq!(failures, 9, "Nine concurrent inserts should fail");
    }

    /// Tests that different workspaces can run concurrently.
    #[test]
    fn running_workspaces_guard_allows_different_workspaces() {
        let running_workspaces: Arc<DashSet<Uuid>> = Arc::new(DashSet::new());
        let workspace_1 = Uuid::new_v4();
        let workspace_2 = Uuid::new_v4();

        // Both workspaces can be inserted
        assert!(running_workspaces.insert(workspace_1));
        assert!(running_workspaces.insert(workspace_2));

        // Both should be tracked
        assert!(running_workspaces.contains(&workspace_1));
        assert!(running_workspaces.contains(&workspace_2));

        // Removing one doesn't affect the other
        running_workspaces.remove(&workspace_1);
        assert!(!running_workspaces.contains(&workspace_1));
        assert!(running_workspaces.contains(&workspace_2));
    }
}
