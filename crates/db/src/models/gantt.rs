use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ts_rs::TS;
use uuid::Uuid;

use super::task::TaskStatus;

/// Represents a task for Gantt chart visualization
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct GanttTask {
    pub id: Uuid,
    pub name: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub progress: f32,
    pub dependencies: Vec<Uuid>,
    pub task_status: TaskStatus,
}

impl GanttTask {
    /// Find all tasks for a project with their dependencies and execution timeline data
    /// optimized for Gantt visualization.
    ///
    /// Progress is calculated from task status:
    /// - done = 100%
    /// - inprogress = 50%
    /// - todo/inreview/cancelled = 0%
    ///
    /// Start/end dates use execution process times when available,
    /// falling back to task created_at/updated_at.
    pub async fn find_by_project_id(
        pool: &SqlitePool,
        project_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        // Query tasks with their execution timeline and dependencies
        let records = sqlx::query!(
            r#"
            SELECT
                t.id AS "id!: Uuid",
                t.title AS "name!",
                t.status AS "task_status!: TaskStatus",
                t.created_at AS "created_at!: DateTime<Utc>",
                t.updated_at AS "updated_at!: DateTime<Utc>",
                -- Get earliest started_at from any execution process for this task
                (
                    SELECT MIN(ep.started_at)
                    FROM workspaces w
                    JOIN sessions s ON s.workspace_id = w.id
                    JOIN execution_processes ep ON ep.session_id = s.id
                    WHERE w.task_id = t.id
                      AND ep.run_reason IN ('setupscript', 'cleanupscript', 'codingagent')
                      AND ep.dropped = FALSE
                ) AS "exec_started_at?: DateTime<Utc>",
                -- Get latest completed_at from any execution process for this task
                (
                    SELECT MAX(COALESCE(ep.completed_at, ep.started_at))
                    FROM workspaces w
                    JOIN sessions s ON s.workspace_id = w.id
                    JOIN execution_processes ep ON ep.session_id = s.id
                    WHERE w.task_id = t.id
                      AND ep.run_reason IN ('setupscript', 'cleanupscript', 'codingagent')
                      AND ep.dropped = FALSE
                ) AS "exec_completed_at?: DateTime<Utc>",
                -- Aggregate dependencies as comma-separated hex UUIDs (BLOBs must be converted to text)
                IFNULL(
                    (
                        SELECT GROUP_CONCAT(lower(hex(td.depends_on_id)))
                        FROM task_dependencies td
                        WHERE td.task_id = t.id
                    ),
                    ''
                ) AS "dependencies_csv!: String"
            FROM tasks t
            WHERE t.project_id = $1
            ORDER BY t.created_at ASC
            "#,
            project_id
        )
        .fetch_all(pool)
        .await?;

        let tasks = records
            .into_iter()
            .map(|rec| {
                // Calculate progress from status
                let progress = match rec.task_status {
                    TaskStatus::Done => 100.0,
                    TaskStatus::InProgress => 50.0,
                    _ => 0.0,
                };

                // Use execution times if available, otherwise fall back to task times
                let start = rec.exec_started_at.unwrap_or(rec.created_at);
                let end = rec.exec_completed_at.unwrap_or(rec.updated_at);

                // Parse dependencies from comma-separated string
                let dependencies: Vec<Uuid> = if rec.dependencies_csv.is_empty() {
                    Vec::new()
                } else {
                    rec.dependencies_csv
                        .split(',')
                        .filter_map(|s| Uuid::parse_str(s.trim()).ok())
                        .collect()
                };

                GanttTask {
                    id: rec.id,
                    name: rec.name,
                    start,
                    end,
                    progress,
                    dependencies,
                    task_status: rec.task_status,
                }
            })
            .collect();

        Ok(tasks)
    }
}
