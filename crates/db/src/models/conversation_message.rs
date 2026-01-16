use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool, Type};
use thiserror::Error;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Type, Serialize, Deserialize, PartialEq, TS)]
#[sqlx(type_name = "message_role", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct ConversationMessage {
    pub id: Uuid,
    pub conversation_session_id: Uuid,
    pub execution_process_id: Option<Uuid>,
    pub role: MessageRole,
    pub content: String,
    pub metadata: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreateConversationMessage {
    pub conversation_session_id: Uuid,
    pub execution_process_id: Option<Uuid>,
    pub role: MessageRole,
    pub content: String,
    pub metadata: Option<String>,
}

#[derive(Debug, Error)]
pub enum ConversationMessageError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("Conversation message not found")]
    NotFound,
    #[error("Conversation session not found")]
    ConversationSessionNotFound,
}

impl ConversationMessage {
    pub async fn create(
        pool: &SqlitePool,
        data: CreateConversationMessage,
    ) -> Result<Self, ConversationMessageError> {
        let id = Uuid::new_v4();

        sqlx::query_as!(
            Self,
            r#"INSERT INTO conversation_messages (id, conversation_session_id, execution_process_id, role, content, metadata)
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING id AS "id!: Uuid",
                         conversation_session_id AS "conversation_session_id!: Uuid",
                         execution_process_id AS "execution_process_id: Uuid",
                         role AS "role!: MessageRole",
                         content,
                         metadata,
                         created_at AS "created_at!: DateTime<Utc>",
                         updated_at AS "updated_at!: DateTime<Utc>""#,
            id,
            data.conversation_session_id,
            data.execution_process_id,
            data.role,
            data.content,
            data.metadata,
        )
        .fetch_one(pool)
        .await
        .map_err(ConversationMessageError::from)
    }

    pub async fn find_by_id(
        pool: &SqlitePool,
        id: Uuid,
    ) -> Result<Option<Self>, ConversationMessageError> {
        let message = sqlx::query_as!(
            Self,
            r#"SELECT id AS "id!: Uuid",
                      conversation_session_id AS "conversation_session_id!: Uuid",
                      execution_process_id AS "execution_process_id: Uuid",
                      role AS "role!: MessageRole",
                      content,
                      metadata,
                      created_at AS "created_at!: DateTime<Utc>",
                      updated_at AS "updated_at!: DateTime<Utc>"
               FROM conversation_messages
               WHERE id = $1"#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(message)
    }

    pub async fn find_by_conversation_session_id(
        pool: &SqlitePool,
        conversation_session_id: Uuid,
    ) -> Result<Vec<Self>, ConversationMessageError> {
        let messages = sqlx::query_as!(
            Self,
            r#"SELECT id AS "id!: Uuid",
                      conversation_session_id AS "conversation_session_id!: Uuid",
                      execution_process_id AS "execution_process_id: Uuid",
                      role AS "role!: MessageRole",
                      content,
                      metadata,
                      created_at AS "created_at!: DateTime<Utc>",
                      updated_at AS "updated_at!: DateTime<Utc>"
               FROM conversation_messages
               WHERE conversation_session_id = $1
               ORDER BY created_at ASC"#,
            conversation_session_id
        )
        .fetch_all(pool)
        .await?;

        Ok(messages)
    }

    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<u64, ConversationMessageError> {
        let result = sqlx::query!("DELETE FROM conversation_messages WHERE id = $1", id)
            .execute(pool)
            .await?;

        Ok(result.rows_affected())
    }
}
