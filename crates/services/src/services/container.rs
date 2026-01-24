use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Error as AnyhowError, anyhow};
use async_trait::async_trait;
use db::{
    DBService,
    models::{
        coding_agent_turn::{CodingAgentTurn, CreateCodingAgentTurn},
        conversation_session::ConversationSession,
        execution_process::{
            CreateExecutionProcess, ExecutionContext, ExecutionProcess, ExecutionProcessRunReason,
            ExecutionProcessStatus,
        },
        execution_process_logs::ExecutionProcessLogs,
        execution_process_normalized_entry::ExecutionProcessNormalizedEntry,
        execution_process_repo_state::{
            CreateExecutionProcessRepoState, ExecutionProcessRepoState,
        },
        execution_queue::ExecutionQueue,
        project::{Project, UpdateProject},
        project_repo::{ProjectRepo, ProjectRepoWithName},
        repo::Repo,
        session::{CreateSession, Session, SessionError},
        task::{Task, TaskStatus},
        workspace::{Workspace, WorkspaceError},
        workspace_repo::WorkspaceRepo,
    },
};
use executors::{
    actions::{
        ExecutorAction, ExecutorActionType,
        coding_agent_initial::CodingAgentInitialRequest,
        script::{ScriptContext, ScriptRequest, ScriptRequestLanguage},
    },
    executors::{ExecutorError, StandardCodingAgentExecutor, claude::SkillsData},
    logs::{NormalizedEntry, NormalizedEntryError, NormalizedEntryType, utils::ConversationPatch},
    profile::{ExecutorConfigs, ExecutorProfileId},
};
use futures::{StreamExt, future};
use sqlx::{Error as SqlxError, SqlitePool};
use thiserror::Error;
use tokio::{sync::RwLock, task::JoinHandle};
use utils::{
    log_msg::LogMsg,
    msg_store::MsgStore,
    text::{git_branch_id, short_uuid},
};
use uuid::Uuid;

use crate::services::{
    config::Config,
    git::{GitService, GitServiceError},
    notification::NotificationService,
    share::SharePublisher,
    skills_cache::GlobalSkillsCache,
    watcher_manager::WatcherManager,
    workspace_manager::WorkspaceError as WorkspaceManagerError,
    worktree_manager::WorktreeError,
};
pub type ContainerRef = String;

/// Result of starting a workspace execution
#[derive(Debug)]
pub enum StartWorkspaceResult {
    /// Execution started immediately
    Started(ExecutionProcess),
    /// Execution was queued due to concurrency limit
    Queued(ExecutionQueue),
}

enum NormalizedEntryPatchOp {
    Upsert {
        index: i64,
        entry: Box<NormalizedEntry>,
    },
    Remove {
        index: i64,
    },
}

fn extract_normalized_entry_ops(patch: &json_patch::Patch) -> Vec<NormalizedEntryPatchOp> {
    let value = match serde_json::to_value(patch) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };
    let ops = match value.as_array() {
        Some(ops) => ops,
        None => return Vec::new(),
    };

    ops.iter()
        .filter_map(|op| {
            let op_type = op.get("op")?.as_str()?;
            let path = op.get("path")?.as_str()?;
            let entry_index = path.strip_prefix("/entries/")?.parse::<i64>().ok()?;

            match op_type {
                "add" | "replace" => {
                    let value = op.get("value")?;
                    let entry_type = value.get("type")?.as_str()?;
                    if entry_type != "NORMALIZED_ENTRY" {
                        return None;
                    }
                    let content = value.get("content")?;
                    let entry: NormalizedEntry = serde_json::from_value(content.clone()).ok()?;
                    Some(NormalizedEntryPatchOp::Upsert {
                        index: entry_index,
                        entry: Box::new(entry),
                    })
                }
                "remove" => Some(NormalizedEntryPatchOp::Remove { index: entry_index }),
                _ => None,
            }
        })
        .collect()
}

async fn apply_normalized_entry_ops(
    pool: &SqlitePool,
    execution_id: Uuid,
    ops: Vec<NormalizedEntryPatchOp>,
) -> Result<(), ContainerError> {
    for op in ops {
        match op {
            NormalizedEntryPatchOp::Upsert { index, entry } => {
                ExecutionProcessNormalizedEntry::upsert(pool, execution_id, index, entry.as_ref())
                    .await
                    .map_err(ContainerError::Other)?;
            }
            NormalizedEntryPatchOp::Remove { index } => {
                ExecutionProcessNormalizedEntry::delete(pool, execution_id, index).await?;
            }
        }
    }

    Ok(())
}

async fn persist_normalized_entries_from_store(
    pool: SqlitePool,
    execution_id: Uuid,
    store: Arc<MsgStore>,
) -> Result<(), ContainerError> {
    use tokio::time::{Duration, sleep};

    let mut processed_len = 0usize;
    let mut last_len = 0usize;
    let mut stable_rounds = 0u8;
    let mut rounds = 0u8;
    let max_rounds = 10u8;
    let delay = Duration::from_millis(200);

    while stable_rounds < 2 && rounds < max_rounds {
        rounds = rounds.saturating_add(1);
        let history = store.get_history();
        let history_len = history.len();

        if history_len == last_len {
            stable_rounds = stable_rounds.saturating_add(1);
        } else {
            stable_rounds = 0;
            last_len = history_len;
        }

        if history_len < processed_len {
            processed_len = 0;
        }

        for msg in history.into_iter().skip(processed_len) {
            if let LogMsg::JsonPatch(patch) = msg {
                let ops = extract_normalized_entry_ops(&patch);
                if ops.is_empty() {
                    continue;
                }
                apply_normalized_entry_ops(&pool, execution_id, ops).await?;
            }
        }

        processed_len = history_len;

        if stable_rounds >= 2 {
            break;
        }

        sleep(delay).await;
    }

    if stable_rounds < 2 {
        tracing::warn!(
            "Normalized entry backfill may be incomplete for execution {}",
            execution_id
        );
    }

    Ok(())
}

/// Derive the default execution purpose from the run reason.
/// Returns a static string representing the purpose of the execution.
pub fn purpose_from_run_reason(run_reason: &ExecutionProcessRunReason) -> &'static str {
    match run_reason {
        ExecutionProcessRunReason::CodingAgent => "task",
        ExecutionProcessRunReason::SetupScript => "setup",
        ExecutionProcessRunReason::CleanupScript => "cleanup",
        ExecutionProcessRunReason::DevServer => "dev_server",
        ExecutionProcessRunReason::InternalAgent => "internal",
        ExecutionProcessRunReason::DisposableConversation => "conversation",
    }
}

#[derive(Debug, Error)]
pub enum ContainerError {
    #[error(transparent)]
    GitServiceError(#[from] GitServiceError),
    #[error(transparent)]
    Sqlx(#[from] SqlxError),
    #[error(transparent)]
    ExecutorError(#[from] ExecutorError),
    #[error(transparent)]
    Worktree(#[from] WorktreeError),
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    WorkspaceManager(#[from] WorkspaceManagerError),
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error("Io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to kill process: {0}")]
    KillFailed(std::io::Error),
    #[error("Execution timed out after {0:?}")]
    ExecutionTimeout(Duration),
    #[error("Execution failed with status: {0:?}")]
    ExecutionFailed(ExecutionProcessStatus),
    #[error("Execution not found: {0}")]
    ExecutionNotFound(Uuid),
    #[error("Workspace {0} already has a running agent")]
    WorkspaceAlreadyRunning(Uuid),
    #[error(transparent)]
    Other(#[from] AnyhowError), // Catches any unclassified errors
}

#[async_trait]
pub trait ContainerService {
    fn msg_stores(&self) -> &Arc<RwLock<HashMap<Uuid, Arc<MsgStore>>>>;

    fn db(&self) -> &DBService;

    fn git(&self) -> &GitService;

    fn share_publisher(&self) -> Option<&SharePublisher>;

    fn notification_service(&self) -> &NotificationService;

    /// Get the shared watcher manager for filesystem watching.
    /// Returns None if the implementation doesn't support shared watching.
    fn watcher_manager(&self) -> Option<&WatcherManager>;

    /// Get the deployment config for accessing max_concurrent_agents setting.
    fn config(&self) -> &Arc<RwLock<Config>>;

    /// Get the global skills cache for storing Claude Code skills data.
    fn skills_cache(&self) -> &GlobalSkillsCache;

    fn workspace_to_current_dir(&self, workspace: &Workspace) -> PathBuf;

    async fn create(&self, workspace: &Workspace) -> Result<ContainerRef, ContainerError>;

    async fn kill_all_running_processes(&self) -> Result<(), ContainerError>;

    async fn delete(&self, workspace: &Workspace) -> Result<(), ContainerError>;

    /// Check if a task has any running execution processes
    async fn has_running_processes(&self, task_id: Uuid) -> Result<bool, ContainerError> {
        Ok(ExecutionProcess::has_running_processes_for_task(&self.db().pool, task_id).await?)
    }

    /// Check if execution should be queued based on concurrency limit.
    /// Returns true if running agents >= max_concurrent_agents (and limit is enabled).
    /// Returns false if max_concurrent_agents == 0 (unlimited).
    async fn should_queue_execution(&self) -> Result<bool, ContainerError> {
        let max_concurrent = self.config().read().await.max_concurrent_agents;

        // 0 means unlimited - never queue
        if max_concurrent == 0 {
            return Ok(false);
        }

        let running_count = ExecutionProcess::count_running_agents(&self.db().pool).await?;
        Ok(running_count >= max_concurrent as i64)
    }

    /// Process the execution queue - start queued workspaces/follow-ups when slots are available.
    /// Pops entries from the queue and starts execution until at capacity or queue empty.
    /// Handles both initial workspace starts (session_id is None) and follow-up executions
    /// (session_id and executor_action are populated).
    async fn process_queue(&self) -> Result<(), ContainerError> {
        loop {
            // Check if we can start more executions
            if self.should_queue_execution().await? {
                // At capacity, stop processing
                break;
            }

            // Try to pop next from queue
            let entry = match ExecutionQueue::pop_next(&self.db().pool).await {
                Ok(Some(e)) => e,
                Ok(None) => {
                    // Queue is empty
                    break;
                }
                Err(e) => {
                    tracing::error!("Failed to pop from execution queue: {}", e);
                    break;
                }
            };

            // Load workspace
            let workspace = match Workspace::find_by_id(&self.db().pool, entry.workspace_id).await {
                Ok(Some(w)) => w,
                Ok(None) => {
                    tracing::warn!(
                        "Workspace {} not found when processing queue, skipping",
                        entry.workspace_id
                    );
                    continue;
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to load workspace {} from queue: {}",
                        entry.workspace_id,
                        e
                    );
                    continue;
                }
            };

            // Note: is_queued is updated automatically via database trigger on execution_queue DELETE

            // Check if this is a follow-up or initial start
            if entry.is_follow_up() {
                // Follow-up execution
                let session_id = entry.session_id.unwrap();
                let executor_action = match entry.parsed_executor_action() {
                    Some(action) => action,
                    None => {
                        tracing::error!(
                            "Failed to parse executor action for queued follow-up, skipping"
                        );
                        continue;
                    }
                };

                let session = match Session::find_by_id(&self.db().pool, session_id).await {
                    Ok(Some(s)) => s,
                    Ok(None) => {
                        tracing::warn!(
                            "Session {} not found when processing queue, skipping",
                            session_id
                        );
                        continue;
                    }
                    Err(e) => {
                        tracing::error!("Failed to load session {} from queue: {}", session_id, e);
                        continue;
                    }
                };

                tracing::info!(
                    "Starting queued follow-up for workspace {} session {} with executor {:?}",
                    workspace.id,
                    session_id,
                    entry.executor_profile_id.0
                );

                // Start the follow-up execution directly
                if let Err(e) = self
                    .start_execution(
                        &workspace,
                        &session,
                        &executor_action,
                        &ExecutionProcessRunReason::CodingAgent,
                        None,
                    )
                    .await
                {
                    tracing::error!(
                        "Failed to start queued follow-up for workspace {}: {}",
                        workspace.id,
                        e
                    );
                }
            } else {
                // Initial workspace start
                tracing::info!(
                    "Starting queued workspace {} with executor {:?}",
                    workspace.id,
                    entry.executor_profile_id.0
                );

                // Start the workspace execution (inner call that bypasses queue check)
                if let Err(e) = self
                    .start_workspace_inner(&workspace, entry.executor_profile_id.0.clone())
                    .await
                {
                    tracing::error!("Failed to start queued workspace {}: {}", workspace.id, e);
                    // Continue processing other queue entries even if one fails
                }
            }
        }

        Ok(())
    }

    /// Wait for an execution process to complete (status != Running).
    ///
    /// Polls `ExecutionProcess::find_by_id()` at regular intervals until the
    /// execution finishes or the timeout is reached.
    ///
    /// Returns the completed `ExecutionProcess` on success, or an error if:
    /// - The execution is not found
    /// - The timeout is exceeded
    /// - The execution failed (status == Failed)
    async fn wait_for_execution_completion(
        &self,
        exec_id: Uuid,
        timeout: Duration,
    ) -> Result<ExecutionProcess, ContainerError> {
        use tokio::time::{Instant, sleep};

        const POLL_INTERVAL: Duration = Duration::from_millis(500);

        let deadline = Instant::now() + timeout;

        loop {
            let process = ExecutionProcess::find_by_id(&self.db().pool, exec_id)
                .await?
                .ok_or(ContainerError::ExecutionNotFound(exec_id))?;

            match process.status {
                ExecutionProcessStatus::Running => {
                    if Instant::now() >= deadline {
                        return Err(ContainerError::ExecutionTimeout(timeout));
                    }
                    sleep(POLL_INTERVAL).await;
                }
                ExecutionProcessStatus::Failed => {
                    return Err(ContainerError::ExecutionFailed(process.status));
                }
                ExecutionProcessStatus::Completed | ExecutionProcessStatus::Killed => {
                    return Ok(process);
                }
            }
        }
    }

    /// A context is finalized when
    /// - Always when the execution process has failed or been killed
    /// - Never when the run reason is DevServer
    /// - Never when the run reason is InternalAgent (feedback, pr_description, merge_message)
    /// - Never when a setup script has no next_action (parallel mode)
    /// - The next action is None (no follow-up actions)
    fn should_finalize(&self, ctx: &ExecutionContext) -> bool {
        // Never finalize DevServer or InternalAgent processes
        // InternalAgent is used for internal operations (feedback collection, PR descriptions, etc.)
        // that should not affect task status
        if matches!(
            ctx.execution_process.run_reason,
            ExecutionProcessRunReason::DevServer | ExecutionProcessRunReason::InternalAgent
        ) {
            return false;
        }

        // Never finalize setup scripts without a next_action (parallel mode).
        // In sequential mode, setup scripts have next_action pointing to coding agent,
        // so they won't finalize anyway (handled by next_action.is_none() check below).
        let action = ctx.execution_process.executor_action().unwrap();
        if matches!(
            ctx.execution_process.run_reason,
            ExecutionProcessRunReason::SetupScript
        ) && action.next_action.is_none()
        {
            return false;
        }

        // Always finalize failed or killed executions, regardless of next action
        if matches!(
            ctx.execution_process.status,
            ExecutionProcessStatus::Failed | ExecutionProcessStatus::Killed
        ) {
            return true;
        }

        // Otherwise, finalize only if no next action
        action.next_action.is_none()
    }

    /// Finalize task execution by updating status to InReview and sending notifications
    async fn finalize_task(
        &self,
        share_publisher: Option<&SharePublisher>,
        ctx: &ExecutionContext,
    ) {
        match Task::update_status(&self.db().pool, ctx.task.id, TaskStatus::InReview).await {
            Ok(_) => {
                if let Some(publisher) = share_publisher
                    && let Err(err) = publisher.update_shared_task_by_id(ctx.task.id).await
                {
                    tracing::warn!(
                        ?err,
                        "Failed to propagate shared task update for {}",
                        ctx.task.id
                    );
                }
            }
            Err(e) => {
                tracing::error!("Failed to update task status to InReview: {e}");
            }
        }

        // Skip notification if process was intentionally killed by user
        if matches!(ctx.execution_process.status, ExecutionProcessStatus::Killed) {
            return;
        }

        let title = format!("Task Complete: {}", ctx.task.title);
        match ctx.execution_process.status {
            ExecutionProcessStatus::Completed => {
                let message = format!(
                    "✅ '{}' completed successfully\nBranch: {:?}\nExecutor: {:?}",
                    ctx.task.title, ctx.workspace.branch, ctx.session.executor
                );
                // OS notification
                self.notification_service().notify(&title, &message).await;
                // In-app notification
                if let Err(e) = NotificationService::notify_agent_complete(
                    &self.db().pool,
                    ctx.project.id,
                    ctx.workspace.id,
                    &ctx.task.title,
                )
                .await
                {
                    tracing::warn!("Failed to create in-app completion notification: {}", e);
                }
            }
            ExecutionProcessStatus::Failed => {
                let message = format!(
                    "❌ '{}' execution failed\nBranch: {:?}\nExecutor: {:?}",
                    ctx.task.title, ctx.workspace.branch, ctx.session.executor
                );
                // OS notification
                self.notification_service()
                    .notify_error(&title, &message)
                    .await;
                // In-app notification
                if let Err(e) = NotificationService::notify_agent_error(
                    &self.db().pool,
                    ctx.project.id,
                    ctx.workspace.id,
                    &ctx.task.title,
                )
                .await
                {
                    tracing::warn!("Failed to create in-app error notification: {}", e);
                }
            }
            _ => {
                tracing::warn!(
                    "Tried to notify workspace completion for {} but process is still running!",
                    ctx.workspace.id
                );
            }
        }
    }

    /// Try to collect agent feedback for a workspace when task transitions to Done.
    ///
    /// This method checks if feedback already exists for the workspace, and if not,
    /// attempts to collect it from the agent (if a valid session exists).
    ///
    /// Default implementation does nothing (for services that don't support feedback).
    /// LocalContainerService overrides this with actual feedback collection.
    async fn try_collect_feedback_for_workspace(
        &self,
        _workspace_id: Uuid,
    ) -> Result<(), ContainerError> {
        // Default: no-op for services that don't support feedback collection
        Ok(())
    }

    /// Cleanup executions marked as running in the db, call at startup
    async fn cleanup_orphan_executions(&self) -> Result<(), ContainerError> {
        let running_processes = ExecutionProcess::find_running(&self.db().pool).await?;
        for process in running_processes {
            tracing::info!(
                "Found orphaned execution process {} for session {:?}",
                process.id,
                process.session_id
            );
            // Update the execution process status first
            if let Err(e) = ExecutionProcess::update_completion(
                &self.db().pool,
                process.id,
                ExecutionProcessStatus::Failed,
                None, // No exit code for orphaned processes
            )
            .await
            {
                tracing::error!(
                    "Failed to update orphaned execution process {} status: {}",
                    process.id,
                    e
                );
                continue;
            }
            // Capture after-head commit OID per repository (only for workspace-based executions)
            if let Ok(ctx) = ExecutionProcess::load_context(&self.db().pool, process.id).await
                && let Some(ref container_ref) = ctx.workspace.container_ref
            {
                let workspace_root = PathBuf::from(container_ref);
                for repo in &ctx.repos {
                    let repo_path = workspace_root.join(&repo.name);
                    if let Ok(head) = self.git().get_head_info(&repo_path)
                        && let Err(err) = ExecutionProcessRepoState::update_after_head_commit(
                            &self.db().pool,
                            process.id,
                            repo.id,
                            &head.oid,
                        )
                        .await
                    {
                        tracing::warn!(
                            "Failed to update after_head_commit for repo {} on process {}: {}",
                            repo.id,
                            process.id,
                            err
                        );
                    }
                }
            }
            // Process marked as failed
            tracing::info!("Marked orphaned execution process {} as failed", process.id);
            // Update task status to InReview for coding agent and setup script failures
            // Skip for conversation-based executions (no session_id)
            if let Some(session_id) = process.session_id
                && matches!(
                    process.run_reason,
                    ExecutionProcessRunReason::CodingAgent
                        | ExecutionProcessRunReason::SetupScript
                        | ExecutionProcessRunReason::CleanupScript
                )
                && let Ok(Some(session)) = Session::find_by_id(&self.db().pool, session_id).await
                && let Ok(Some(workspace)) =
                    Workspace::find_by_id(&self.db().pool, session.workspace_id).await
                && let Ok(Some(task)) = workspace.parent_task(&self.db().pool).await
            {
                match Task::update_status(&self.db().pool, task.id, TaskStatus::InReview).await {
                    Ok(_) => {
                        if let Some(publisher) = self.share_publisher()
                            && let Err(err) = publisher.update_shared_task_by_id(task.id).await
                        {
                            tracing::warn!(
                                ?err,
                                "Failed to propagate shared task update for {}",
                                task.id
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to update task status to InReview for orphaned session: {}",
                            e
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// Backfill before_head_commit for legacy execution processes.
    /// Rules:
    /// - If a process has after_head_commit and missing before_head_commit,
    ///   then set before_head_commit to the previous process's after_head_commit.
    /// - If there is no previous process, set before_head_commit to the base branch commit.
    async fn backfill_before_head_commits(&self) -> Result<(), ContainerError> {
        let pool = &self.db().pool;
        let rows = ExecutionProcess::list_missing_before_context(pool).await?;
        for row in rows {
            // Skip if no after commit at all (shouldn't happen due to WHERE)
            // Prefer previous process after-commit if present
            let mut before = row.prev_after_head_commit.clone();

            // Fallback to base branch commit OID
            if before.is_none() {
                let repo_path = std::path::Path::new(row.repo_path.as_deref().unwrap_or_default());
                match self
                    .git()
                    .get_branch_oid(repo_path, row.target_branch.as_str())
                {
                    Ok(oid) => before = Some(oid),
                    Err(e) => {
                        tracing::warn!(
                            "Backfill: Failed to resolve base branch OID for workspace {} (branch {}): {}",
                            row.workspace_id,
                            row.target_branch,
                            e
                        );
                    }
                }
            }

            if let Some(before_oid) = before
                && let Err(e) = ExecutionProcessRepoState::update_before_head_commit(
                    pool,
                    row.id,
                    row.repo_id,
                    &before_oid,
                )
                .await
            {
                tracing::warn!(
                    "Backfill: Failed to update before_head_commit for process {}: {}",
                    row.id,
                    e
                );
            }
        }

        Ok(())
    }

    /// Backfill repo names that were migrated with a sentinel placeholder.
    /// Also backfills dev_script_working_dir and agent_working_dir for single-repo projects.
    async fn backfill_repo_names(&self) -> Result<(), ContainerError> {
        let pool = &self.db().pool;
        let repos = Repo::list_needing_name_fix(pool).await?;

        if repos.is_empty() {
            return Ok(());
        }

        tracing::info!("Backfilling {} repo names", repos.len());

        for repo in repos {
            let name = repo
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&repo.id.to_string())
                .to_string();

            Repo::update_name(pool, repo.id, &name, &name).await?;

            // Also update dev_script_working_dir and agent_working_dir for single-repo projects
            let project_repos = ProjectRepo::find_by_repo_id(pool, repo.id).await?;
            for pr in project_repos {
                let all_repos = ProjectRepo::find_by_project_id(pool, pr.project_id).await?;
                if all_repos.len() == 1
                    && let Some(project) = Project::find_by_id(pool, pr.project_id).await?
                {
                    let needs_dev_script_working_dir = project
                        .dev_script
                        .as_ref()
                        .map(|s| !s.is_empty())
                        .unwrap_or(false)
                        && project
                            .dev_script_working_dir
                            .as_ref()
                            .map(|s| s.is_empty())
                            .unwrap_or(true);

                    let needs_default_agent_working_dir = project
                        .default_agent_working_dir
                        .as_ref()
                        .map(|s| s.is_empty())
                        .unwrap_or(true);

                    if needs_dev_script_working_dir || needs_default_agent_working_dir {
                        Project::update(
                            pool,
                            pr.project_id,
                            &UpdateProject {
                                name: Some(project.name.clone()),
                                dev_script: project.dev_script.clone(),
                                dev_script_working_dir: if needs_dev_script_working_dir {
                                    Some(name.clone())
                                } else {
                                    project.dev_script_working_dir.clone()
                                },
                                default_agent_working_dir: if needs_default_agent_working_dir {
                                    Some(name.clone())
                                } else {
                                    project.default_agent_working_dir.clone()
                                },
                            },
                        )
                        .await?;
                    }
                }
            }
        }

        Ok(())
    }

    fn cleanup_actions_for_repos(&self, repos: &[ProjectRepoWithName]) -> Option<ExecutorAction> {
        let repos_with_cleanup: Vec<_> = repos
            .iter()
            .filter(|r| r.cleanup_script.is_some())
            .collect();

        if repos_with_cleanup.is_empty() {
            return None;
        }

        let mut iter = repos_with_cleanup.iter();
        let first = iter.next()?;
        let mut root_action = ExecutorAction::new(
            ExecutorActionType::ScriptRequest(ScriptRequest {
                script: first.cleanup_script.clone().unwrap(),
                language: ScriptRequestLanguage::Bash,
                context: ScriptContext::CleanupScript,
                working_dir: Some(first.repo_name.clone()),
            }),
            None,
        );

        for repo in iter {
            root_action = root_action.append_action(ExecutorAction::new(
                ExecutorActionType::ScriptRequest(ScriptRequest {
                    script: repo.cleanup_script.clone().unwrap(),
                    language: ScriptRequestLanguage::Bash,
                    context: ScriptContext::CleanupScript,
                    working_dir: Some(repo.repo_name.clone()),
                }),
                None,
            ));
        }

        Some(root_action)
    }

    fn setup_actions_for_repos(&self, repos: &[ProjectRepoWithName]) -> Option<ExecutorAction> {
        let repos_with_setup: Vec<_> = repos.iter().filter(|r| r.setup_script.is_some()).collect();

        if repos_with_setup.is_empty() {
            return None;
        }

        let mut iter = repos_with_setup.iter();
        let first = iter.next()?;
        let mut root_action = ExecutorAction::new(
            ExecutorActionType::ScriptRequest(ScriptRequest {
                script: first.setup_script.clone().unwrap(),
                language: ScriptRequestLanguage::Bash,
                context: ScriptContext::SetupScript,
                working_dir: Some(first.repo_name.clone()),
            }),
            None,
        );

        for repo in iter {
            root_action = root_action.append_action(ExecutorAction::new(
                ExecutorActionType::ScriptRequest(ScriptRequest {
                    script: repo.setup_script.clone().unwrap(),
                    language: ScriptRequestLanguage::Bash,
                    context: ScriptContext::SetupScript,
                    working_dir: Some(repo.repo_name.clone()),
                }),
                None,
            ));
        }

        Some(root_action)
    }

    fn setup_action_for_repo(repo: &ProjectRepoWithName) -> Option<ExecutorAction> {
        repo.setup_script.as_ref().map(|script| {
            ExecutorAction::new(
                ExecutorActionType::ScriptRequest(ScriptRequest {
                    script: script.clone(),
                    language: ScriptRequestLanguage::Bash,
                    context: ScriptContext::SetupScript,
                    working_dir: Some(repo.repo_name.clone()),
                }),
                None,
            )
        })
    }

    fn build_sequential_setup_chain(
        repos: &[&ProjectRepoWithName],
        next_action: ExecutorAction,
    ) -> ExecutorAction {
        let mut chained = next_action;
        for repo in repos.iter().rev() {
            if let Some(script) = &repo.setup_script {
                chained = ExecutorAction::new(
                    ExecutorActionType::ScriptRequest(ScriptRequest {
                        script: script.clone(),
                        language: ScriptRequestLanguage::Bash,
                        context: ScriptContext::SetupScript,
                        working_dir: Some(repo.repo_name.clone()),
                    }),
                    Some(Box::new(chained)),
                );
            }
        }
        chained
    }

    async fn try_stop(&self, workspace: &Workspace, include_dev_server: bool) {
        // stop execution processes for this workspace's sessions
        let sessions = match Session::find_by_workspace_id(&self.db().pool, workspace.id).await {
            Ok(s) => s,
            Err(_) => return,
        };

        for session in sessions {
            if let Ok(processes) =
                ExecutionProcess::find_by_session_id(&self.db().pool, session.id, false).await
            {
                for process in processes {
                    // Skip dev server processes unless explicitly included
                    if !include_dev_server
                        && process.run_reason == ExecutionProcessRunReason::DevServer
                    {
                        continue;
                    }
                    if process.status == ExecutionProcessStatus::Running {
                        self.stop_execution(&process, ExecutionProcessStatus::Killed)
                            .await
                            .unwrap_or_else(|e| {
                                tracing::debug!(
                                    "Failed to stop execution process {} for workspace {}: {}",
                                    process.id,
                                    workspace.id,
                                    e
                                );
                            });
                    }
                }
            }
        }
    }

    async fn ensure_container_exists(
        &self,
        workspace: &Workspace,
    ) -> Result<ContainerRef, ContainerError>;

    async fn is_container_clean(&self, workspace: &Workspace) -> Result<bool, ContainerError>;

    async fn start_execution_inner(
        &self,
        workspace: &Workspace,
        execution_process: &ExecutionProcess,
        executor_action: &ExecutorAction,
        purpose: &str,
    ) -> Result<(), ContainerError>;

    async fn stop_execution(
        &self,
        execution_process: &ExecutionProcess,
        status: ExecutionProcessStatus,
    ) -> Result<(), ContainerError>;

    async fn try_commit_changes(&self, ctx: &ExecutionContext) -> Result<bool, ContainerError>;

    async fn copy_project_files(
        &self,
        source_dir: &Path,
        target_dir: &Path,
        copy_files: &str,
    ) -> Result<(), ContainerError>;

    /// Stream diff updates as LogMsg for WebSocket endpoints.
    async fn stream_diff(
        &self,
        workspace: &Workspace,
        stats_only: bool,
    ) -> Result<futures::stream::BoxStream<'static, Result<LogMsg, std::io::Error>>, ContainerError>;

    /// Fetch the MsgStore for a given execution ID, panicking if missing.
    async fn get_msg_store_by_id(&self, uuid: &Uuid) -> Option<Arc<MsgStore>> {
        let map = self.msg_stores().read().await;
        map.get(uuid).cloned()
    }

    async fn git_branch_prefix(&self) -> String;

    async fn git_branch_from_workspace(&self, workspace_id: &Uuid, task_title: &str) -> String {
        let task_title_id = git_branch_id(task_title);
        let prefix = self.git_branch_prefix().await;

        if prefix.is_empty() {
            format!("{}-{}", short_uuid(workspace_id), task_title_id)
        } else {
            format!("{}/{}-{}", prefix, short_uuid(workspace_id), task_title_id)
        }
    }

    async fn stream_raw_logs(
        &self,
        id: &Uuid,
    ) -> Option<futures::stream::BoxStream<'static, Result<LogMsg, std::io::Error>>> {
        if let Some(store) = self.get_msg_store_by_id(id).await {
            // First try in-memory store
            return Some(
                store
                    .history_plus_stream()
                    .filter(|msg| {
                        future::ready(matches!(
                            msg,
                            Ok(LogMsg::Stdout(..) | LogMsg::Stderr(..) | LogMsg::Finished)
                        ))
                    })
                    .boxed(),
            );
        } else {
            // Fallback: load from DB and create direct stream
            let log_records =
                match ExecutionProcessLogs::find_by_execution_id(&self.db().pool, *id).await {
                    Ok(records) if !records.is_empty() => records,
                    Ok(_) => return None, // No logs exist
                    Err(e) => {
                        tracing::error!("Failed to fetch logs for execution {}: {}", id, e);
                        return None;
                    }
                };

            let messages = match ExecutionProcessLogs::parse_logs(&log_records) {
                Ok(msgs) => msgs,
                Err(e) => {
                    tracing::error!("Failed to parse logs for execution {}: {}", id, e);
                    return None;
                }
            };

            // Direct stream from parsed messages
            let stream = futures::stream::iter(
                messages
                    .into_iter()
                    .filter(|m| matches!(m, LogMsg::Stdout(_) | LogMsg::Stderr(_)))
                    .chain(std::iter::once(LogMsg::Finished))
                    .map(Ok::<_, std::io::Error>),
            )
            .boxed();

            Some(stream)
        }
    }

    async fn build_normalized_store_from_db(&self, id: &Uuid) -> Option<Arc<MsgStore>> {
        let log_records =
            match ExecutionProcessLogs::find_by_execution_id(&self.db().pool, *id).await {
                Ok(records) if !records.is_empty() => records,
                Ok(_) => return None,
                Err(e) => {
                    tracing::error!("Failed to fetch logs for execution {}: {}", id, e);
                    return None;
                }
            };

        let raw_messages = match ExecutionProcessLogs::parse_logs(&log_records) {
            Ok(msgs) => msgs,
            Err(e) => {
                tracing::error!("Failed to parse logs for execution {}: {}", id, e);
                return None;
            }
        };

        let temp_store = Arc::new(MsgStore::new());
        for msg in raw_messages {
            if matches!(
                msg,
                LogMsg::Stdout(_) | LogMsg::Stderr(_) | LogMsg::JsonPatch(_)
            ) {
                temp_store.push(msg);
            }
        }
        temp_store.push_finished();

        let process = match ExecutionProcess::find_by_id(&self.db().pool, *id).await {
            Ok(Some(process)) => process,
            Ok(None) => {
                tracing::error!("No execution process found for ID: {}", id);
                return None;
            }
            Err(e) => {
                tracing::error!("Failed to fetch execution process {}: {}", id, e);
                return None;
            }
        };

        let (workspace, _session) =
            match process.parent_workspace_and_session(&self.db().pool).await {
                Ok(Some((workspace, session))) => (workspace, session),
                Ok(None) => {
                    tracing::error!(
                        "No workspace/session found for session ID: {:?}",
                        process.session_id
                    );
                    return None;
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to fetch workspace for session {:?}: {}",
                        process.session_id,
                        e
                    );
                    return None;
                }
            };

        if let Err(err) = self.ensure_container_exists(&workspace).await {
            tracing::warn!(
                "Failed to recreate worktree before log normalization for workspace {}: {}",
                workspace.id,
                err
            );
        }

        let current_dir = self.workspace_to_current_dir(&workspace);

        let executor_action = if let Ok(executor_action) = process.executor_action() {
            executor_action
        } else {
            tracing::error!(
                "Failed to parse executor action: {:?}",
                process.executor_action()
            );
            return None;
        };

        // Create skills callback that updates the global cache
        let skills_cache = self.skills_cache().clone();
        let skills_callback: Option<Box<dyn FnOnce(SkillsData) + Send + 'static>> =
            Some(Box::new(move |skills_data| {
                let cache = skills_cache;
                tokio::spawn(async move {
                    cache.update_skills(skills_data).await;
                });
            }));

        match executor_action.typ() {
            ExecutorActionType::CodingAgentInitialRequest(request) => {
                let executor = ExecutorConfigs::get_cached()
                    .get_coding_agent_or_default(&request.executor_profile_id);
                executor.normalize_logs_with_skills_callback(
                    temp_store.clone(),
                    &current_dir,
                    skills_callback,
                );
            }
            ExecutorActionType::CodingAgentFollowUpRequest(request) => {
                let executor = ExecutorConfigs::get_cached()
                    .get_coding_agent_or_default(&request.executor_profile_id);
                executor.normalize_logs_with_skills_callback(temp_store.clone(), &current_dir, None);
            }
            _ => {
                tracing::debug!(
                    "Executor action doesn't support log normalization: {:?}",
                    process.executor_action()
                );
                return None;
            }
        }

        Some(temp_store)
    }

    async fn stream_normalized_logs(
        &self,
        id: &Uuid,
    ) -> Option<futures::stream::BoxStream<'static, Result<LogMsg, std::io::Error>>> {
        // First try in-memory store (existing behavior)
        if let Some(store) = self.get_msg_store_by_id(id).await {
            Some(
                store
                    .history_plus_stream() // BoxStream<Result<LogMsg, io::Error>>
                    .filter(|msg| future::ready(matches!(msg, Ok(LogMsg::JsonPatch(..)))))
                    .chain(futures::stream::once(async {
                        Ok::<_, std::io::Error>(LogMsg::Finished)
                    }))
                    .boxed(),
            )
        } else {
            let store = self.build_normalized_store_from_db(id).await?;
            Some(
                store
                    .history_plus_stream()
                    .filter(|msg| future::ready(matches!(msg, Ok(LogMsg::JsonPatch(..)))))
                    .chain(futures::stream::once(async {
                        Ok::<_, std::io::Error>(LogMsg::Finished)
                    }))
                    .boxed(),
            )
        }
    }

    async fn backfill_normalized_entries(&self, execution_id: Uuid) -> Result<(), ContainerError> {
        let store = if let Some(store) = self.get_msg_store_by_id(&execution_id).await {
            Some(store)
        } else {
            self.build_normalized_store_from_db(&execution_id).await
        };

        if let Some(store) = store {
            persist_normalized_entries_from_store(self.db().pool.clone(), execution_id, store)
                .await?;
        }

        Ok(())
    }

    fn spawn_stream_raw_logs_to_db(&self, execution_id: &Uuid) -> JoinHandle<()> {
        let execution_id = *execution_id;
        let msg_stores = self.msg_stores().clone();
        let db = self.db().clone();

        tokio::spawn(async move {
            // Get the message store for this execution
            let store = {
                let map = msg_stores.read().await;
                map.get(&execution_id).cloned()
            };

            if let Some(store) = store {
                let mut stream = store.history_plus_stream();

                while let Some(Ok(msg)) = stream.next().await {
                    match &msg {
                        LogMsg::Stdout(_) | LogMsg::Stderr(_) => {
                            // Serialize this individual message as a JSONL line
                            match serde_json::to_string(&msg) {
                                Ok(jsonl_line) => {
                                    let jsonl_line_with_newline = format!("{jsonl_line}\n");

                                    // Append this line to the database
                                    if let Err(e) = ExecutionProcessLogs::append_log_line(
                                        &db.pool,
                                        execution_id,
                                        &jsonl_line_with_newline,
                                    )
                                    .await
                                    {
                                        tracing::error!(
                                            "Failed to append log line for execution {}: {}",
                                            execution_id,
                                            e
                                        );
                                    }
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Failed to serialize log message for execution {}: {}",
                                        execution_id,
                                        e
                                    );
                                }
                            }
                        }
                        LogMsg::SessionId(agent_session_id) => {
                            // Append this line to the database
                            if let Err(e) = CodingAgentTurn::update_agent_session_id(
                                &db.pool,
                                execution_id,
                                agent_session_id,
                            )
                            .await
                            {
                                tracing::error!(
                                    "Failed to update agent_session_id {} for execution process {}: {}",
                                    agent_session_id,
                                    execution_id,
                                    e
                                );
                            }
                        }
                        LogMsg::Finished => {
                            break;
                        }
                        LogMsg::JsonPatch(_) => continue,
                    }
                }
            }
        })
    }

    fn spawn_stream_normalized_entries_to_db(&self, execution_id: &Uuid) -> JoinHandle<()> {
        let execution_id = *execution_id;
        let msg_stores = self.msg_stores().clone();
        let db = self.db().clone();

        tokio::spawn(async move {
            let store = {
                let map = msg_stores.read().await;
                map.get(&execution_id).cloned()
            };

            if let Some(store) = store {
                let mut stream = store.history_plus_stream();

                while let Some(Ok(msg)) = stream.next().await {
                    match msg {
                        LogMsg::JsonPatch(patch) => {
                            let ops = extract_normalized_entry_ops(&patch);
                            if ops.is_empty() {
                                continue;
                            }
                            if let Err(err) =
                                apply_normalized_entry_ops(&db.pool, execution_id, ops).await
                            {
                                tracing::error!(
                                    "Failed to persist normalized entries for execution {}: {}",
                                    execution_id,
                                    err
                                );
                            }
                        }
                        LogMsg::Finished => break,
                        _ => continue,
                    }
                }
            }
        })
    }

    /// Start a workspace execution, potentially queuing if at concurrency limit.
    /// Returns `StartWorkspaceResult::Queued` if execution was queued,
    /// or `StartWorkspaceResult::Started` if execution began immediately.
    async fn start_workspace(
        &self,
        workspace: &Workspace,
        executor_profile_id: ExecutorProfileId,
    ) -> Result<StartWorkspaceResult, ContainerError> {
        // Check if we should queue this execution
        if self.should_queue_execution().await? {
            tracing::info!(
                "At concurrency limit, queueing workspace {} for execution",
                workspace.id
            );
            let queue_entry =
                ExecutionQueue::create(&self.db().pool, workspace.id, &executor_profile_id).await?;
            // Note: is_queued is updated automatically via database trigger on execution_queue INSERT
            return Ok(StartWorkspaceResult::Queued(queue_entry));
        }

        // Not at limit, start immediately
        let execution_process = self
            .start_workspace_inner(workspace, executor_profile_id)
            .await?;
        Ok(StartWorkspaceResult::Started(execution_process))
    }

    /// Inner implementation of start_workspace that bypasses queue check.
    /// Used when starting from the queue (already popped) or when not at capacity.
    async fn start_workspace_inner(
        &self,
        workspace: &Workspace,
        executor_profile_id: ExecutorProfileId,
    ) -> Result<ExecutionProcess, ContainerError> {
        // Create container
        self.create(workspace).await?;

        // Get parent task
        let task = workspace
            .parent_task(&self.db().pool)
            .await?
            .ok_or(SqlxError::RowNotFound)?;

        // Get parent project
        let project = task
            .parent_project(&self.db().pool)
            .await?
            .ok_or(SqlxError::RowNotFound)?;

        let project_repos =
            ProjectRepo::find_by_project_id_with_names(&self.db().pool, project.id).await?;

        let workspace = Workspace::find_by_id(&self.db().pool, workspace.id)
            .await?
            .ok_or(SqlxError::RowNotFound)?;

        // Create a session for this workspace
        let session = Session::create(
            &self.db().pool,
            &CreateSession {
                executor: Some(executor_profile_id.executor.to_string()),
            },
            Uuid::new_v4(),
            workspace.id,
        )
        .await?;

        let prompt = task.to_prompt();

        let repos_with_setup: Vec<_> = project_repos
            .iter()
            .filter(|pr| pr.setup_script.is_some())
            .collect();

        let all_parallel = repos_with_setup.iter().all(|pr| pr.parallel_setup_script);

        let cleanup_action = self.cleanup_actions_for_repos(&project_repos);

        let working_dir = workspace
            .agent_working_dir
            .as_ref()
            .filter(|dir| !dir.is_empty())
            .cloned();

        let coding_action = ExecutorAction::new(
            ExecutorActionType::CodingAgentInitialRequest(CodingAgentInitialRequest {
                prompt,
                executor_profile_id: executor_profile_id.clone(),
                working_dir,
            }),
            cleanup_action.map(Box::new),
        );

        let execution_process = if all_parallel {
            // All parallel: start each setup independently, then start coding agent
            for repo in &repos_with_setup {
                if let Some(action) = Self::setup_action_for_repo(repo)
                    && let Err(e) = self
                        .start_execution(
                            &workspace,
                            &session,
                            &action,
                            &ExecutionProcessRunReason::SetupScript,
                            None,
                        )
                        .await
                {
                    tracing::warn!(?e, "Failed to start setup script in parallel mode");
                }
            }
            self.start_execution(
                &workspace,
                &session,
                &coding_action,
                &ExecutionProcessRunReason::CodingAgent,
                None,
            )
            .await?
        } else {
            // Any sequential: chain ALL setups → coding agent via next_action
            let main_action = Self::build_sequential_setup_chain(&repos_with_setup, coding_action);
            self.start_execution(
                &workspace,
                &session,
                &main_action,
                &ExecutionProcessRunReason::SetupScript,
                None,
            )
            .await?
        };

        Ok(execution_process)
    }

    async fn start_execution(
        &self,
        workspace: &Workspace,
        session: &Session,
        executor_action: &ExecutorAction,
        run_reason: &ExecutionProcessRunReason,
        purpose: Option<&str>,
    ) -> Result<ExecutionProcess, ContainerError> {
        // Compute effective purpose: explicit override or derived from run_reason
        let effective_purpose = purpose.unwrap_or_else(|| purpose_from_run_reason(run_reason));

        // Update task status to InProgress when starting an execution
        // Skip for DevServer and InternalAgent (internal operations should not affect task status)
        let task = workspace
            .parent_task(&self.db().pool)
            .await?
            .ok_or(SqlxError::RowNotFound)?;
        if task.status != TaskStatus::InProgress
            && !matches!(
                run_reason,
                ExecutionProcessRunReason::DevServer | ExecutionProcessRunReason::InternalAgent
            )
        {
            Task::update_status(&self.db().pool, task.id, TaskStatus::InProgress).await?;

            if let Some(publisher) = self.share_publisher()
                && let Err(err) = publisher.update_shared_task_by_id(task.id).await
            {
                tracing::warn!(
                    ?err,
                    "Failed to propagate shared task update for {}",
                    task.id
                );
            }
        }
        // Create new execution process record
        // Capture current HEAD per repository as the "before" commit for this execution
        let repositories =
            WorkspaceRepo::find_repos_for_workspace(&self.db().pool, workspace.id).await?;
        if repositories.is_empty() {
            return Err(ContainerError::Other(anyhow!(
                "Workspace has no repositories configured"
            )));
        }

        let workspace_root = workspace
            .container_ref
            .as_ref()
            .map(std::path::PathBuf::from)
            .ok_or_else(|| ContainerError::Other(anyhow!("Container ref not found")))?;

        let mut repo_states = Vec::with_capacity(repositories.len());
        for repo in &repositories {
            let repo_path = workspace_root.join(&repo.name);
            let before_head_commit = self.git().get_head_info(&repo_path).ok().map(|h| h.oid);
            repo_states.push(CreateExecutionProcessRepoState {
                repo_id: repo.id,
                before_head_commit,
                after_head_commit: None,
                merge_commit: None,
            });
        }
        let create_execution_process = CreateExecutionProcess {
            session_id: session.id,
            executor_action: executor_action.clone(),
            run_reason: run_reason.clone(),
        };

        let execution_process = ExecutionProcess::create(
            &self.db().pool,
            &create_execution_process,
            Uuid::new_v4(),
            &repo_states,
        )
        .await?;

        if let Some(prompt) = match executor_action.typ() {
            ExecutorActionType::CodingAgentInitialRequest(coding_agent_request) => {
                Some(coding_agent_request.prompt.clone())
            }
            ExecutorActionType::CodingAgentFollowUpRequest(follow_up_request) => {
                Some(follow_up_request.prompt.clone())
            }
            _ => None,
        } {
            let create_coding_agent_turn = CreateCodingAgentTurn {
                execution_process_id: execution_process.id,
                prompt: Some(prompt),
            };

            let coding_agent_turn_id = Uuid::new_v4();

            CodingAgentTurn::create(
                &self.db().pool,
                &create_coding_agent_turn,
                coding_agent_turn_id,
            )
            .await?;
        }

        if let Err(start_error) = self
            .start_execution_inner(workspace, &execution_process, executor_action, effective_purpose)
            .await
        {
            // Mark process as failed
            if let Err(update_error) = ExecutionProcess::update_completion(
                &self.db().pool,
                execution_process.id,
                ExecutionProcessStatus::Failed,
                None,
            )
            .await
            {
                tracing::error!(
                    "Failed to mark execution process {} as failed after start error: {}",
                    execution_process.id,
                    update_error
                );
            }
            Task::update_status(&self.db().pool, task.id, TaskStatus::InReview).await?;

            // Emit stderr error message
            let log_message = LogMsg::Stderr(format!("Failed to start execution: {start_error}"));
            if let Ok(json_line) = serde_json::to_string(&log_message) {
                let _ = ExecutionProcessLogs::append_log_line(
                    &self.db().pool,
                    execution_process.id,
                    &format!("{json_line}\n"),
                )
                .await;
            }

            // Emit NextAction with failure context for coding agent requests
            if let ContainerError::ExecutorError(ExecutorError::ExecutableNotFound { program }) =
                &start_error
            {
                let help_text = format!("The required executable `{program}` is not installed.");
                let error_message = NormalizedEntry {
                    timestamp: None,
                    entry_type: NormalizedEntryType::ErrorMessage {
                        error_type: NormalizedEntryError::SetupRequired,
                    },
                    content: help_text,
                    metadata: None,
                };
                let patch = ConversationPatch::add_normalized_entry(2, error_message);
                if let Ok(json_line) = serde_json::to_string::<LogMsg>(&LogMsg::JsonPatch(patch)) {
                    let _ = ExecutionProcessLogs::append_log_line(
                        &self.db().pool,
                        execution_process.id,
                        &format!("{json_line}\n"),
                    )
                    .await;
                }
            };
            return Err(start_error);
        }

        // Start processing normalised logs for executor requests and follow ups
        if let Some(msg_store) = self.get_msg_store_by_id(&execution_process.id).await
            && let Some((executor_profile_id, is_initial)) = match executor_action.typ() {
                ExecutorActionType::CodingAgentInitialRequest(request) => {
                    Some((&request.executor_profile_id, true))
                }
                ExecutorActionType::CodingAgentFollowUpRequest(request) => {
                    Some((&request.executor_profile_id, false))
                }
                _ => None,
            }
        {
            if let Some(executor) =
                ExecutorConfigs::get_cached().get_coding_agent(executor_profile_id)
            {
                // Only capture skills on initial requests (init message is only sent once)
                let skills_callback: Option<Box<dyn FnOnce(SkillsData) + Send + 'static>> =
                    if is_initial {
                        let skills_cache = self.skills_cache().clone();
                        Some(Box::new(move |skills_data| {
                            let cache = skills_cache;
                            tokio::spawn(async move {
                                cache.update_skills(skills_data).await;
                            });
                        }))
                    } else {
                        None
                    };
                executor.normalize_logs_with_skills_callback(
                    msg_store,
                    &self.workspace_to_current_dir(workspace),
                    skills_callback,
                );
            } else {
                tracing::error!(
                    "Failed to resolve profile '{:?}' for normalization",
                    executor_profile_id
                );
            }

            self.spawn_stream_normalized_entries_to_db(&execution_process.id);
        }

        self.spawn_stream_raw_logs_to_db(&execution_process.id);
        Ok(execution_process)
    }

    async fn try_start_next_action(&self, ctx: &ExecutionContext) -> Result<(), ContainerError> {
        let action = ctx.execution_process.executor_action()?;
        let next_action = if let Some(next_action) = action.next_action() {
            next_action
        } else {
            tracing::debug!("No next action configured");
            return Ok(());
        };

        // Determine the run reason of the next action
        let next_run_reason = match (action.typ(), next_action.typ()) {
            (ExecutorActionType::ScriptRequest(_), ExecutorActionType::ScriptRequest(_)) => {
                ExecutionProcessRunReason::SetupScript
            }
            (
                ExecutorActionType::CodingAgentInitialRequest(_)
                | ExecutorActionType::CodingAgentFollowUpRequest(_),
                ExecutorActionType::ScriptRequest(_),
            ) => ExecutionProcessRunReason::CleanupScript,
            (
                _,
                ExecutorActionType::CodingAgentFollowUpRequest(_)
                | ExecutorActionType::CodingAgentInitialRequest(_),
            ) => ExecutionProcessRunReason::CodingAgent,
        };

        self.start_execution(&ctx.workspace, &ctx.session, next_action, &next_run_reason, None)
            .await?;

        tracing::debug!("Started next action: {:?}", next_action);
        Ok(())
    }

    /// Start a conversation execution without git context.
    /// This creates an ExecutionProcess linked to a ConversationSession instead of a Session.
    async fn start_conversation_execution(
        &self,
        conversation_session: &ConversationSession,
        executor_action: &ExecutorAction,
    ) -> Result<ExecutionProcess, ContainerError> {
        use db::models::execution_process::CreateConversationExecutionProcess;

        let process_id = Uuid::new_v4();
        let create_data = CreateConversationExecutionProcess {
            conversation_session_id: conversation_session.id,
            executor_action: executor_action.clone(),
        };

        let execution_process =
            ExecutionProcess::create_for_conversation(&self.db().pool, &create_data, process_id)
                .await?;

        // Create coding agent turn if this is a coding agent request
        if let Some(prompt) = match executor_action.typ() {
            ExecutorActionType::CodingAgentInitialRequest(request) => Some(request.prompt.clone()),
            ExecutorActionType::CodingAgentFollowUpRequest(request) => Some(request.prompt.clone()),
            _ => None,
        } {
            let create_turn = CreateCodingAgentTurn {
                execution_process_id: execution_process.id,
                prompt: Some(prompt),
            };
            CodingAgentTurn::create(&self.db().pool, &create_turn, Uuid::new_v4()).await?;
        }

        // Start the execution using a temporary directory (no git context)
        if let Err(start_error) = self
            .start_conversation_execution_inner(&execution_process, executor_action)
            .await
        {
            // Mark process as failed
            if let Err(update_error) = ExecutionProcess::update_completion(
                &self.db().pool,
                execution_process.id,
                ExecutionProcessStatus::Failed,
                None,
            )
            .await
            {
                tracing::error!(
                    "Failed to mark conversation execution {} as failed: {}",
                    execution_process.id,
                    update_error
                );
            }

            // Emit error log
            let log_message = LogMsg::Stderr(format!("Failed to start execution: {start_error}"));
            if let Ok(json_line) = serde_json::to_string(&log_message) {
                let _ = ExecutionProcessLogs::append_log_line(
                    &self.db().pool,
                    execution_process.id,
                    &format!("{json_line}\n"),
                )
                .await;
            }

            return Err(start_error);
        }

        // Start log streaming
        if let Some(msg_store) = self.get_msg_store_by_id(&execution_process.id).await
            && let Some((executor_profile_id, is_initial)) = match executor_action.typ() {
                ExecutorActionType::CodingAgentInitialRequest(request) => {
                    Some((&request.executor_profile_id, true))
                }
                ExecutorActionType::CodingAgentFollowUpRequest(request) => {
                    Some((&request.executor_profile_id, false))
                }
                _ => None,
            }
        {
            // For conversation execution, use current working directory (temp or home)
            let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/tmp"));
            if let Some(executor) =
                ExecutorConfigs::get_cached().get_coding_agent(executor_profile_id)
            {
                // Only capture skills on initial requests (init message is only sent once)
                let skills_callback: Option<Box<dyn FnOnce(SkillsData) + Send + 'static>> =
                    if is_initial {
                        let skills_cache = self.skills_cache().clone();
                        Some(Box::new(move |skills_data| {
                            let cache = skills_cache;
                            tokio::spawn(async move {
                                cache.update_skills(skills_data).await;
                            });
                        }))
                    } else {
                        None
                    };
                executor.normalize_logs_with_skills_callback(msg_store, &current_dir, skills_callback);
            }

            self.spawn_stream_normalized_entries_to_db(&execution_process.id);
        }

        self.spawn_stream_raw_logs_to_db(&execution_process.id);

        Ok(execution_process)
    }

    /// Start conversation execution without workspace/git context.
    /// Used for disposable conversations that don't need git integration.
    async fn start_conversation_execution_inner(
        &self,
        execution_process: &ExecutionProcess,
        executor_action: &ExecutorAction,
    ) -> Result<(), ContainerError>;
}
