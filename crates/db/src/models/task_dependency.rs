use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use thiserror::Error;
use ts_rs::TS;
use uuid::Uuid;

use super::task::{Task, TaskStatus};

#[derive(Debug, Error)]
pub enum TaskDependencyError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("Task not found")]
    TaskNotFound,
    #[error("Tasks must belong to the same project")]
    DifferentProjects,
    #[error("Adding this dependency would create a cycle")]
    CycleDetected,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct TaskDependency {
    pub id: Uuid,
    pub task_id: Uuid,
    pub depends_on_id: Uuid,
    pub created_at: DateTime<Utc>,
}

impl TaskDependency {
    pub async fn create(
        pool: &SqlitePool,
        task_id: Uuid,
        depends_on_id: Uuid,
    ) -> Result<Self, TaskDependencyError> {
        Self::validate_same_project(pool, task_id, depends_on_id).await?;

        if Self::would_create_cycle(pool, task_id, depends_on_id).await? {
            return Err(TaskDependencyError::CycleDetected);
        }

        let id = Uuid::new_v4();
        let dependency = sqlx::query_as!(
            TaskDependency,
            r#"INSERT INTO task_dependencies (id, task_id, depends_on_id)
               VALUES ($1, $2, $3)
               RETURNING id as "id!: Uuid",
                         task_id as "task_id!: Uuid",
                         depends_on_id as "depends_on_id!: Uuid",
                         created_at as "created_at!: DateTime<Utc>""#,
            id,
            task_id,
            depends_on_id
        )
        .fetch_one(pool)
        .await?;

        Ok(dependency)
    }

    pub async fn delete(
        pool: &SqlitePool,
        task_id: Uuid,
        depends_on_id: Uuid,
    ) -> Result<(), TaskDependencyError> {
        sqlx::query!(
            "DELETE FROM task_dependencies WHERE task_id = $1 AND depends_on_id = $2",
            task_id,
            depends_on_id
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn find_blocked_by(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Vec<Task>, sqlx::Error> {
        sqlx::query_as!(
            Task,
            r#"SELECT t.id as "id!: Uuid",
                      t.project_id as "project_id!: Uuid",
                      t.title,
                      t.description,
                      t.status as "status!: TaskStatus",
                      t.parent_workspace_id as "parent_workspace_id: Uuid",
                      t.shared_task_id as "shared_task_id: Uuid",
                      t.created_at as "created_at!: DateTime<Utc>",
                      t.updated_at as "updated_at!: DateTime<Utc>"
               FROM task_dependencies td
               JOIN tasks t ON t.id = td.depends_on_id
               WHERE td.task_id = $1
               ORDER BY td.created_at DESC"#,
            task_id
        )
        .fetch_all(pool)
        .await
    }

    pub async fn find_blocking(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Vec<Task>, sqlx::Error> {
        sqlx::query_as!(
            Task,
            r#"SELECT t.id as "id!: Uuid",
                      t.project_id as "project_id!: Uuid",
                      t.title,
                      t.description,
                      t.status as "status!: TaskStatus",
                      t.parent_workspace_id as "parent_workspace_id: Uuid",
                      t.shared_task_id as "shared_task_id: Uuid",
                      t.created_at as "created_at!: DateTime<Utc>",
                      t.updated_at as "updated_at!: DateTime<Utc>"
               FROM task_dependencies td
               JOIN tasks t ON t.id = td.task_id
               WHERE td.depends_on_id = $1
               ORDER BY td.created_at DESC"#,
            task_id
        )
        .fetch_all(pool)
        .await
    }

    pub async fn would_create_cycle(
        pool: &SqlitePool,
        task_id: Uuid,
        depends_on_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        if task_id == depends_on_id {
            return Ok(true);
        }

        let mut visited = HashSet::new();
        let mut stack = vec![depends_on_id];

        while let Some(current_id) = stack.pop() {
            if !visited.insert(current_id) {
                continue;
            }

            if current_id == task_id {
                return Ok(true);
            }

            let next_ids = sqlx::query_scalar!(
                r#"SELECT depends_on_id as "depends_on_id!: Uuid"
                   FROM task_dependencies
                   WHERE task_id = $1"#,
                current_id
            )
            .fetch_all(pool)
            .await?;

            stack.extend(next_ids);
        }

        Ok(false)
    }

    async fn validate_same_project(
        pool: &SqlitePool,
        task_id: Uuid,
        depends_on_id: Uuid,
    ) -> Result<(), TaskDependencyError> {
        let record = sqlx::query!(
            r#"SELECT t1.project_id as "task_project_id: Uuid",
                      t2.project_id as "depends_on_project_id: Uuid"
               FROM tasks t1
               JOIN tasks t2 ON t2.id = $2
               WHERE t1.id = $1"#,
            task_id,
            depends_on_id
        )
        .fetch_optional(pool)
        .await?;

        let record = record.ok_or(TaskDependencyError::TaskNotFound)?;
        if record.task_project_id != record.depends_on_project_id {
            return Err(TaskDependencyError::DifferentProjects);
        }

        Ok(())
    }
}
