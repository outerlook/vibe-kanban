use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct TaskGroup {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub base_branch: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreateTaskGroup {
    pub project_id: Uuid,
    pub name: String,
    pub base_branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct UpdateTaskGroup {
    pub name: Option<String>,
    pub base_branch: Option<String>,
}

impl TaskGroup {
    pub async fn create(
        pool: &SqlitePool,
        project_id: Uuid,
        name: String,
        base_branch: Option<String>,
    ) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();
        sqlx::query_as!(
            TaskGroup,
            r#"INSERT INTO task_groups (id, project_id, name, base_branch)
               VALUES ($1, $2, $3, $4)
               RETURNING id as "id!: Uuid",
                         project_id as "project_id!: Uuid",
                         name,
                         base_branch,
                         created_at as "created_at!: DateTime<Utc>",
                         updated_at as "updated_at!: DateTime<Utc>""#,
            id,
            project_id,
            name,
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
        let update_base_branch = update.base_branch.is_some();
        sqlx::query_as!(
            TaskGroup,
            r#"UPDATE task_groups
               SET name = COALESCE($2, name),
                   base_branch = CASE WHEN $3 THEN $4 ELSE base_branch END,
                   updated_at = datetime('now', 'subsec')
               WHERE id = $1
               RETURNING id as "id!: Uuid",
                         project_id as "project_id!: Uuid",
                         name,
                         base_branch,
                         created_at as "created_at!: DateTime<Utc>",
                         updated_at as "updated_at!: DateTime<Utc>""#,
            id,
            update.name,
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

        let mut query_builder = sqlx::QueryBuilder::new(
            "UPDATE tasks SET task_group_id = ",
        );
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
}
