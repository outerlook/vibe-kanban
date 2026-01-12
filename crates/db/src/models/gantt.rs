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
    pub task_group_id: Option<Uuid>,
}

/// Raw record from the gantt task query, used internally for mapping.
struct GanttTaskRecord {
    id: Uuid,
    name: String,
    task_status: TaskStatus,
    task_group_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    exec_started_at: Option<DateTime<Utc>>,
    exec_completed_at: Option<DateTime<Utc>>,
    dependencies_csv: String,
}

impl From<GanttTaskRecord> for GanttTask {
    fn from(rec: GanttTaskRecord) -> Self {
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
            task_group_id: rec.task_group_id,
        }
    }
}

impl GanttTask {
    /// Find all tasks for a project with their dependencies and execution timeline data
    /// optimized for Gantt visualization.
    ///
    /// Tasks are ordered by created_at DESC (newest first) for consistency with pagination.
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
        let records = sqlx::query_as!(
            GanttTaskRecord,
            r#"
            SELECT
                t.id AS "id!: Uuid",
                t.title AS "name!",
                t.status AS "task_status!: TaskStatus",
                t.task_group_id AS "task_group_id?: Uuid",
                t.created_at AS "created_at!: DateTime<Utc>",
                t.updated_at AS "updated_at!: DateTime<Utc>",
                (
                    SELECT MIN(ep.started_at)
                    FROM workspaces w
                    JOIN sessions s ON s.workspace_id = w.id
                    JOIN execution_processes ep ON ep.session_id = s.id
                    WHERE w.task_id = t.id
                      AND ep.run_reason IN ('setupscript', 'cleanupscript', 'codingagent')
                      AND ep.dropped = FALSE
                ) AS "exec_started_at?: DateTime<Utc>",
                (
                    SELECT MAX(COALESCE(ep.completed_at, ep.started_at))
                    FROM workspaces w
                    JOIN sessions s ON s.workspace_id = w.id
                    JOIN execution_processes ep ON ep.session_id = s.id
                    WHERE w.task_id = t.id
                      AND ep.run_reason IN ('setupscript', 'cleanupscript', 'codingagent')
                      AND ep.dropped = FALSE
                ) AS "exec_completed_at?: DateTime<Utc>",
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
            ORDER BY t.created_at DESC
            "#,
            project_id
        )
        .fetch_all(pool)
        .await?;

        Ok(records.into_iter().map(GanttTask::from).collect())
    }

    /// Find a single task by ID with its dependencies and execution timeline data.
    /// Used for real-time updates to avoid fetching all project tasks.
    pub async fn find_by_id(pool: &SqlitePool, task_id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        let record = sqlx::query_as!(
            GanttTaskRecord,
            r#"
            SELECT
                t.id AS "id!: Uuid",
                t.title AS "name!",
                t.status AS "task_status!: TaskStatus",
                t.task_group_id AS "task_group_id?: Uuid",
                t.created_at AS "created_at!: DateTime<Utc>",
                t.updated_at AS "updated_at!: DateTime<Utc>",
                (
                    SELECT MIN(ep.started_at)
                    FROM workspaces w
                    JOIN sessions s ON s.workspace_id = w.id
                    JOIN execution_processes ep ON ep.session_id = s.id
                    WHERE w.task_id = t.id
                      AND ep.run_reason IN ('setupscript', 'cleanupscript', 'codingagent')
                      AND ep.dropped = FALSE
                ) AS "exec_started_at?: DateTime<Utc>",
                (
                    SELECT MAX(COALESCE(ep.completed_at, ep.started_at))
                    FROM workspaces w
                    JOIN sessions s ON s.workspace_id = w.id
                    JOIN execution_processes ep ON ep.session_id = s.id
                    WHERE w.task_id = t.id
                      AND ep.run_reason IN ('setupscript', 'cleanupscript', 'codingagent')
                      AND ep.dropped = FALSE
                ) AS "exec_completed_at?: DateTime<Utc>",
                IFNULL(
                    (
                        SELECT GROUP_CONCAT(lower(hex(td.depends_on_id)))
                        FROM task_dependencies td
                        WHERE td.task_id = t.id
                    ),
                    ''
                ) AS "dependencies_csv!: String"
            FROM tasks t
            WHERE t.id = $1
            "#,
            task_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(record.map(GanttTask::from))
    }

    /// Find paginated tasks for a project with their dependencies and execution timeline data
    /// optimized for Gantt visualization.
    ///
    /// Returns a tuple of (tasks, total_count) for pagination support.
    /// Tasks are ordered by created_at DESC (newest first).
    pub async fn find_paginated_by_project_id(
        pool: &SqlitePool,
        project_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Self>, i64), sqlx::Error> {
        let total = sqlx::query!(
            r#"SELECT COUNT(*) as "count!: i64" FROM tasks WHERE project_id = $1"#,
            project_id
        )
        .fetch_one(pool)
        .await?
        .count;

        let records = sqlx::query_as!(
            GanttTaskRecord,
            r#"
            SELECT
                t.id AS "id!: Uuid",
                t.title AS "name!",
                t.status AS "task_status!: TaskStatus",
                t.task_group_id AS "task_group_id?: Uuid",
                t.created_at AS "created_at!: DateTime<Utc>",
                t.updated_at AS "updated_at!: DateTime<Utc>",
                (
                    SELECT MIN(ep.started_at)
                    FROM workspaces w
                    JOIN sessions s ON s.workspace_id = w.id
                    JOIN execution_processes ep ON ep.session_id = s.id
                    WHERE w.task_id = t.id
                      AND ep.run_reason IN ('setupscript', 'cleanupscript', 'codingagent')
                      AND ep.dropped = FALSE
                ) AS "exec_started_at?: DateTime<Utc>",
                (
                    SELECT MAX(COALESCE(ep.completed_at, ep.started_at))
                    FROM workspaces w
                    JOIN sessions s ON s.workspace_id = w.id
                    JOIN execution_processes ep ON ep.session_id = s.id
                    WHERE w.task_id = t.id
                      AND ep.run_reason IN ('setupscript', 'cleanupscript', 'codingagent')
                      AND ep.dropped = FALSE
                ) AS "exec_completed_at?: DateTime<Utc>",
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
            ORDER BY t.created_at DESC
            LIMIT $2 OFFSET $3
            "#,
            project_id,
            limit,
            offset
        )
        .fetch_all(pool)
        .await?;

        Ok((records.into_iter().map(GanttTask::from).collect(), total))
    }
}
