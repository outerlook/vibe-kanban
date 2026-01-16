use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool, Type};
use thiserror::Error;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Type, Serialize, Deserialize, PartialEq, TS)]
#[sqlx(type_name = "conversation_session_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum ConversationSessionStatus {
    Active,
    Archived,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct ConversationSession {
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: String,
    pub status: ConversationSessionStatus,
    pub executor: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreateConversationSession {
    pub project_id: Uuid,
    pub title: String,
    pub executor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct UpdateConversationSession {
    pub title: Option<String>,
    pub status: Option<ConversationSessionStatus>,
    pub executor: Option<String>,
}

#[derive(Debug, Error)]
pub enum ConversationSessionError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("Conversation session not found")]
    NotFound,
    #[error("Project not found")]
    ProjectNotFound,
}

impl ConversationSession {
    pub async fn create(
        pool: &SqlitePool,
        data: CreateConversationSession,
    ) -> Result<Self, ConversationSessionError> {
        let id = Uuid::new_v4();
        let status = ConversationSessionStatus::Active;

        sqlx::query_as!(
            Self,
            r#"INSERT INTO conversation_sessions (id, project_id, title, status, executor)
               VALUES ($1, $2, $3, $4, $5)
               RETURNING id AS "id!: Uuid",
                         project_id AS "project_id!: Uuid",
                         title,
                         status AS "status!: ConversationSessionStatus",
                         executor,
                         created_at AS "created_at!: DateTime<Utc>",
                         updated_at AS "updated_at!: DateTime<Utc>""#,
            id,
            data.project_id,
            data.title,
            status,
            data.executor,
        )
        .fetch_one(pool)
        .await
        .map_err(ConversationSessionError::from)
    }

    pub async fn find_by_id(
        pool: &SqlitePool,
        id: Uuid,
    ) -> Result<Option<Self>, ConversationSessionError> {
        let session = sqlx::query_as!(
            Self,
            r#"SELECT id AS "id!: Uuid",
                      project_id AS "project_id!: Uuid",
                      title,
                      status AS "status!: ConversationSessionStatus",
                      executor,
                      created_at AS "created_at!: DateTime<Utc>",
                      updated_at AS "updated_at!: DateTime<Utc>"
               FROM conversation_sessions
               WHERE id = $1"#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(session)
    }

    pub async fn find_by_project_id(
        pool: &SqlitePool,
        project_id: Uuid,
    ) -> Result<Vec<Self>, ConversationSessionError> {
        let sessions = sqlx::query_as!(
            Self,
            r#"SELECT id AS "id!: Uuid",
                      project_id AS "project_id!: Uuid",
                      title,
                      status AS "status!: ConversationSessionStatus",
                      executor,
                      created_at AS "created_at!: DateTime<Utc>",
                      updated_at AS "updated_at!: DateTime<Utc>"
               FROM conversation_sessions
               WHERE project_id = $1
               ORDER BY updated_at DESC"#,
            project_id
        )
        .fetch_all(pool)
        .await?;

        Ok(sessions)
    }

    pub async fn find_active_by_project_id(
        pool: &SqlitePool,
        project_id: Uuid,
    ) -> Result<Vec<Self>, ConversationSessionError> {
        let status = ConversationSessionStatus::Active;
        let sessions = sqlx::query_as!(
            Self,
            r#"SELECT id AS "id!: Uuid",
                      project_id AS "project_id!: Uuid",
                      title,
                      status AS "status!: ConversationSessionStatus",
                      executor,
                      created_at AS "created_at!: DateTime<Utc>",
                      updated_at AS "updated_at!: DateTime<Utc>"
               FROM conversation_sessions
               WHERE project_id = $1 AND status = $2
               ORDER BY updated_at DESC"#,
            project_id,
            status
        )
        .fetch_all(pool)
        .await?;

        Ok(sessions)
    }

    pub async fn update(
        pool: &SqlitePool,
        id: Uuid,
        update: &UpdateConversationSession,
    ) -> Result<Option<Self>, ConversationSessionError> {
        let update_executor = update.executor.is_some();
        sqlx::query_as!(
            Self,
            r#"UPDATE conversation_sessions
               SET title = COALESCE($2, title),
                   status = COALESCE($3, status),
                   executor = CASE WHEN $4 THEN $5 ELSE executor END,
                   updated_at = datetime('now', 'subsec')
               WHERE id = $1
               RETURNING id AS "id!: Uuid",
                         project_id AS "project_id!: Uuid",
                         title,
                         status AS "status!: ConversationSessionStatus",
                         executor,
                         created_at AS "created_at!: DateTime<Utc>",
                         updated_at AS "updated_at!: DateTime<Utc>""#,
            id,
            update.title,
            update.status,
            update_executor,
            update.executor
        )
        .fetch_optional(pool)
        .await
        .map_err(ConversationSessionError::from)
    }

    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<u64, ConversationSessionError> {
        let result = sqlx::query!("DELETE FROM conversation_sessions WHERE id = $1", id)
            .execute(pool)
            .await?;

        Ok(result.rows_affected())
    }
}
