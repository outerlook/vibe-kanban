use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use thiserror::Error;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum MergeError {
    #[error("Cannot merge a group into itself")]
    SameGroup,
    #[error("Source task group not found")]
    SourceNotFound,
    #[error("Target task group not found")]
    TargetNotFound,
    #[error("Groups belong to different projects")]
    DifferentProjects,
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
pub struct TaskStatusCounts {
    pub todo: i64,
    pub inprogress: i64,
    pub inreview: i64,
    pub done: i64,
    pub cancelled: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct TaskGroupWithStats {
    #[serde(flatten)]
    #[ts(flatten)]
    pub group: TaskGroup,
    pub task_counts: TaskStatusCounts,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct TaskGroup {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub base_branch: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreateTaskGroup {
    pub project_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub base_branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct UpdateTaskGroup {
    pub name: Option<String>,
    pub description: Option<String>,
    pub base_branch: Option<String>,
}

impl TaskGroup {
    pub async fn create(
        pool: &SqlitePool,
        project_id: Uuid,
        name: String,
        description: Option<String>,
        base_branch: Option<String>,
    ) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();
        sqlx::query_as!(
            TaskGroup,
            r#"INSERT INTO task_groups (id, project_id, name, description, base_branch)
               VALUES ($1, $2, $3, $4, $5)
               RETURNING id as "id!: Uuid",
                         project_id as "project_id!: Uuid",
                         name,
                         description,
                         base_branch,
                         created_at as "created_at!: DateTime<Utc>",
                         updated_at as "updated_at!: DateTime<Utc>""#,
            id,
            project_id,
            name,
            description,
            base_branch
        )
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            TaskGroup,
            r#"SELECT id as "id!: Uuid",
                      project_id as "project_id!: Uuid",
                      name,
                      description,
                      base_branch,
                      created_at as "created_at!: DateTime<Utc>",
                      updated_at as "updated_at!: DateTime<Utc>"
               FROM task_groups
               WHERE id = $1"#,
            id
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_project_id(
        pool: &SqlitePool,
        project_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            TaskGroup,
            r#"SELECT id as "id!: Uuid",
                      project_id as "project_id!: Uuid",
                      name,
                      description,
                      base_branch,
                      created_at as "created_at!: DateTime<Utc>",
                      updated_at as "updated_at!: DateTime<Utc>"
               FROM task_groups
               WHERE project_id = $1
               ORDER BY created_at DESC"#,
            project_id
        )
        .fetch_all(pool)
        .await
    }

    pub async fn update(
        pool: &SqlitePool,
        id: Uuid,
        update: &UpdateTaskGroup,
    ) -> Result<Option<Self>, sqlx::Error> {
        let update_description = update.description.is_some();
        let update_base_branch = update.base_branch.is_some();
        sqlx::query_as!(
            TaskGroup,
            r#"UPDATE task_groups
               SET name = COALESCE($2, name),
                   description = CASE WHEN $3 THEN $4 ELSE description END,
                   base_branch = CASE WHEN $5 THEN $6 ELSE base_branch END,
                   updated_at = datetime('now', 'subsec')
               WHERE id = $1
               RETURNING id as "id!: Uuid",
                         project_id as "project_id!: Uuid",
                         name,
                         description,
                         base_branch,
                         created_at as "created_at!: DateTime<Utc>",
                         updated_at as "updated_at!: DateTime<Utc>""#,
            id,
            update.name,
            update_description,
            update.description,
            update_base_branch,
            update.base_branch
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!("DELETE FROM task_groups WHERE id = $1", id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Bulk assign tasks to this task group.
    /// Only updates tasks that belong to the same project as the task group.
    pub async fn bulk_assign_tasks(
        pool: &SqlitePool,
        group_id: Uuid,
        project_id: Uuid,
        task_ids: &[Uuid],
    ) -> Result<u64, sqlx::Error> {
        if task_ids.is_empty() {
            return Ok(0);
        }

        let mut query_builder = sqlx::QueryBuilder::new("UPDATE tasks SET task_group_id = ");
        query_builder.push_bind(group_id);
        query_builder.push(", updated_at = datetime('now', 'subsec') WHERE project_id = ");
        query_builder.push_bind(project_id);
        query_builder.push(" AND id IN (");

        let mut separated = query_builder.separated(", ");
        for id in task_ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");

        let result = query_builder.build().execute(pool).await?;
        Ok(result.rows_affected())
    }

    /// Merge source task group into target task group.
    /// Moves all tasks from source to target, then deletes source.
    /// Returns the updated target TaskGroup.
    pub async fn merge_into(
        pool: &SqlitePool,
        source_id: Uuid,
        target_id: Uuid,
    ) -> Result<Self, MergeError> {
        if source_id == target_id {
            return Err(MergeError::SameGroup);
        }

        let source = Self::find_by_id(pool, source_id)
            .await?
            .ok_or(MergeError::SourceNotFound)?;

        let target = Self::find_by_id(pool, target_id)
            .await?
            .ok_or(MergeError::TargetNotFound)?;

        if source.project_id != target.project_id {
            return Err(MergeError::DifferentProjects);
        }

        let mut tx = pool.begin().await?;

        // Move all tasks from source to target
        sqlx::query(
            "UPDATE tasks SET task_group_id = ?, updated_at = datetime('now', 'subsec') WHERE task_group_id = ?",
        )
        .bind(target_id)
        .bind(source_id)
        .execute(&mut *tx)
        .await?;

        // Delete the source group
        sqlx::query("DELETE FROM task_groups WHERE id = ?")
            .bind(source_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        // Re-fetch target to get updated_at if needed (though it wasn't modified)
        Self::find_by_id(pool, target_id)
            .await?
            .ok_or(MergeError::TargetNotFound)
    }

    pub async fn get_stats_for_project(
        pool: &SqlitePool,
        project_id: Uuid,
    ) -> Result<Vec<TaskGroupWithStats>, sqlx::Error> {
        #[derive(FromRow)]
        struct Row {
            id: Uuid,
            project_id: Uuid,
            name: String,
            description: Option<String>,
            base_branch: Option<String>,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
            todo: i64,
            inprogress: i64,
            inreview: i64,
            done: i64,
            cancelled: i64,
        }

        let rows: Vec<Row> = sqlx::query_as(
            r#"SELECT
                tg.id,
                tg.project_id,
                tg.name,
                tg.description,
                tg.base_branch,
                tg.created_at,
                tg.updated_at,
                COALESCE(SUM(CASE WHEN t.status = 'todo' THEN 1 ELSE 0 END), 0) AS todo,
                COALESCE(SUM(CASE WHEN t.status = 'inprogress' THEN 1 ELSE 0 END), 0) AS inprogress,
                COALESCE(SUM(CASE WHEN t.status = 'inreview' THEN 1 ELSE 0 END), 0) AS inreview,
                COALESCE(SUM(CASE WHEN t.status = 'done' THEN 1 ELSE 0 END), 0) AS done,
                COALESCE(SUM(CASE WHEN t.status = 'cancelled' THEN 1 ELSE 0 END), 0) AS cancelled
            FROM task_groups tg
            LEFT JOIN tasks t ON t.task_group_id = tg.id
            WHERE tg.project_id = ?1
            GROUP BY tg.id
            ORDER BY tg.created_at DESC"#,
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| TaskGroupWithStats {
                group: TaskGroup {
                    id: row.id,
                    project_id: row.project_id,
                    name: row.name,
                    description: row.description,
                    base_branch: row.base_branch,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                },
                task_counts: TaskStatusCounts {
                    todo: row.todo,
                    inprogress: row.inprogress,
                    inreview: row.inreview,
                    done: row.done,
                    cancelled: row.cancelled,
                },
            })
            .collect())
    }

    /// Get all unique non-null base branches for task groups in a project.
    /// Returns branches sorted alphabetically.
    pub async fn get_unique_base_branches(
        pool: &SqlitePool,
        project_id: Uuid,
    ) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query_scalar!(
            r#"SELECT DISTINCT base_branch as "base_branch!"
               FROM task_groups
               WHERE project_id = $1 AND base_branch IS NOT NULL
               ORDER BY base_branch"#,
            project_id
        )
        .fetch_all(pool)
        .await?;

        Ok(rows)
    }
}
