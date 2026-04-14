//! Merge Queue Processor Service
//!
//! Processes entries in the merge queue for a project, orchestrating:
//! rebase → merge, handling conflicts by skipping to next task.

use std::{path::Path, sync::Arc};

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

use super::{
    config::Config,
    domain_events::{DomainEvent, EventDispatchCallback},
    git::{GitService, GitServiceError},
    merge_queue_store::{MergeQueueEntry, MergeQueueStore},
    operation_status::{OperationStatus, OperationStatusStore, OperationStatusType},
};

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
    event_dispatcher: Option<EventDispatchCallback>,
}

impl MergeQueueProcessor {
    /// Create a new MergeQueueProcessor
    pub fn new(
        pool: SqlitePool,
        git: GitService,
        merge_queue_store: MergeQueueStore,
        _config: Arc<RwLock<Config>>,
    ) -> Self {
        Self {
            pool,
            git,
            merge_queue_store,
            operation_status: None,
            event_dispatcher: None,
        }
    }

    /// Create a new MergeQueueProcessor with operation status tracking
    pub fn with_operation_status(
        pool: SqlitePool,
        git: GitService,
        merge_queue_store: MergeQueueStore,
        operation_status: OperationStatusStore,
        _config: Arc<RwLock<Config>>,
    ) -> Self {
        Self {
            pool,
            git,
            merge_queue_store,
            operation_status: Some(operation_status),
            event_dispatcher: None,
        }
    }

    /// Set the event dispatcher callback for dispatching domain events.
    pub fn with_event_dispatcher(mut self, dispatcher: EventDispatchCallback) -> Self {
        self.event_dispatcher = Some(dispatcher);
        self
    }

    async fn dispatch_merge_queue_transition(
        &self,
        entry: &MergeQueueEntry,
        task: Option<&Task>,
        state: super::domain_events::MergeQueueTransitionState,
        merge_commit: Option<String>,
        detail: Option<String>,
    ) {
        let Some(dispatcher) = &self.event_dispatcher else {
            return;
        };

        dispatcher(DomainEvent::MergeQueueTransition {
            entry_id: entry.id,
            project_id: entry.project_id,
            workspace_id: entry.workspace_id,
            task_id: task.map(|task| task.id),
            task_group_id: task.and_then(|task| task.task_group_id),
            repo_id: entry.repo_id,
            state,
            merge_commit,
            detail,
            occurred_at: chrono::Utc::now(),
        })
        .await;
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

            self.dispatch_merge_queue_transition(
                &entry,
                None,
                super::domain_events::MergeQueueTransitionState::Claimed,
                None,
                None,
            )
            .await;

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
                    self.dispatch_merge_queue_transition(
                        &entry,
                        None,
                        super::domain_events::MergeQueueTransitionState::Conflict,
                        None,
                        e.conflict_message().map(str::to_string),
                    )
                    .await;
                    // Continue to next entry
                }
                Err(e) => {
                    error!(
                        entry_id = %entry.id,
                        error = %e,
                        "Unexpected error processing merge queue entry, removing entry"
                    );
                    self.merge_queue_store.remove(entry.workspace_id);
                    self.dispatch_merge_queue_transition(
                        &entry,
                        None,
                        super::domain_events::MergeQueueTransitionState::Removed,
                        None,
                        Some(e.to_string()),
                    )
                    .await;
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
                .ok_or(MergeQueueError::WorkspaceRepoNotFound(
                    workspace.id,
                    repo.id,
                ))?;

        // Get paths
        let repo_path = &repo.path;
        let container_ref = workspace.container_ref.as_ref().ok_or_else(|| {
            MergeQueueError::WorkspaceNotFound(workspace.id) // No container_ref means no worktree
        })?;
        let worktree_path = std::path::PathBuf::from(container_ref).join(&repo.name);

        let task_branch = &workspace.branch;
        let base_branch = &workspace_repo.target_branch;

        // Check if there are commits to merge before proceeding
        let (ahead, _) = self
            .git
            .get_branch_status(repo_path, task_branch, base_branch)?;
        if ahead == 0 {
            info!(
                workspace_id = %workspace.id,
                %task_branch,
                %base_branch,
                "Nothing to merge (0 commits ahead), removing from queue"
            );
            self.merge_queue_store.remove(entry.workspace_id);
            self.dispatch_merge_queue_transition(
                entry,
                Some(&task),
                super::domain_events::MergeQueueTransitionState::Skipped,
                None,
                Some("nothing_to_merge".to_string()),
            )
            .await;
            return Ok("skipped:nothing_to_merge".to_string());
        }

        info!(
            workspace_id = %workspace.id,
            repo_path = %repo_path.display(),
            worktree_path = %worktree_path.display(),
            task_branch = %task_branch,
            base_branch = %base_branch,
            commits_ahead = ahead,
            "Executing merge for workspace"
        );

        // Step 1: Rebase task branch onto base branch
        self.rebase_if_needed(repo_path, &worktree_path, base_branch, task_branch)
            .await?;

        // Step 2: Use commit message from entry (always populated at enqueue time)
        let commit_message = &entry.commit_message;

        // Step 3: Merge changes
        let merge_commit = self
            .merge_changes(
                repo_path,
                &worktree_path,
                task_branch,
                base_branch,
                commit_message,
            )
            .await?;

        self.complete_successful_entry(entry, &workspace, &repo, &task, base_branch, merge_commit)
            .await
    }

    async fn complete_successful_entry(
        &self,
        entry: &MergeQueueEntry,
        workspace: &Workspace,
        repo: &Repo,
        task: &Task,
        base_branch: &str,
        merge_commit: String,
    ) -> Result<String, MergeQueueError> {
        self.merge_queue_store.remove(entry.workspace_id);

        Merge::create_direct(
            &self.pool,
            workspace.id,
            repo.id,
            base_branch,
            &merge_commit,
        )
        .await?;

        let previous_status = task.status.clone();
        Task::update_status(&self.pool, task.id, TaskStatus::Done).await?;

        info!(
            task_id = %task.id,
            "Task marked as Done after successful merge"
        );

        self.dispatch_merge_queue_transition(
            entry,
            Some(task),
            super::domain_events::MergeQueueTransitionState::Completed,
            Some(merge_commit.clone()),
            None,
        )
        .await;

        if let Some(dispatcher) = &self.event_dispatcher {
            let mut updated_task = task.clone();
            updated_task.status = TaskStatus::Done;
            dispatcher(DomainEvent::TaskStatusChanged {
                task: updated_task,
                previous_status,
            })
            .await;
        }

        Ok(merge_commit)
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
        match self.git.rebase_branch(
            repo_path,
            worktree_path,
            base_branch,
            base_branch,
            task_branch,
        ) {
            Ok(_) => Ok(()),
            Err(GitServiceError::MergeConflicts(msg)) => Err(MergeQueueError::RebaseConflict(msg)),
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
            repo_path,     // base_worktree_path (main repo)
            worktree_path, // task_worktree_path
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
    use std::sync::Arc;

    use db::{
        DBService,
        models::{
            project::{CreateProject, Project},
            repo::Repo,
            task::{CreateTask, Task, TaskStatus},
            workspace::{CreateWorkspace, Workspace},
        },
    };
    use tempfile::TempDir;
    use utils::msg_store::MsgStore;

    use super::*;
    use crate::services::domain_events::{
        EventDispatchCallback, OrchestrationEventMapper, OrchestrationEventPublisher,
        OrchestrationEventType, RecordingOrchestrationEventPublisher, default_topic_namespace,
    };

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
        assert_eq!(merge_err.conflict_message(), Some("merge conflict details"));

        let rebase_err = MergeQueueError::RebaseConflict("rebase conflict details".to_string());
        assert_eq!(
            rebase_err.conflict_message(),
            Some("rebase conflict details")
        );

        let other_err = MergeQueueError::TaskNotFound(Uuid::new_v4());
        assert_eq!(other_err.conflict_message(), None);
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
                    let event_name = serde_json::to_string(&envelope.event_type)
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

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn merge_queue_completion_emits_completed_transition() {
        let _lock = crate::services::TEST_DB_LOCK.lock().expect("lock poisoned");
        let db = DBService::new().await.expect("db service");
        let publisher = RecordingOrchestrationEventPublisher::default();
        let store = MergeQueueStore::new(Arc::new(MsgStore::new()));
        let processor = MergeQueueProcessor::new(
            db.pool.clone(),
            GitService::new(),
            store.clone(),
            Arc::new(RwLock::new(Config::default())),
        )
        .with_event_dispatcher(recording_dispatcher(&db, publisher.clone()));

        let tempdir = TempDir::new().expect("tempdir");
        let repo_path = tempdir.path().join("repo");
        std::fs::create_dir_all(&repo_path).expect("create repo dir");

        let project = Project::create(
            &db.pool,
            &CreateProject {
                name: "merge-queue-events".to_string(),
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
                title: "merge-me".to_string(),
                description: None,
                status: Some(TaskStatus::InProgress),
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
                branch: "feature/merge-me".to_string(),
                agent_working_dir: None,
            },
            Uuid::new_v4(),
            task.id,
        )
        .await
        .expect("create workspace");

        let repo = Repo::find_or_create(&db.pool, &repo_path, "repo")
            .await
            .expect("create repo");

        let entry = store.enqueue(
            project.id,
            workspace.id,
            repo.id,
            "Merge feature branch".to_string(),
        );

        processor
            .complete_successful_entry(
                &entry,
                &workspace,
                &repo,
                &task,
                "main",
                "abc123".to_string(),
            )
            .await
            .expect("complete merge entry");

        assert!(
            store.get(workspace.id).is_none(),
            "queue entry should be removed"
        );
        let updated_task = Task::find_by_id(&db.pool, task.id)
            .await
            .expect("load task")
            .expect("task exists");
        assert_eq!(updated_task.status, TaskStatus::Done);
        assert_eq!(
            Merge::find_by_workspace_id(&db.pool, workspace.id)
                .await
                .expect("load merges")
                .len(),
            1
        );

        let event_types = publisher
            .published()
            .into_iter()
            .map(|(_, envelope)| envelope.event_type)
            .collect::<Vec<_>>();
        assert!(event_types.contains(&OrchestrationEventType::MergeQueueTransition));
        assert!(event_types.contains(&OrchestrationEventType::TaskStatusChanged));
    }
}
