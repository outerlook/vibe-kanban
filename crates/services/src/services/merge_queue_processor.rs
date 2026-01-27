//! Merge Queue Processor Service
//!
//! Processes entries in the merge queue for a project, orchestrating:
//! rebase → merge, handling conflicts by skipping to next task.

use std::path::Path;
use std::sync::Arc;

use db::models::{
    execution_queue::ExecutionQueue,
    merge::Merge,
    repo::Repo,
    session::Session,
    task::{Task, TaskStatus},
    workspace::Workspace,
    workspace_repo::WorkspaceRepo,
};
use executors::profile::ExecutorProfileId;
use sqlx::SqlitePool;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::autopilot;
use super::config::Config;
use super::git::{GitService, GitServiceError};
use super::merge_queue_store::{MergeQueueEntry, MergeQueueStore};
use super::operation_status::{OperationStatus, OperationStatusStore, OperationStatusType};

/// Errors that can occur during merge queue processing
#[derive(Debug, Error)]
pub enum MergeQueueError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),

    #[error(transparent)]
    Git(#[from] GitServiceError),

    #[error("Workspace not found: {0}")]
    WorkspaceNotFound(Uuid),

    #[error("Repo not found: {0}")]
    RepoNotFound(Uuid),

    #[error("Task not found: {0}")]
    TaskNotFound(Uuid),

    #[error("WorkspaceRepo not found for workspace {0} and repo {1}")]
    WorkspaceRepoNotFound(Uuid, Uuid),

    #[error("Merge conflict: {0}")]
    MergeConflict(String),

    #[error("Rebase conflict: {0}")]
    RebaseConflict(String),
}

impl MergeQueueError {
    /// Returns true if this error represents a conflict (rebase or merge)
    pub fn is_conflict(&self) -> bool {
        matches!(
            self,
            MergeQueueError::MergeConflict(_) | MergeQueueError::RebaseConflict(_)
        )
    }

    /// Returns the conflict message if this is a conflict error
    pub fn conflict_message(&self) -> Option<&str> {
        match self {
            MergeQueueError::MergeConflict(msg) | MergeQueueError::RebaseConflict(msg) => Some(msg),
            _ => None,
        }
    }
}

/// Service for processing merge queue entries
pub struct MergeQueueProcessor {
    pool: SqlitePool,
    git: GitService,
    merge_queue_store: MergeQueueStore,
    operation_status: Option<OperationStatusStore>,
    config: Arc<RwLock<Config>>,
}

impl MergeQueueProcessor {
    /// Create a new MergeQueueProcessor
    pub fn new(
        pool: SqlitePool,
        git: GitService,
        merge_queue_store: MergeQueueStore,
        config: Arc<RwLock<Config>>,
    ) -> Self {
        Self {
            pool,
            git,
            merge_queue_store,
            operation_status: None,
            config,
        }
    }

    /// Create a new MergeQueueProcessor with operation status tracking
    pub fn with_operation_status(
        pool: SqlitePool,
        git: GitService,
        merge_queue_store: MergeQueueStore,
        operation_status: OperationStatusStore,
        config: Arc<RwLock<Config>>,
    ) -> Self {
        Self {
            pool,
            git,
            merge_queue_store,
            operation_status: Some(operation_status),
            config,
        }
    }

    /// Process all queued entries for a project until the queue is empty.
    ///
    /// This method loops through the queue, processing each entry:
    /// 1. Claims the next queued entry (updates status to 'merging')
    /// 2. Performs rebase to update task branch with base branch changes
    /// 3. Uses pre-populated commit message
    /// 4. Performs the merge
    ///
    /// On conflict, the entry is removed and processing continues with the next entry.
    pub async fn process_project_queue(&self, project_id: Uuid) -> Result<(), MergeQueueError> {
        info!(%project_id, "Starting merge queue processing");

        loop {
            // Claim the next queued entry from the in-memory store
            let entry = match self.merge_queue_store.claim_next(project_id) {
                Some(entry) => entry,
                None => {
                    info!(%project_id, "Merge queue empty, processing complete");
                    return Ok(());
                }
            };

            info!(
                entry_id = %entry.id,
                workspace_id = %entry.workspace_id,
                repo_id = %entry.repo_id,
                "Processing merge queue entry"
            );

            // Set Merging operation status (load workspace to get task_id)
            if let Some(ref op_status) = self.operation_status {
                if let Ok(Some(workspace)) =
                    Workspace::find_by_id(&self.pool, entry.workspace_id).await
                {
                    op_status.set(OperationStatus::new(
                        entry.workspace_id,
                        workspace.task_id,
                        OperationStatusType::Merging,
                    ));
                }
            }

            // Process this entry, handling errors gracefully
            let result = self.process_entry(&entry).await;

            // Clear operation status after processing (success or failure)
            if let Some(ref op_status) = self.operation_status {
                op_status.clear(entry.workspace_id);
            }

            match result {
                Ok(merge_commit) => {
                    info!(
                        entry_id = %entry.id,
                        %merge_commit,
                        "Merge completed successfully"
                    );
                    // Entry already removed in process_entry
                }
                Err(e) if e.is_conflict() => {
                    warn!(
                        entry_id = %entry.id,
                        error = %e,
                        "Merge queue entry has conflicts, removing entry"
                    );
                    self.merge_queue_store.remove(entry.workspace_id);
                    // Continue to next entry
                }
                Err(e) => {
                    error!(
                        entry_id = %entry.id,
                        error = %e,
                        "Unexpected error processing merge queue entry, removing entry"
                    );
                    self.merge_queue_store.remove(entry.workspace_id);
                    // Continue to next entry
                }
            }
        }
    }

    /// Process a single merge queue entry
    ///
    /// Returns the merge commit SHA on success
    async fn process_entry(&self, entry: &MergeQueueEntry) -> Result<String, MergeQueueError> {
        // Load required entities
        let workspace = Workspace::find_by_id(&self.pool, entry.workspace_id)
            .await?
            .ok_or(MergeQueueError::WorkspaceNotFound(entry.workspace_id))?;

        let repo = Repo::find_by_id(&self.pool, entry.repo_id)
            .await?
            .ok_or(MergeQueueError::RepoNotFound(entry.repo_id))?;

        let task = Task::find_by_id(&self.pool, workspace.task_id)
            .await?
            .ok_or(MergeQueueError::TaskNotFound(workspace.task_id))?;

        let workspace_repo =
            WorkspaceRepo::find_by_workspace_and_repo_id(&self.pool, workspace.id, repo.id)
                .await?
                .ok_or(MergeQueueError::WorkspaceRepoNotFound(workspace.id, repo.id))?;

        // Get paths
        let repo_path = &repo.path;
        let container_ref = workspace.container_ref.as_ref().ok_or_else(|| {
            MergeQueueError::WorkspaceNotFound(workspace.id) // No container_ref means no worktree
        })?;
        let worktree_path = std::path::PathBuf::from(container_ref).join(&repo.name);

        let task_branch = &workspace.branch;
        let base_branch = &workspace_repo.target_branch;

        info!(
            workspace_id = %workspace.id,
            repo_path = %repo_path.display(),
            worktree_path = %worktree_path.display(),
            task_branch = %task_branch,
            base_branch = %base_branch,
            "Executing merge for workspace"
        );

        // Step 1: Rebase task branch onto base branch
        self.rebase_if_needed(repo_path, &worktree_path, base_branch, task_branch)
            .await?;

        // Step 2: Use commit message from entry (always populated at enqueue time)
        let commit_message = &entry.commit_message;

        // Step 3: Merge changes
        let merge_commit = self
            .merge_changes(repo_path, &worktree_path, task_branch, base_branch, commit_message)
            .await?;

        // Step 4: Remove the queue entry (completed successfully)
        self.merge_queue_store.remove(entry.workspace_id);

        // Step 5: Create merge record
        Merge::create_direct(&self.pool, workspace.id, repo.id, base_branch, &merge_commit).await?;

        // Step 6: Update task status to Done
        Task::update_status(&self.pool, task.id, TaskStatus::Done).await?;

        // Note: Agent feedback collection is not done here because:
        // 1. MergeQueueProcessor doesn't have access to ContainerService
        // 2. Feedback is typically collected when merge is triggered via HTTP endpoints
        // 3. The agent session may have expired by the time the queue processes

        info!(
            task_id = %task.id,
            "Task marked as Done after successful merge"
        );

        // Step 7: Auto-dequeue unblocked dependents if autopilot is enabled
        // Note: Enqueued tasks will be picked up by container's process_queue when
        // the next execution completes or when any new execution is requested.
        self.auto_dequeue_unblocked_dependents(task.id).await;

        Ok(merge_commit)
    }

    /// Auto-dequeue unblocked dependent tasks when autopilot is enabled.
    ///
    /// After a task is marked as Done, this method:
    /// 1. Checks if autopilot is enabled in config
    /// 2. Finds all tasks that depend on the completed task and are now unblocked
    /// 3. For each unblocked task that has a workspace, queues it for execution
    ///
    /// Returns the number of tasks that were enqueued.
    async fn auto_dequeue_unblocked_dependents(&self, completed_task_id: Uuid) -> usize {
        // Check if autopilot is enabled
        let autopilot_enabled = self.config.read().await.autopilot_enabled;
        if !autopilot_enabled {
            debug!(
                task_id = %completed_task_id,
                "Autopilot disabled, skipping auto-dequeue of dependents"
            );
            return 0;
        }

        // Find unblocked dependent tasks
        let unblocked_tasks = match autopilot::find_unblocked_dependents(&self.pool, completed_task_id)
            .await
        {
            Ok(tasks) => tasks,
            Err(e) => {
                error!(
                    task_id = %completed_task_id,
                    error = %e,
                    "Failed to find unblocked dependents"
                );
                return 0;
            }
        };

        if unblocked_tasks.is_empty() {
            debug!(
                task_id = %completed_task_id,
                "No unblocked dependent tasks to auto-dequeue"
            );
            return 0;
        }

        info!(
            completed_task_id = %completed_task_id,
            unblocked_count = unblocked_tasks.len(),
            "Auto-dequeueing unblocked dependent tasks"
        );

        let mut enqueued_count = 0;

        for unblocked_task in unblocked_tasks {
            // Find the latest workspace for this task
            let workspace = match Workspace::find_latest_by_task_id(&self.pool, unblocked_task.id)
                .await
            {
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
                .get_executor_profile_for_workspace(workspace.id)
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
            match ExecutionQueue::create(&self.pool, workspace.id, &executor_profile_id).await {
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

        enqueued_count
    }

    /// Get the executor profile ID from the latest session of a workspace.
    ///
    /// Returns None if no session exists or if the executor string cannot be parsed.
    async fn get_executor_profile_for_workspace(
        &self,
        workspace_id: Uuid,
    ) -> Option<ExecutorProfileId> {
        let session = Session::find_latest_by_workspace_id(&self.pool, workspace_id)
            .await
            .ok()??;

        // The executor field is stored as a JSON string
        let executor_str = session.executor.as_ref()?;
        serde_json::from_str(executor_str).ok()
    }

    /// Rebase the task branch onto the base branch if needed
    async fn rebase_if_needed(
        &self,
        repo_path: &Path,
        worktree_path: &Path,
        base_branch: &str,
        task_branch: &str,
    ) -> Result<(), MergeQueueError> {
        // Check if rebase is needed by comparing branch positions
        let (ahead, behind) = self
            .git
            .get_branch_status(repo_path, task_branch, base_branch)?;

        if behind == 0 {
            info!(
                %task_branch,
                %base_branch,
                "Task branch is up-to-date with base, no rebase needed"
            );
            return Ok(());
        }

        info!(
            %task_branch,
            %base_branch,
            commits_behind = behind,
            commits_ahead = ahead,
            "Rebasing task branch onto base branch"
        );

        // Perform the rebase
        match self
            .git
            .rebase_branch(repo_path, worktree_path, base_branch, base_branch, task_branch)
        {
            Ok(_) => Ok(()),
            Err(GitServiceError::MergeConflicts(msg)) => {
                Err(MergeQueueError::RebaseConflict(msg))
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Perform the merge
    async fn merge_changes(
        &self,
        repo_path: &Path,
        worktree_path: &Path,
        task_branch: &str,
        base_branch: &str,
        commit_message: &str,
    ) -> Result<String, MergeQueueError> {
        match self.git.merge_changes(
            repo_path,      // base_worktree_path (main repo)
            worktree_path,  // task_worktree_path
            task_branch,
            base_branch,
            commit_message,
        ) {
            Ok(commit_sha) => Ok(commit_sha),
            Err(GitServiceError::MergeConflicts(msg)) => Err(MergeQueueError::MergeConflict(msg)),
            Err(GitServiceError::BranchesDiverged(msg)) => {
                // If branches diverged after rebase, treat as conflict
                Err(MergeQueueError::MergeConflict(format!(
                    "Branches diverged: {}",
                    msg
                )))
            }
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_queue_error_is_conflict() {
        assert!(MergeQueueError::MergeConflict("test".to_string()).is_conflict());
        assert!(MergeQueueError::RebaseConflict("test".to_string()).is_conflict());
        assert!(!MergeQueueError::TaskNotFound(Uuid::new_v4()).is_conflict());
        assert!(!MergeQueueError::RepoNotFound(Uuid::new_v4()).is_conflict());
    }

    #[test]
    fn test_merge_queue_error_conflict_message() {
        let merge_err = MergeQueueError::MergeConflict("merge conflict details".to_string());
        assert_eq!(
            merge_err.conflict_message(),
            Some("merge conflict details")
        );

        let rebase_err = MergeQueueError::RebaseConflict("rebase conflict details".to_string());
        assert_eq!(
            rebase_err.conflict_message(),
            Some("rebase conflict details")
        );

        let other_err = MergeQueueError::TaskNotFound(Uuid::new_v4());
        assert_eq!(other_err.conflict_message(), None);
    }
}
