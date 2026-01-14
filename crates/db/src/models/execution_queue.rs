use chrono::{DateTime, Utc};
use executors::profile::ExecutorProfileId;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

/// Represents an entry in the execution queue.
/// Presence in this table means the workspace is waiting to execute.
/// When execution starts, the row is deleted.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ExecutionQueue {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub executor_profile_id: sqlx::types::Json<ExecutorProfileId>,
    pub queued_at: DateTime<Utc>,
}

impl ExecutionQueue {
    /// Insert a new queue entry (waiting to run)
    pub async fn create(
        pool: &SqlitePool,
        workspace_id: Uuid,
        executor_profile_id: &ExecutorProfileId,
    ) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();
        let executor_profile_json = serde_json::to_string(executor_profile_id)
            .map_err(|e| sqlx::Error::Encode(Box::new(e)))?;

        sqlx::query!(
            r#"INSERT INTO execution_queue (id, workspace_id, executor_profile_id)
               VALUES (?, ?, ?)"#,
            id,
            workspace_id,
            executor_profile_json
        )
        .execute(pool)
        .await?;

        Self::find_by_id(pool, id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    /// Find a queue entry by ID
    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            ExecutionQueue,
            r#"SELECT
                id AS "id!: Uuid",
                workspace_id AS "workspace_id!: Uuid",
                executor_profile_id AS "executor_profile_id!: sqlx::types::Json<ExecutorProfileId>",
                queued_at AS "queued_at!: DateTime<Utc>"
            FROM execution_queue
            WHERE id = ?"#,
            id
        )
        .fetch_optional(pool)
        .await
    }

    /// Pop the oldest waiting entry (SELECT + DELETE) - FIFO by queued_at
    /// Returns None if queue is empty
    pub async fn pop_next(pool: &SqlitePool) -> Result<Option<Self>, sqlx::Error> {
        // Get the oldest entry
        let entry = sqlx::query_as!(
            ExecutionQueue,
            r#"SELECT
                id AS "id!: Uuid",
                workspace_id AS "workspace_id!: Uuid",
                executor_profile_id AS "executor_profile_id!: sqlx::types::Json<ExecutorProfileId>",
                queued_at AS "queued_at!: DateTime<Utc>"
            FROM execution_queue
            ORDER BY queued_at ASC
            LIMIT 1"#
        )
        .fetch_optional(pool)
        .await?;

        // If found, delete it
        if let Some(ref e) = entry {
            sqlx::query!("DELETE FROM execution_queue WHERE id = ?", e.id)
                .execute(pool)
                .await?;
        }

        Ok(entry)
    }

    /// Check if a workspace has a pending queue entry
    pub async fn find_by_workspace(
        pool: &SqlitePool,
        workspace_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            ExecutionQueue,
            r#"SELECT
                id AS "id!: Uuid",
                workspace_id AS "workspace_id!: Uuid",
                executor_profile_id AS "executor_profile_id!: sqlx::types::Json<ExecutorProfileId>",
                queued_at AS "queued_at!: DateTime<Utc>"
            FROM execution_queue
            WHERE workspace_id = ?"#,
            workspace_id
        )
        .fetch_optional(pool)
        .await
    }

    /// Cancel/remove a workspace's entry from the queue
    pub async fn delete_by_workspace(
        pool: &SqlitePool,
        workspace_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "DELETE FROM execution_queue WHERE workspace_id = ?",
            workspace_id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Get total count of entries in the queue
    pub async fn count(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
        let result = sqlx::query_scalar!(r#"SELECT COUNT(*) AS "count!: i64" FROM execution_queue"#)
            .fetch_one(pool)
            .await?;
        Ok(result)
    }
}
