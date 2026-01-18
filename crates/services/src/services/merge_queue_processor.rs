//! Merge Queue Processor Service
//!
//! Processes entries in the merge queue for a project, orchestrating:
//! rebase → generate commit message → merge, handling conflicts by skipping to next task.

use std::path::Path;

use db::models::{
    merge::Merge,
    merge_queue::{MergeQueue, MergeQueueStatus},
    repo::Repo,
    task::{Task, TaskStatus},
    workspace::Workspace,
    workspace_repo::WorkspaceRepo,
};
use sqlx::SqlitePool;
use thiserror::Error;
use tracing::{error, info, warn};
use uuid::Uuid;

use super::git::{GitService, GitServiceError};

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
}

impl MergeQueueProcessor {
    /// Create a new MergeQueueProcessor
    pub fn new(pool: SqlitePool, git: GitService) -> Self {
        Self { pool, git }
    }

    /// Process all queued entries for a project until the queue is empty.
    ///
    /// This method loops through the queue, processing each entry:
    /// 1. Claims the next queued entry (updates status to 'merging')
    /// 2. Performs rebase to update task branch with base branch changes
    /// 3. Generates a commit message based on task info
    /// 4. Performs the merge
    /// 5. Updates status to 'completed' or 'conflict'
    ///
    /// On conflict, the entry is marked with 'conflict' status and processing
    /// continues with the next entry.
    pub async fn process_project_queue(&self, project_id: Uuid) -> Result<(), MergeQueueError> {
        info!(%project_id, "Starting merge queue processing");

        loop {
            // Claim the next queued entry
            let entry = match MergeQueue::claim_next(&self.pool, project_id).await? {
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

            // Process this entry, handling errors gracefully
            match self.process_entry(&entry).await {
                Ok(merge_commit) => {
                    info!(
                        entry_id = %entry.id,
                        %merge_commit,
                        "Merge completed successfully"
                    );
                    // Status already updated in process_entry
                }
                Err(e) if e.is_conflict() => {
                    let conflict_msg = e.conflict_message().unwrap_or("Unknown conflict");
                    warn!(
                        entry_id = %entry.id,
                        error = %e,
                        "Merge queue entry has conflicts, skipping"
                    );
                    MergeQueue::update_status(
                        &self.pool,
                        entry.id,
                        MergeQueueStatus::Conflict,
                        Some(conflict_msg),
                    )
                    .await?;
                    // Continue to next entry
                }
                Err(e) => {
                    error!(
                        entry_id = %entry.id,
                        error = %e,
                        "Unexpected error processing merge queue entry"
                    );
                    // Mark as conflict with error details so it's not stuck
                    MergeQueue::update_status(
                        &self.pool,
                        entry.id,
                        MergeQueueStatus::Conflict,
                        Some(&e.to_string()),
                    )
                    .await?;
                    // Continue to next entry
                }
            }
        }
    }

    /// Process a single merge queue entry
    ///
    /// Returns the merge commit SHA on success
    async fn process_entry(&self, entry: &MergeQueue) -> Result<String, MergeQueueError> {
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
        let worktree_path = workspace
            .container_ref
            .as_ref()
            .map(std::path::PathBuf::from)
            .ok_or_else(|| {
                MergeQueueError::WorkspaceNotFound(workspace.id) // No container_ref means no worktree
            })?;

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

        // Step 2: Use stored commit message or generate one
        let commit_message = entry
            .commit_message
            .clone()
            .unwrap_or_else(|| self.generate_commit_message(&task, base_branch));

        // Step 3: Merge changes
        let merge_commit = self
            .merge_changes(repo_path, &worktree_path, task_branch, base_branch, &commit_message)
            .await?;

        // Step 4: Update status to completed
        MergeQueue::update_status(&self.pool, entry.id, MergeQueueStatus::Completed, None).await?;

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

    /// Generate a commit message for the merge
    fn generate_commit_message(&self, task: &Task, base_branch: &str) -> String {
        let description = task
            .description
            .as_deref()
            .filter(|d| !d.is_empty())
            .map(|d| format!("\n\n{}", d))
            .unwrap_or_default();

        format!(
            "Merge: {}{}\n\nMerged to {} via merge queue",
            task.title, description, base_branch
        )
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

    /// Helper function to generate commit message without needing a full processor
    fn generate_commit_message_for_test(task: &Task, base_branch: &str) -> String {
        let description = task
            .description
            .as_deref()
            .filter(|d| !d.is_empty())
            .map(|d| format!("\n\n{}", d))
            .unwrap_or_default();

        format!(
            "Merge: {}{}\n\nMerged to {} via merge queue",
            task.title, description, base_branch
        )
    }

    #[test]
    fn test_generate_commit_message_with_description() {
        let task = Task {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            title: "Add new feature".to_string(),
            description: Some("This adds a great new feature".to_string()),
            status: TaskStatus::InProgress,
            parent_workspace_id: None,
            shared_task_id: None,
            task_group_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            is_blocked: false,
            has_in_progress_attempt: false,
            last_attempt_failed: false,
            is_queued: false,
            last_executor: String::new(),
        };

        let message = generate_commit_message_for_test(&task, "main");
        assert!(message.contains("Add new feature"));
        assert!(message.contains("This adds a great new feature"));
        assert!(message.contains("Merged to main via merge queue"));
    }

    #[test]
    fn test_generate_commit_message_without_description() {
        let task = Task {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            title: "Fix bug".to_string(),
            description: None,
            status: TaskStatus::InProgress,
            parent_workspace_id: None,
            shared_task_id: None,
            task_group_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            is_blocked: false,
            has_in_progress_attempt: false,
            last_attempt_failed: false,
            is_queued: false,
            last_executor: String::new(),
        };

        let message = generate_commit_message_for_test(&task, "develop");
        assert!(message.contains("Fix bug"));
        assert!(!message.contains("\n\nThis")); // No description block
        assert!(message.contains("Merged to develop via merge queue"));
    }

    #[test]
    fn test_generate_commit_message_with_empty_description() {
        let task = Task {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            title: "Update docs".to_string(),
            description: Some("".to_string()), // Empty string
            status: TaskStatus::InProgress,
            parent_workspace_id: None,
            shared_task_id: None,
            task_group_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            is_blocked: false,
            has_in_progress_attempt: false,
            last_attempt_failed: false,
            is_queued: false,
            last_executor: String::new(),
        };

        let message = generate_commit_message_for_test(&task, "main");
        assert!(message.contains("Update docs"));
        // Empty description should be treated same as None
        assert!(!message.contains("\n\n\n")); // No double newlines from empty description
        assert!(message.contains("Merged to main via merge queue"));
    }
}
