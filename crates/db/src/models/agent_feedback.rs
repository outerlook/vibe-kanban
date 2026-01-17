use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct AgentFeedback {
    pub id: Uuid,
    pub execution_process_id: Uuid,
    pub task_id: Uuid,
    pub workspace_id: Uuid,
    pub feedback_json: Option<String>,
    pub collected_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, TS)]
pub struct CreateAgentFeedback {
    pub execution_process_id: Uuid,
    pub task_id: Uuid,
    pub workspace_id: Uuid,
    pub feedback_json: Option<String>,
}

impl AgentFeedback {
    pub async fn create(
        pool: &SqlitePool,
        data: &CreateAgentFeedback,
        id: Uuid,
    ) -> Result<Self, sqlx::Error> {
        let now = Utc::now();

        sqlx::query_as!(
            AgentFeedback,
            r#"INSERT INTO agent_feedback (
                id, execution_process_id, task_id, workspace_id,
                feedback_json, collected_at, created_at, updated_at
               )
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               RETURNING
                id as "id!: Uuid",
                execution_process_id as "execution_process_id!: Uuid",
                task_id as "task_id!: Uuid",
                workspace_id as "workspace_id!: Uuid",
                feedback_json,
                collected_at as "collected_at!: DateTime<Utc>",
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>""#,
            id,
            data.execution_process_id,
            data.task_id,
            data.workspace_id,
            data.feedback_json,
            now,
            now,
            now
        )
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            AgentFeedback,
            r#"SELECT
                id as "id!: Uuid",
                execution_process_id as "execution_process_id!: Uuid",
                task_id as "task_id!: Uuid",
                workspace_id as "workspace_id!: Uuid",
                feedback_json,
                collected_at as "collected_at!: DateTime<Utc>",
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>"
               FROM agent_feedback
               WHERE id = $1"#,
            id
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_execution_process_id(
        pool: &SqlitePool,
        execution_process_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            AgentFeedback,
            r#"SELECT
                id as "id!: Uuid",
                execution_process_id as "execution_process_id!: Uuid",
                task_id as "task_id!: Uuid",
                workspace_id as "workspace_id!: Uuid",
                feedback_json,
                collected_at as "collected_at!: DateTime<Utc>",
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>"
               FROM agent_feedback
               WHERE execution_process_id = $1"#,
            execution_process_id
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_task_id(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            AgentFeedback,
            r#"SELECT
                id as "id!: Uuid",
                execution_process_id as "execution_process_id!: Uuid",
                task_id as "task_id!: Uuid",
                workspace_id as "workspace_id!: Uuid",
                feedback_json,
                collected_at as "collected_at!: DateTime<Utc>",
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>"
               FROM agent_feedback
               WHERE task_id = $1
               ORDER BY collected_at DESC"#,
            task_id
        )
        .fetch_all(pool)
        .await
    }

    pub async fn find_recent(pool: &SqlitePool, limit: i64) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            AgentFeedback,
            r#"SELECT
                id as "id!: Uuid",
                execution_process_id as "execution_process_id!: Uuid",
                task_id as "task_id!: Uuid",
                workspace_id as "workspace_id!: Uuid",
                feedback_json,
                collected_at as "collected_at!: DateTime<Utc>",
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>"
               FROM agent_feedback
               ORDER BY collected_at DESC
               LIMIT $1"#,
            limit
        )
        .fetch_all(pool)
        .await
    }
}
