//! AutopilotService for managing autopilot business logic.
//!
//! This service determines when tasks qualify for auto-merge and finds
//! dependent tasks that become unblocked after a task completes.

use db::models::{
    task::{Task, TaskStatus},
    task_dependency::TaskDependency,
};
use sqlx::SqlitePool;
use tracing::{debug, info};
use uuid::Uuid;

/// Service for autopilot decision-making and dependency management.
#[derive(Clone, Default)]
pub struct AutopilotService;

impl AutopilotService {
    pub fn new() -> Self {
        Self
    }

    /// Determine if a task should be auto-merged.
    ///
    /// A task qualifies for auto-merge when:
    /// - Autopilot is enabled for the project
    /// - Task status is InReview
    /// - Task does not need attention (needs_attention is false or None)
    ///
    /// # Arguments
    /// * `task` - The task to evaluate
    /// * `autopilot_enabled` - Whether autopilot is enabled for the project
    ///
    /// # Returns
    /// `true` if the task should be auto-merged, `false` otherwise
    pub fn should_auto_merge(task: &Task, autopilot_enabled: bool) -> bool {
        if !autopilot_enabled {
            debug!(
                task_id = %task.id,
                "Autopilot disabled, skipping auto-merge"
            );
            return false;
        }

        if task.status != TaskStatus::InReview {
            debug!(
                task_id = %task.id,
                status = %task.status,
                "Task not in review status, skipping auto-merge"
            );
            return false;
        }

        // needs_attention = Some(true) means review flagged issues
        // needs_attention = Some(false) or None means good to merge
        if task.needs_attention == Some(true) {
            debug!(
                task_id = %task.id,
                "Task needs attention, skipping auto-merge"
            );
            return false;
        }

        info!(
            task_id = %task.id,
            "Task qualifies for auto-merge"
        );
        true
    }

    /// Find tasks that depend on the completed task and are now unblocked.
    ///
    /// Returns tasks where:
    /// - A dependency on the given task_id exists
    /// - is_blocked = false (all dependencies now complete)
    /// - status = Todo (ready to be started)
    ///
    /// # Arguments
    /// * `pool` - Database connection pool
    /// * `task_id` - The ID of the task that was completed
    ///
    /// # Returns
    /// List of tasks that are now unblocked and ready to start
    pub async fn find_unblocked_dependents(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Vec<Task>, sqlx::Error> {
        // Get all tasks that depend on this task
        let dependent_tasks = TaskDependency::find_blocking(pool, task_id).await?;

        // Filter to only unblocked Todo tasks
        let unblocked: Vec<Task> = dependent_tasks
            .into_iter()
            .filter(|task| !task.is_blocked && task.status == TaskStatus::Todo)
            .collect();

        if !unblocked.is_empty() {
            info!(
                completed_task_id = %task_id,
                unblocked_count = unblocked.len(),
                "Found unblocked dependent tasks"
            );
        } else {
            debug!(
                completed_task_id = %task_id,
                "No unblocked dependent tasks found"
            );
        }

        Ok(unblocked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_test_task(status: TaskStatus, needs_attention: Option<bool>) -> Task {
        Task {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            title: "Test task".to_string(),
            description: None,
            status,
            parent_workspace_id: None,
            shared_task_id: None,
            task_group_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            is_blocked: false,
            has_in_progress_attempt: false,
            last_attempt_failed: false,
            is_queued: false,
            last_executor: String::new(),
            needs_attention,
        }
    }

    #[test]
    fn test_should_auto_merge_autopilot_disabled() {
        let task = create_test_task(TaskStatus::InReview, Some(false));
        assert!(!AutopilotService::should_auto_merge(&task, false));
    }

    #[test]
    fn test_should_auto_merge_needs_attention_true() {
        let task = create_test_task(TaskStatus::InReview, Some(true));
        assert!(!AutopilotService::should_auto_merge(&task, true));
    }

    #[test]
    fn test_should_auto_merge_needs_attention_false() {
        let task = create_test_task(TaskStatus::InReview, Some(false));
        assert!(AutopilotService::should_auto_merge(&task, true));
    }

    #[test]
    fn test_should_auto_merge_needs_attention_none() {
        // When review attention is disabled, needs_attention is None
        let task = create_test_task(TaskStatus::InReview, None);
        assert!(AutopilotService::should_auto_merge(&task, true));
    }

    #[test]
    fn test_should_auto_merge_status_todo() {
        let task = create_test_task(TaskStatus::Todo, Some(false));
        assert!(!AutopilotService::should_auto_merge(&task, true));
    }

    #[test]
    fn test_should_auto_merge_status_in_progress() {
        let task = create_test_task(TaskStatus::InProgress, Some(false));
        assert!(!AutopilotService::should_auto_merge(&task, true));
    }

    #[test]
    fn test_should_auto_merge_status_done() {
        let task = create_test_task(TaskStatus::Done, Some(false));
        assert!(!AutopilotService::should_auto_merge(&task, true));
    }

    #[test]
    fn test_should_auto_merge_status_cancelled() {
        let task = create_test_task(TaskStatus::Cancelled, Some(false));
        assert!(!AutopilotService::should_auto_merge(&task, true));
    }
}
