use chrono::{DateTime, Utc};
use executors::{actions::ExecutorAction, profile::ExecutorProfileId};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

/// Represents an entry in the execution queue.
/// Presence in this table means the workspace is waiting to execute.
/// When execution starts, the row is deleted.
///
/// For initial workspace starts: session_id and executor_action are None.
/// For follow-up executions: session_id and executor_action are populated.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExecutionQueue {
    pub id: Uuid,
    pub workspace_id: Uuid,
    #[ts(type = "ExecutorProfileId")]
    pub executor_profile_id: sqlx::types::Json<ExecutorProfileId>,
    pub queued_at: DateTime<Utc>,
    /// Session ID for follow-up executions (None for initial workspace starts)
    pub session_id: Option<Uuid>,
    /// Serialized ExecutorAction for follow-up executions (None for initial workspace starts)
    pub executor_action: Option<String>,
}

impl ExecutionQueue {
    /// Check if this is a follow-up queue entry
    pub fn is_follow_up(&self) -> bool {
        self.session_id.is_some() && self.executor_action.is_some()
    }

    /// Parse the executor action (for follow-up entries)
    pub fn parsed_executor_action(&self) -> Option<ExecutorAction> {
        self.executor_action
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
    }

    /// Insert a new queue entry for initial workspace start (waiting to run)
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

    /// Insert a new queue entry for follow-up execution
    pub async fn create_follow_up(
        pool: &SqlitePool,
        workspace_id: Uuid,
        session_id: Uuid,
        executor_action: &ExecutorAction,
    ) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();

        // Extract executor_profile_id from the action
        let executor_profile_id = match executor_action.typ() {
            executors::actions::ExecutorActionType::CodingAgentInitialRequest(req) => {
                req.executor_profile_id.clone()
            }
            executors::actions::ExecutorActionType::CodingAgentFollowUpRequest(req) => {
                req.executor_profile_id.clone()
            }
            executors::actions::ExecutorActionType::ScriptRequest(_) => {
                // Scripts don't have a profile, use a default
                ExecutorProfileId {
                    executor: executors::executors::BaseCodingAgent::ClaudeCode,
                    variant: None,
                }
            }
        };

        let executor_profile_json = serde_json::to_string(&executor_profile_id)
            .map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
        let executor_action_json =
            serde_json::to_string(executor_action).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;

        sqlx::query!(
            r#"INSERT INTO execution_queue (id, workspace_id, executor_profile_id, session_id, executor_action)
               VALUES (?, ?, ?, ?, ?)"#,
            id,
            workspace_id,
            executor_profile_json,
            session_id,
            executor_action_json
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
                queued_at AS "queued_at!: DateTime<Utc>",
                session_id AS "session_id: Uuid",
                executor_action AS "executor_action: String"
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
                queued_at AS "queued_at!: DateTime<Utc>",
                session_id AS "session_id: Uuid",
                executor_action AS "executor_action: String"
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
                queued_at AS "queued_at!: DateTime<Utc>",
                session_id AS "session_id: Uuid",
                executor_action AS "executor_action: String"
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
        let result =
            sqlx::query_scalar!(r#"SELECT COUNT(*) AS "count!: i64" FROM execution_queue"#)
                .fetch_one(pool)
                .await?;
        Ok(result)
    }
}
