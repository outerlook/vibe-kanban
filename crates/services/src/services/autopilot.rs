//! Autopilot utilities for dependency management.
//!
//! This module provides functions to find dependent tasks that become unblocked
//! after a task completes.

use db::models::{task::Task, task::TaskStatus, task_dependency::TaskDependency};
use sqlx::SqlitePool;
use tracing::{debug, info};
use uuid::Uuid;

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
