use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Executor, FromRow, Sqlite, SqlitePool, Type};
use strum_macros::{Display, EnumString};
use ts_rs::TS;
use uuid::Uuid;

use super::{project::Project, workspace::Workspace};

#[derive(
    Debug, Clone, Type, Serialize, Deserialize, PartialEq, TS, EnumString, Display, Default,
)]
#[sqlx(type_name = "task_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum TaskStatus {
    #[default]
    Todo,
    InProgress,
    InReview,
    Done,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
pub enum TaskOrderBy {
    CreatedAtAsc,
    #[default]
    CreatedAtDesc,
    UpdatedAtAsc,
    UpdatedAtDesc,
}

impl TaskOrderBy {
    pub fn to_sql(&self) -> &'static str {
        match self {
            TaskOrderBy::CreatedAtAsc => "t.created_at ASC",
            TaskOrderBy::CreatedAtDesc => "t.created_at DESC",
            TaskOrderBy::UpdatedAtAsc => "t.updated_at ASC",
            TaskOrderBy::UpdatedAtDesc => "t.updated_at DESC",
        }
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct Task {
    pub id: Uuid,
    pub project_id: Uuid, // Foreign key to Project
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub parent_workspace_id: Option<Uuid>, // Foreign key to parent Workspace
    pub shared_task_id: Option<Uuid>,
    pub task_group_id: Option<Uuid>, // Foreign key to TaskGroup
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct TaskWithAttemptStatus {
    #[serde(flatten)]
    #[ts(flatten)]
    pub task: Task,
    pub has_in_progress_attempt: bool,
    pub last_attempt_failed: bool,
    pub is_blocked: bool,
    pub is_queued: bool,
    pub executor: String,
}

impl std::ops::Deref for TaskWithAttemptStatus {
    type Target = Task;
    fn deref(&self) -> &Self::Target {
        &self.task
    }
}

impl std::ops::DerefMut for TaskWithAttemptStatus {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.task
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct TaskRelationships {
    pub parent_task: Option<Task>, // The task that owns the parent workspace
    pub current_workspace: Workspace, // The workspace we're viewing
    pub children: Vec<Task>,       // Tasks created from this workspace
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreateTask {
    pub project_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub status: Option<TaskStatus>,
    pub parent_workspace_id: Option<Uuid>,
    pub image_ids: Option<Vec<Uuid>>,
    pub shared_task_id: Option<Uuid>,
    pub task_group_id: Option<Uuid>,
}

impl CreateTask {
    pub fn from_title_description(
        project_id: Uuid,
        title: String,
        description: Option<String>,
    ) -> Self {
        Self {
            project_id,
            title,
            description,
            status: Some(TaskStatus::Todo),
            parent_workspace_id: None,
            image_ids: None,
            shared_task_id: None,
            task_group_id: None,
        }
    }

    pub fn from_shared_task(
        project_id: Uuid,
        title: String,
        description: Option<String>,
        status: TaskStatus,
        shared_task_id: Uuid,
    ) -> Self {
        Self {
            project_id,
            title,
            description,
            status: Some(status),
            parent_workspace_id: None,
            image_ids: None,
            shared_task_id: Some(shared_task_id),
            task_group_id: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct UpdateTask {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<TaskStatus>,
    pub parent_workspace_id: Option<Uuid>,
    pub image_ids: Option<Vec<Uuid>>,
    pub task_group_id: Option<Uuid>,
}

impl Task {
    pub fn to_prompt(&self) -> String {
        if let Some(description) = self.description.as_ref().filter(|d| !d.trim().is_empty()) {
            format!("{}\n\n{}", &self.title, description)
        } else {
            self.title.clone()
        }
    }

    pub async fn parent_project(&self, pool: &SqlitePool) -> Result<Option<Project>, sqlx::Error> {
        Project::find_by_id(pool, self.project_id).await
    }

    pub async fn find_by_project_id_with_attempt_status(
        pool: &SqlitePool,
        project_id: Uuid,
    ) -> Result<Vec<TaskWithAttemptStatus>, sqlx::Error> {
        let records = sqlx::query!(
            r#"SELECT
  t.id                            AS "id!: Uuid",
  t.project_id                    AS "project_id!: Uuid",
  t.title,
  t.description,
  t.status                        AS "status!: TaskStatus",
  t.parent_workspace_id           AS "parent_workspace_id: Uuid",
  t.shared_task_id                AS "shared_task_id: Uuid",
  t.task_group_id                 AS "task_group_id: Uuid",
  t.created_at                    AS "created_at!: DateTime<Utc>",
  t.updated_at                    AS "updated_at!: DateTime<Utc>",

  CASE WHEN EXISTS (
    SELECT 1
      FROM task_dependencies td
      JOIN tasks dep ON dep.id = td.depends_on_id
     WHERE td.task_id = t.id
       AND dep.status != 'done'
  ) THEN 1 ELSE 0 END            AS "is_blocked!: i64",

  CASE WHEN EXISTS (
    SELECT 1
      FROM workspaces w
      JOIN sessions s ON s.workspace_id = w.id
      JOIN execution_processes ep ON ep.session_id = s.id
     WHERE w.task_id       = t.id
       AND ep.status        = 'running'
       AND ep.run_reason IN ('setupscript','cleanupscript','codingagent')
     LIMIT 1
  ) THEN 1 ELSE 0 END            AS "has_in_progress_attempt!: i64",

  CASE WHEN (
    SELECT ep.status
      FROM workspaces w
      JOIN sessions s ON s.workspace_id = w.id
      JOIN execution_processes ep ON ep.session_id = s.id
     WHERE w.task_id       = t.id
     AND ep.run_reason IN ('setupscript','cleanupscript','codingagent')
     ORDER BY ep.created_at DESC
     LIMIT 1
  ) IN ('failed','killed') THEN 1 ELSE 0 END
                                 AS "last_attempt_failed!: i64",

  CASE WHEN EXISTS (
    SELECT 1 FROM workspaces w
    JOIN execution_queue eq ON eq.workspace_id = w.id
    WHERE w.task_id = t.id
    LIMIT 1
  ) THEN 1 ELSE 0 END            AS "is_queued!: i64",

  ( SELECT s.executor
      FROM workspaces w
      JOIN sessions s ON s.workspace_id = w.id
      WHERE w.task_id = t.id
     ORDER BY s.created_at DESC
      LIMIT 1
    )                               AS "executor!: String"

FROM tasks t
WHERE t.project_id = $1
ORDER BY t.created_at DESC"#,
            project_id
        )
        .fetch_all(pool)
        .await?;

        let tasks = records
            .into_iter()
            .map(|rec| TaskWithAttemptStatus {
                task: Task {
                    id: rec.id,
                    project_id: rec.project_id,
                    title: rec.title,
                    description: rec.description,
                    status: rec.status,
                    parent_workspace_id: rec.parent_workspace_id,
                    shared_task_id: rec.shared_task_id,
                    task_group_id: rec.task_group_id,
                    created_at: rec.created_at,
                    updated_at: rec.updated_at,
                },
                has_in_progress_attempt: rec.has_in_progress_attempt != 0,
                last_attempt_failed: rec.last_attempt_failed != 0,
                is_blocked: rec.is_blocked != 0,
                is_queued: rec.is_queued != 0,
                executor: rec.executor,
            })
            .collect();

        Ok(tasks)
    }

    pub async fn find_by_id_with_attempt_status(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Option<TaskWithAttemptStatus>, sqlx::Error> {
        let rec = sqlx::query!(
            r#"SELECT
  t.id                            AS "id!: Uuid",
  t.project_id                    AS "project_id!: Uuid",
  t.title,
  t.description,
  t.status                        AS "status!: TaskStatus",
  t.parent_workspace_id           AS "parent_workspace_id: Uuid",
  t.shared_task_id                AS "shared_task_id: Uuid",
  t.task_group_id                 AS "task_group_id: Uuid",
  t.created_at                    AS "created_at!: DateTime<Utc>",
  t.updated_at                    AS "updated_at!: DateTime<Utc>",

  CASE WHEN EXISTS (
    SELECT 1
      FROM task_dependencies td
      JOIN tasks dep ON dep.id = td.depends_on_id
     WHERE td.task_id = t.id
       AND dep.status != 'done'
  ) THEN 1 ELSE 0 END            AS "is_blocked!: i64",

  CASE WHEN EXISTS (
    SELECT 1
      FROM workspaces w
      JOIN sessions s ON s.workspace_id = w.id
      JOIN execution_processes ep ON ep.session_id = s.id
     WHERE w.task_id       = t.id
       AND ep.status        = 'running'
       AND ep.run_reason IN ('setupscript','cleanupscript','codingagent')
     LIMIT 1
  ) THEN 1 ELSE 0 END            AS "has_in_progress_attempt!: i64",

  CASE WHEN (
    SELECT ep.status
      FROM workspaces w
      JOIN sessions s ON s.workspace_id = w.id
      JOIN execution_processes ep ON ep.session_id = s.id
     WHERE w.task_id       = t.id
     AND ep.run_reason IN ('setupscript','cleanupscript','codingagent')
     ORDER BY ep.created_at DESC
     LIMIT 1
  ) IN ('failed','killed') THEN 1 ELSE 0 END
                                 AS "last_attempt_failed!: i64",

  CASE WHEN EXISTS (
    SELECT 1 FROM workspaces w
    JOIN execution_queue eq ON eq.workspace_id = w.id
    WHERE w.task_id = t.id
    LIMIT 1
  ) THEN 1 ELSE 0 END            AS "is_queued!: i64",

  ( SELECT s.executor
      FROM workspaces w
      JOIN sessions s ON s.workspace_id = w.id
      WHERE w.task_id = t.id
     ORDER BY s.created_at DESC
      LIMIT 1
    )                               AS "executor!: String"

FROM tasks t
WHERE t.id = $1"#,
            task_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(rec.map(|rec| TaskWithAttemptStatus {
            task: Task {
                id: rec.id,
                project_id: rec.project_id,
                title: rec.title,
                description: rec.description,
                status: rec.status,
                parent_workspace_id: rec.parent_workspace_id,
                shared_task_id: rec.shared_task_id,
                task_group_id: rec.task_group_id,
                created_at: rec.created_at,
                updated_at: rec.updated_at,
            },
            has_in_progress_attempt: rec.has_in_progress_attempt != 0,
            last_attempt_failed: rec.last_attempt_failed != 0,
            is_blocked: rec.is_blocked != 0,
            is_queued: rec.is_queued != 0,
            executor: rec.executor,
        }))
    }

    pub async fn find_paginated_by_project_id_with_attempt_status(
        pool: &SqlitePool,
        project_id: Uuid,
        query: Option<String>,
        status: Option<TaskStatus>,
        order_by: TaskOrderBy,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<TaskWithAttemptStatus>, i64), sqlx::Error> {
        let search_pattern = query.as_ref().map(|q| format!("%{}%", q));

        let total = sqlx::query!(
            r#"SELECT COUNT(*) as "count!: i64"
               FROM tasks t
               WHERE t.project_id = $1
                 AND ($2 IS NULL OR t.status = $2)
                 AND ($3 IS NULL OR t.title LIKE $3 OR t.description LIKE $3)"#,
            project_id,
            status,
            search_pattern
        )
        .fetch_one(pool)
        .await?
        .count;

        let status_str = status.as_ref().map(|s| s.to_string().to_lowercase());

        let query = format!(
            r#"SELECT
  t.id,
  t.project_id,
  t.title,
  t.description,
  t.status,
  t.parent_workspace_id,
  t.shared_task_id,
  t.task_group_id,
  t.created_at,
  t.updated_at,

  CASE WHEN EXISTS (
    SELECT 1
      FROM task_dependencies td
      JOIN tasks dep ON dep.id = td.depends_on_id
     WHERE td.task_id = t.id
       AND dep.status != 'done'
  ) THEN 1 ELSE 0 END AS is_blocked,

  CASE WHEN EXISTS (
    SELECT 1
      FROM workspaces w
      JOIN sessions s ON s.workspace_id = w.id
      JOIN execution_processes ep ON ep.session_id = s.id
     WHERE w.task_id       = t.id
       AND ep.status        = 'running'
       AND ep.run_reason IN ('setupscript','cleanupscript','codingagent')
     LIMIT 1
  ) THEN 1 ELSE 0 END AS has_in_progress_attempt,

  CASE WHEN (
    SELECT ep.status
      FROM workspaces w
      JOIN sessions s ON s.workspace_id = w.id
      JOIN execution_processes ep ON ep.session_id = s.id
     WHERE w.task_id       = t.id
     AND ep.run_reason IN ('setupscript','cleanupscript','codingagent')
     ORDER BY ep.created_at DESC
     LIMIT 1
  ) IN ('failed','killed') THEN 1 ELSE 0 END AS last_attempt_failed,

  CASE WHEN EXISTS (
    SELECT 1 FROM workspaces w
    JOIN execution_queue eq ON eq.workspace_id = w.id
    WHERE w.task_id = t.id
    LIMIT 1
  ) THEN 1 ELSE 0 END AS is_queued,

  COALESCE(( SELECT s.executor
      FROM workspaces w
      JOIN sessions s ON s.workspace_id = w.id
      WHERE w.task_id = t.id
     ORDER BY s.created_at DESC
      LIMIT 1
    ), '') AS executor

FROM tasks t
WHERE t.project_id = ?1
  AND (?2 IS NULL OR t.status = ?2)
  AND (?5 IS NULL OR t.title LIKE ?5 OR t.description LIKE ?5)
ORDER BY {}
LIMIT ?3 OFFSET ?4"#,
            order_by.to_sql()
        );

        #[derive(FromRow)]
        struct TaskWithAttemptStatusRow {
            id: Uuid,
            project_id: Uuid,
            title: String,
            description: Option<String>,
            status: TaskStatus,
            parent_workspace_id: Option<Uuid>,
            shared_task_id: Option<Uuid>,
            task_group_id: Option<Uuid>,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
            is_blocked: i64,
            has_in_progress_attempt: i64,
            last_attempt_failed: i64,
            is_queued: i64,
            executor: String,
        }

        let records: Vec<TaskWithAttemptStatusRow> = sqlx::query_as(&query)
            .bind(project_id)
            .bind(status_str)
            .bind(limit)
            .bind(offset)
            .bind(search_pattern)
            .fetch_all(pool)
            .await?;

        let tasks = records
            .into_iter()
            .map(|rec| TaskWithAttemptStatus {
                task: Task {
                    id: rec.id,
                    project_id: rec.project_id,
                    title: rec.title,
                    description: rec.description,
                    status: rec.status,
                    parent_workspace_id: rec.parent_workspace_id,
                    shared_task_id: rec.shared_task_id,
                    task_group_id: rec.task_group_id,
                    created_at: rec.created_at,
                    updated_at: rec.updated_at,
                },
                has_in_progress_attempt: rec.has_in_progress_attempt != 0,
                last_attempt_failed: rec.last_attempt_failed != 0,
                is_blocked: rec.is_blocked != 0,
                is_queued: rec.is_queued != 0,
                executor: rec.executor,
            })
            .collect();

        Ok((tasks, total))
    }

    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Task,
            r#"SELECT id as "id!: Uuid", project_id as "project_id!: Uuid", title, description, status as "status!: TaskStatus", parent_workspace_id as "parent_workspace_id: Uuid", shared_task_id as "shared_task_id: Uuid", task_group_id as "task_group_id: Uuid", created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>"
               FROM tasks
               WHERE id = $1"#,
            id
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_rowid(pool: &SqlitePool, rowid: i64) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Task,
            r#"SELECT id as "id!: Uuid", project_id as "project_id!: Uuid", title, description, status as "status!: TaskStatus", parent_workspace_id as "parent_workspace_id: Uuid", shared_task_id as "shared_task_id: Uuid", task_group_id as "task_group_id: Uuid", created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>"
               FROM tasks
               WHERE rowid = $1"#,
            rowid
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_shared_task_id<'e, E>(
        executor: E,
        shared_task_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error>
    where
        E: Executor<'e, Database = Sqlite>,
    {
        sqlx::query_as!(
            Task,
            r#"SELECT id as "id!: Uuid", project_id as "project_id!: Uuid", title, description, status as "status!: TaskStatus", parent_workspace_id as "parent_workspace_id: Uuid", shared_task_id as "shared_task_id: Uuid", task_group_id as "task_group_id: Uuid", created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>"
               FROM tasks
               WHERE shared_task_id = $1
               LIMIT 1"#,
            shared_task_id
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn find_all_shared(pool: &SqlitePool) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Task,
            r#"SELECT id as "id!: Uuid", project_id as "project_id!: Uuid", title, description, status as "status!: TaskStatus", parent_workspace_id as "parent_workspace_id: Uuid", shared_task_id as "shared_task_id: Uuid", task_group_id as "task_group_id: Uuid", created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>"
               FROM tasks
               WHERE shared_task_id IS NOT NULL"#
        )
        .fetch_all(pool)
        .await
    }

    pub async fn create(
        pool: &SqlitePool,
        data: &CreateTask,
        task_id: Uuid,
    ) -> Result<Self, sqlx::Error> {
        let status = data.status.clone().unwrap_or_default();
        sqlx::query_as!(
            Task,
            r#"INSERT INTO tasks (id, project_id, title, description, status, parent_workspace_id, shared_task_id, task_group_id)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               RETURNING id as "id!: Uuid", project_id as "project_id!: Uuid", title, description, status as "status!: TaskStatus", parent_workspace_id as "parent_workspace_id: Uuid", shared_task_id as "shared_task_id: Uuid", task_group_id as "task_group_id: Uuid", created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>""#,
            task_id,
            data.project_id,
            data.title,
            data.description,
            status,
            data.parent_workspace_id,
            data.shared_task_id,
            data.task_group_id
        )
        .fetch_one(pool)
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update(
        pool: &SqlitePool,
        id: Uuid,
        project_id: Uuid,
        title: String,
        description: Option<String>,
        status: TaskStatus,
        parent_workspace_id: Option<Uuid>,
        task_group_id: Option<Uuid>,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as!(
            Task,
            r#"UPDATE tasks
               SET title = $3, description = $4, status = $5, parent_workspace_id = $6, task_group_id = $7
               WHERE id = $1 AND project_id = $2
               RETURNING id as "id!: Uuid", project_id as "project_id!: Uuid", title, description, status as "status!: TaskStatus", parent_workspace_id as "parent_workspace_id: Uuid", shared_task_id as "shared_task_id: Uuid", task_group_id as "task_group_id: Uuid", created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>""#,
            id,
            project_id,
            title,
            description,
            status,
            parent_workspace_id,
            task_group_id
        )
        .fetch_one(pool)
        .await
    }

    pub async fn update_status(
        pool: &SqlitePool,
        id: Uuid,
        status: TaskStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE tasks SET status = $2, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
            id,
            status
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Update the parent_workspace_id field for a task
    pub async fn update_parent_workspace_id(
        pool: &SqlitePool,
        task_id: Uuid,
        parent_workspace_id: Option<Uuid>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE tasks SET parent_workspace_id = $2, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
            task_id,
            parent_workspace_id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Nullify parent_workspace_id for all tasks that reference the given workspace ID
    /// This breaks parent-child relationships before deleting a parent task
    pub async fn nullify_children_by_workspace_id<'e, E>(
        executor: E,
        workspace_id: Uuid,
    ) -> Result<u64, sqlx::Error>
    where
        E: Executor<'e, Database = Sqlite>,
    {
        let result = sqlx::query!(
            "UPDATE tasks SET parent_workspace_id = NULL WHERE parent_workspace_id = $1",
            workspace_id
        )
        .execute(executor)
        .await?;
        Ok(result.rows_affected())
    }

    /// Clear shared_task_id for all tasks that reference shared tasks belonging to a remote project
    /// This breaks the link between local tasks and shared tasks when a project is unlinked
    pub async fn clear_shared_task_ids_for_remote_project<'e, E>(
        executor: E,
        remote_project_id: Uuid,
    ) -> Result<u64, sqlx::Error>
    where
        E: Executor<'e, Database = Sqlite>,
    {
        let result = sqlx::query!(
            r#"UPDATE tasks
               SET shared_task_id = NULL
               WHERE project_id IN (
                   SELECT id FROM projects WHERE remote_project_id = $1
               )"#,
            remote_project_id
        )
        .execute(executor)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn delete<'e, E>(executor: E, id: Uuid) -> Result<u64, sqlx::Error>
    where
        E: Executor<'e, Database = Sqlite>,
    {
        let result = sqlx::query!("DELETE FROM tasks WHERE id = $1", id)
            .execute(executor)
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn set_shared_task_id<'e, E>(
        executor: E,
        id: Uuid,
        shared_task_id: Option<Uuid>,
    ) -> Result<(), sqlx::Error>
    where
        E: Executor<'e, Database = Sqlite>,
    {
        sqlx::query!(
            "UPDATE tasks SET shared_task_id = $2, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
            id,
            shared_task_id
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    /// Inherit task_group_id from another task if the current task has no group.
    /// Returns the number of rows affected (0 if task already has a group, 1 if inherited).
    pub async fn inherit_group_if_none(
        pool: &SqlitePool,
        task_id: Uuid,
        group_id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            "UPDATE tasks SET task_group_id = $1, updated_at = CURRENT_TIMESTAMP WHERE id = $2 AND task_group_id IS NULL",
            group_id,
            task_id
        )
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn batch_unlink_shared_tasks<'e, E>(
        executor: E,
        shared_task_ids: &[Uuid],
    ) -> Result<u64, sqlx::Error>
    where
        E: Executor<'e, Database = Sqlite>,
    {
        if shared_task_ids.is_empty() {
            return Ok(0);
        }

        let mut query_builder = sqlx::QueryBuilder::new(
            "UPDATE tasks SET shared_task_id = NULL, updated_at = CURRENT_TIMESTAMP WHERE shared_task_id IN (",
        );

        let mut separated = query_builder.separated(", ");
        for id in shared_task_ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");

        let result = query_builder.build().execute(executor).await?;
        Ok(result.rows_affected())
    }

    pub async fn find_children_by_workspace_id(
        pool: &SqlitePool,
        workspace_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        // Find only child tasks that have this workspace as their parent
        sqlx::query_as!(
            Task,
            r#"SELECT id as "id!: Uuid", project_id as "project_id!: Uuid", title, description, status as "status!: TaskStatus", parent_workspace_id as "parent_workspace_id: Uuid", shared_task_id as "shared_task_id: Uuid", task_group_id as "task_group_id: Uuid", created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>"
               FROM tasks
               WHERE parent_workspace_id = $1
               ORDER BY created_at DESC"#,
            workspace_id,
        )
        .fetch_all(pool)
        .await
    }

    /// Escape special FTS5 query syntax characters.
    /// FTS5 special characters: " * ^ - : OR AND NOT NEAR
    /// We wrap tokens in double quotes to treat them as literals.
    fn escape_fts5_query(query: &str) -> String {
        // Split on whitespace, wrap each token in quotes, and join with spaces
        // This ensures special characters are treated as literals
        query
            .split_whitespace()
            .map(|token| {
                // Escape any internal double quotes by doubling them
                let escaped = token.replace('"', "\"\"");
                format!("\"{}\"", escaped)
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Search tasks using FTS5 full-text search with BM25 ranking.
    ///
    /// Returns tasks matching the query, ranked by BM25 relevance score.
    /// The BM25 score is negated (more negative = more relevant), so we
    /// return it as a positive score where higher = better match.
    pub async fn search_fts(
        pool: &SqlitePool,
        project_id: Uuid,
        query: &str,
        status: Option<TaskStatus>,
        limit: i64,
    ) -> Result<Vec<(TaskWithAttemptStatus, f64)>, sqlx::Error> {
        let trimmed_query = query.trim();
        if trimmed_query.is_empty() {
            return Ok(Vec::new());
        }

        let escaped_query = Self::escape_fts5_query(trimmed_query);
        let status_str = status.map(|s| s.to_string().to_lowercase());

        #[derive(FromRow)]
        struct FtsSearchRow {
            id: Uuid,
            project_id: Uuid,
            title: String,
            description: Option<String>,
            status: TaskStatus,
            parent_workspace_id: Option<Uuid>,
            shared_task_id: Option<Uuid>,
            task_group_id: Option<Uuid>,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
            is_blocked: i64,
            has_in_progress_attempt: i64,
            last_attempt_failed: i64,
            is_queued: i64,
            executor: String,
            rank_score: f64,
        }

        let records: Vec<FtsSearchRow> = sqlx::query_as(
            r#"SELECT
  t.id,
  t.project_id,
  t.title,
  t.description,
  t.status,
  t.parent_workspace_id,
  t.shared_task_id,
  t.task_group_id,
  t.created_at,
  t.updated_at,

  CASE WHEN EXISTS (
    SELECT 1
      FROM task_dependencies td
      JOIN tasks dep ON dep.id = td.depends_on_id
     WHERE td.task_id = t.id
       AND dep.status != 'done'
  ) THEN 1 ELSE 0 END AS is_blocked,

  CASE WHEN EXISTS (
    SELECT 1
      FROM workspaces w
      JOIN sessions s ON s.workspace_id = w.id
      JOIN execution_processes ep ON ep.session_id = s.id
     WHERE w.task_id       = t.id
       AND ep.status        = 'running'
       AND ep.run_reason IN ('setupscript','cleanupscript','codingagent')
     LIMIT 1
  ) THEN 1 ELSE 0 END AS has_in_progress_attempt,

  CASE WHEN (
    SELECT ep.status
      FROM workspaces w
      JOIN sessions s ON s.workspace_id = w.id
      JOIN execution_processes ep ON ep.session_id = s.id
     WHERE w.task_id       = t.id
     AND ep.run_reason IN ('setupscript','cleanupscript','codingagent')
     ORDER BY ep.created_at DESC
     LIMIT 1
  ) IN ('failed','killed') THEN 1 ELSE 0 END AS last_attempt_failed,

  CASE WHEN EXISTS (
    SELECT 1 FROM workspaces w
    JOIN execution_queue eq ON eq.workspace_id = w.id
    WHERE w.task_id = t.id
    LIMIT 1
  ) THEN 1 ELSE 0 END AS is_queued,

  COALESCE(( SELECT s.executor
      FROM workspaces w
      JOIN sessions s ON s.workspace_id = w.id
      WHERE w.task_id = t.id
     ORDER BY s.created_at DESC
      LIMIT 1
    ), '') AS executor,

  -bm25(tasks_fts) AS rank_score

FROM tasks_fts
JOIN tasks t ON t.rowid = tasks_fts.rowid
WHERE tasks_fts MATCH ?1
  AND t.project_id = ?2
  AND (?3 IS NULL OR t.status = ?3)
ORDER BY rank_score DESC
LIMIT ?4"#,
        )
        .bind(&escaped_query)
        .bind(project_id)
        .bind(status_str)
        .bind(limit)
        .fetch_all(pool)
        .await?;

        let results = records
            .into_iter()
            .map(|rec| {
                (
                    TaskWithAttemptStatus {
                        task: Task {
                            id: rec.id,
                            project_id: rec.project_id,
                            title: rec.title,
                            description: rec.description,
                            status: rec.status,
                            parent_workspace_id: rec.parent_workspace_id,
                            shared_task_id: rec.shared_task_id,
                            task_group_id: rec.task_group_id,
                            created_at: rec.created_at,
                            updated_at: rec.updated_at,
                        },
                        has_in_progress_attempt: rec.has_in_progress_attempt != 0,
                        last_attempt_failed: rec.last_attempt_failed != 0,
                        is_blocked: rec.is_blocked != 0,
                        is_queued: rec.is_queued != 0,
                        executor: rec.executor,
                    },
                    rec.rank_score,
                )
            })
            .collect();

        Ok(results)
    }

    /// Search tasks using hybrid search combining vector similarity and FTS5.
    ///
    /// Combines semantic understanding (vector embeddings) with exact keyword matching (FTS5)
    /// for best results. Uses weighted scoring: 0.6 * vector_score + 0.4 * fts_score.
    ///
    /// Handles edge cases:
    /// - Tasks with embedding but no FTS match: uses vector score only
    /// - Tasks with FTS match but no embedding: uses FTS score only
    /// - Tasks matching both: uses weighted combination (highest ranked)
    pub async fn search_hybrid(
        pool: &SqlitePool,
        project_id: Uuid,
        query_embedding: &[f32],
        keyword_query: &str,
        status: Option<TaskStatus>,
        limit: i64,
    ) -> Result<Vec<(TaskWithAttemptStatus, f64)>, sqlx::Error> {
        use super::embedding::{TaskEmbedding, EMBEDDING_DIMENSION};

        if query_embedding.len() != EMBEDDING_DIMENSION {
            return Err(sqlx::Error::Protocol(format!(
                "Query embedding dimension mismatch: expected {}, got {}",
                EMBEDDING_DIMENSION,
                query_embedding.len()
            )));
        }

        let trimmed_query = keyword_query.trim();
        let escaped_query = if trimmed_query.is_empty() {
            None
        } else {
            Some(Self::escape_fts5_query(trimmed_query))
        };

        let query_bytes = TaskEmbedding::serialize_embedding(query_embedding);
        let status_str = status.map(|s| s.to_string().to_lowercase());

        // Hybrid search query:
        // - vector_scores: cosine similarity converted to 0-1 (1 = most similar)
        // - fts_scores: BM25 normalized to 0-1 range using sigmoid-like transform
        // - Final score: 0.6 * vector + 0.4 * fts, with fallback when one is missing
        #[derive(FromRow)]
        struct HybridSearchRow {
            id: Uuid,
            project_id: Uuid,
            title: String,
            description: Option<String>,
            status: TaskStatus,
            parent_workspace_id: Option<Uuid>,
            shared_task_id: Option<Uuid>,
            task_group_id: Option<Uuid>,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
            is_blocked: i64,
            has_in_progress_attempt: i64,
            last_attempt_failed: i64,
            is_queued: i64,
            executor: String,
            hybrid_score: f64,
        }

        // Build the query dynamically based on whether we have a keyword query
        let sql = if escaped_query.is_some() {
            // Full hybrid: both vector and FTS
            r#"WITH vector_scores AS (
                SELECT
                    te.task_rowid,
                    -- Convert cosine distance (0-2) to similarity score (1-0)
                    1.0 - (vec_distance_cosine(te.embedding, ?1) / 2.0) AS score
                FROM task_embeddings te
                JOIN tasks t ON t.rowid = te.task_rowid
                WHERE t.project_id = ?2
            ),
            fts_scores AS (
                SELECT
                    tasks_fts.rowid,
                    -- BM25 scores are negative (more negative = better match)
                    -- Normalize to 0-1 using: MIN(1.0, -bm25 / 20.0) - caps at score of 1.0 for bm25 <= -20
                    -- A typical good match has bm25 around -5 to -15
                    MIN(1.0, MAX(0.0, -bm25(tasks_fts) / 20.0)) AS score
                FROM tasks_fts
                WHERE tasks_fts MATCH ?3
            )
            SELECT
                t.id,
                t.project_id,
                t.title,
                t.description,
                t.status,
                t.parent_workspace_id,
                t.shared_task_id,
                t.task_group_id,
                t.created_at,
                t.updated_at,

                CASE WHEN EXISTS (
                    SELECT 1
                    FROM task_dependencies td
                    JOIN tasks dep ON dep.id = td.depends_on_id
                    WHERE td.task_id = t.id
                    AND dep.status != 'done'
                ) THEN 1 ELSE 0 END AS is_blocked,

                CASE WHEN EXISTS (
                    SELECT 1
                    FROM workspaces w
                    JOIN sessions s ON s.workspace_id = w.id
                    JOIN execution_processes ep ON ep.session_id = s.id
                    WHERE w.task_id = t.id
                    AND ep.status = 'running'
                    AND ep.run_reason IN ('setupscript','cleanupscript','codingagent')
                    LIMIT 1
                ) THEN 1 ELSE 0 END AS has_in_progress_attempt,

                CASE WHEN (
                    SELECT ep.status
                    FROM workspaces w
                    JOIN sessions s ON s.workspace_id = w.id
                    JOIN execution_processes ep ON ep.session_id = s.id
                    WHERE w.task_id = t.id
                    AND ep.run_reason IN ('setupscript','cleanupscript','codingagent')
                    ORDER BY ep.created_at DESC
                    LIMIT 1
                ) IN ('failed','killed') THEN 1 ELSE 0 END AS last_attempt_failed,

                CASE WHEN EXISTS (
                    SELECT 1 FROM workspaces w
                    JOIN execution_queue eq ON eq.workspace_id = w.id
                    WHERE w.task_id = t.id
                    LIMIT 1
                ) THEN 1 ELSE 0 END AS is_queued,

                COALESCE((SELECT s.executor
                    FROM workspaces w
                    JOIN sessions s ON s.workspace_id = w.id
                    WHERE w.task_id = t.id
                    ORDER BY s.created_at DESC
                    LIMIT 1
                ), '') AS executor,

                -- Hybrid score calculation:
                -- When both exist: weighted combination
                -- When only vector: use vector score
                -- When only FTS: use FTS score
                CASE
                    WHEN vs.score IS NOT NULL AND fs.score IS NOT NULL THEN
                        0.6 * vs.score + 0.4 * fs.score
                    WHEN vs.score IS NOT NULL THEN
                        vs.score
                    WHEN fs.score IS NOT NULL THEN
                        fs.score
                    ELSE 0.0
                END AS hybrid_score

            FROM tasks t
            LEFT JOIN vector_scores vs ON vs.task_rowid = t.rowid
            LEFT JOIN fts_scores fs ON fs.rowid = t.rowid
            WHERE t.project_id = ?2
                AND (vs.score IS NOT NULL OR fs.score IS NOT NULL)
                AND (?4 IS NULL OR t.status = ?4)
            ORDER BY hybrid_score DESC
            LIMIT ?5"#
        } else {
            // Vector-only search (no keyword query)
            r#"WITH vector_scores AS (
                SELECT
                    te.task_rowid,
                    1.0 - (vec_distance_cosine(te.embedding, ?1) / 2.0) AS score
                FROM task_embeddings te
                JOIN tasks t ON t.rowid = te.task_rowid
                WHERE t.project_id = ?2
            )
            SELECT
                t.id,
                t.project_id,
                t.title,
                t.description,
                t.status,
                t.parent_workspace_id,
                t.shared_task_id,
                t.task_group_id,
                t.created_at,
                t.updated_at,

                CASE WHEN EXISTS (
                    SELECT 1
                    FROM task_dependencies td
                    JOIN tasks dep ON dep.id = td.depends_on_id
                    WHERE td.task_id = t.id
                    AND dep.status != 'done'
                ) THEN 1 ELSE 0 END AS is_blocked,

                CASE WHEN EXISTS (
                    SELECT 1
                    FROM workspaces w
                    JOIN sessions s ON s.workspace_id = w.id
                    JOIN execution_processes ep ON ep.session_id = s.id
                    WHERE w.task_id = t.id
                    AND ep.status = 'running'
                    AND ep.run_reason IN ('setupscript','cleanupscript','codingagent')
                    LIMIT 1
                ) THEN 1 ELSE 0 END AS has_in_progress_attempt,

                CASE WHEN (
                    SELECT ep.status
                    FROM workspaces w
                    JOIN sessions s ON s.workspace_id = w.id
                    JOIN execution_processes ep ON ep.session_id = s.id
                    WHERE w.task_id = t.id
                    AND ep.run_reason IN ('setupscript','cleanupscript','codingagent')
                    ORDER BY ep.created_at DESC
                    LIMIT 1
                ) IN ('failed','killed') THEN 1 ELSE 0 END AS last_attempt_failed,

                CASE WHEN EXISTS (
                    SELECT 1 FROM workspaces w
                    JOIN execution_queue eq ON eq.workspace_id = w.id
                    WHERE w.task_id = t.id
                    LIMIT 1
                ) THEN 1 ELSE 0 END AS is_queued,

                COALESCE((SELECT s.executor
                    FROM workspaces w
                    JOIN sessions s ON s.workspace_id = w.id
                    WHERE w.task_id = t.id
                    ORDER BY s.created_at DESC
                    LIMIT 1
                ), '') AS executor,

                vs.score AS hybrid_score

            FROM tasks t
            JOIN vector_scores vs ON vs.task_rowid = t.rowid
            WHERE t.project_id = ?2
                AND (?4 IS NULL OR t.status = ?4)
            ORDER BY hybrid_score DESC
            LIMIT ?5"#
        };

        let records: Vec<HybridSearchRow> = if let Some(ref fts_query) = escaped_query {
            sqlx::query_as(sql)
                .bind(&query_bytes)
                .bind(project_id)
                .bind(fts_query)
                .bind(&status_str)
                .bind(limit)
                .fetch_all(pool)
                .await?
        } else {
            sqlx::query_as(sql)
                .bind(&query_bytes)
                .bind(project_id)
                .bind::<Option<String>>(None) // placeholder for ?3
                .bind(&status_str)
                .bind(limit)
                .fetch_all(pool)
                .await?
        };

        let results = records
            .into_iter()
            .map(|rec| {
                (
                    TaskWithAttemptStatus {
                        task: Task {
                            id: rec.id,
                            project_id: rec.project_id,
                            title: rec.title,
                            description: rec.description,
                            status: rec.status,
                            parent_workspace_id: rec.parent_workspace_id,
                            shared_task_id: rec.shared_task_id,
                            task_group_id: rec.task_group_id,
                            created_at: rec.created_at,
                            updated_at: rec.updated_at,
                        },
                        has_in_progress_attempt: rec.has_in_progress_attempt != 0,
                        last_attempt_failed: rec.last_attempt_failed != 0,
                        is_blocked: rec.is_blocked != 0,
                        is_queued: rec.is_queued != 0,
                        executor: rec.executor,
                    },
                    rec.hybrid_score,
                )
            })
            .collect();

        Ok(results)
    }

    pub async fn find_relationships_for_workspace(
        pool: &SqlitePool,
        workspace: &Workspace,
    ) -> Result<TaskRelationships, sqlx::Error> {
        // 1. Get the current task (task that owns this workspace)
        let current_task = Self::find_by_id(pool, workspace.task_id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;

        // 2. Get parent task (if current task was created by another workspace)
        let parent_task = if let Some(parent_workspace_id) = current_task.parent_workspace_id {
            // Find the workspace that created the current task
            if let Ok(Some(parent_workspace)) =
                Workspace::find_by_id(pool, parent_workspace_id).await
            {
                // Find the task that owns that parent workspace - THAT's the real parent
                Self::find_by_id(pool, parent_workspace.task_id).await?
            } else {
                None
            }
        } else {
            None
        };

        // 3. Get children tasks (created from this workspace)
        let children = Self::find_children_by_workspace_id(pool, workspace.id).await?;

        Ok(TaskRelationships {
            parent_task,
            current_workspace: workspace.clone(),
            children,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::embedding::EMBEDDING_DIMENSION;

    #[test]
    fn test_escape_fts5_query_simple() {
        assert_eq!(Task::escape_fts5_query("hello"), r#""hello""#);
        assert_eq!(Task::escape_fts5_query("hello world"), r#""hello" "world""#);
    }

    #[test]
    fn test_escape_fts5_query_special_chars() {
        // FTS5 operators should be quoted
        assert_eq!(Task::escape_fts5_query("OR"), r#""OR""#);
        assert_eq!(Task::escape_fts5_query("AND"), r#""AND""#);
        assert_eq!(Task::escape_fts5_query("NOT"), r#""NOT""#);
        assert_eq!(Task::escape_fts5_query("NEAR"), r#""NEAR""#);
    }

    #[test]
    fn test_escape_fts5_query_with_asterisk() {
        // Asterisk is a prefix search operator, should be quoted
        assert_eq!(Task::escape_fts5_query("auth*"), r#""auth*""#);
    }

    #[test]
    fn test_escape_fts5_query_with_quotes() {
        // Internal double quotes should be escaped by doubling
        // Input: say "hello" -> tokens: ["say", "\"hello\""]
        // Each token gets wrapped, with internal quotes doubled:
        // "say" -> "\"say\""
        // "hello" (with surrounding quotes) -> doubled internal quotes, then wrapped
        assert_eq!(
            Task::escape_fts5_query(r#"say "hello""#),
            "\"say\" \"\"\"hello\"\"\""
        );
    }

    #[test]
    fn test_escape_fts5_query_whitespace() {
        // Multiple spaces should be collapsed
        assert_eq!(Task::escape_fts5_query("  hello   world  "), r#""hello" "world""#);
    }

    #[test]
    fn test_escape_fts5_query_empty() {
        assert_eq!(Task::escape_fts5_query(""), "");
        assert_eq!(Task::escape_fts5_query("   "), "");
    }

    #[test]
    fn test_escape_fts5_query_column_prefix() {
        // Column prefixes like "title:" should be quoted to prevent FTS5 interpretation
        assert_eq!(Task::escape_fts5_query("title:auth"), r#""title:auth""#);
    }

    #[tokio::test]
    async fn test_search_hybrid_validates_embedding_dimension() {
        // Create a test pool (in-memory)
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();

        let project_id = Uuid::new_v4();

        // Test with wrong dimension (too short)
        let wrong_embedding: Vec<f32> = vec![0.0; 100];
        let result = Task::search_hybrid(
            &pool,
            project_id,
            &wrong_embedding,
            "test query",
            None,
            10,
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("dimension mismatch"));

        // Test with wrong dimension (too long)
        let wrong_embedding: Vec<f32> = vec![0.0; 500];
        let result = Task::search_hybrid(
            &pool,
            project_id,
            &wrong_embedding,
            "test query",
            None,
            10,
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("dimension mismatch"));
    }

    #[test]
    fn test_hybrid_search_embedding_dimension_constant() {
        // Ensure embedding dimension matches expected BGE-small-en-v1.5 size
        assert_eq!(EMBEDDING_DIMENSION, 384);
    }
}
