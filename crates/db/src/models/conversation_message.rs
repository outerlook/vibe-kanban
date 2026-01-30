use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool, Type};
use thiserror::Error;
use ts_rs::TS;
use uuid::Uuid;

const DEFAULT_PAGE_LIMIT: usize = 50;
const MAX_PAGE_LIMIT: usize = 200;

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
    #[error("Invalid cursor: {0}")]
    InvalidCursor(String),
}

/// Cursor for pagination, encoding (created_at, id) pair
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageCursor {
    pub created_at: DateTime<Utc>,
    pub id: Uuid,
}

impl MessageCursor {
    pub fn new(created_at: DateTime<Utc>, id: Uuid) -> Self {
        Self { created_at, id }
    }

    pub fn encode(&self) -> String {
        let json = serde_json::to_string(self).expect("MessageCursor serialization cannot fail");
        URL_SAFE_NO_PAD.encode(json.as_bytes())
    }

    pub fn decode(cursor: &str) -> Result<Self, ConversationMessageError> {
        let bytes = URL_SAFE_NO_PAD
            .decode(cursor)
            .map_err(|e| ConversationMessageError::InvalidCursor(format!("base64 decode: {e}")))?;
        let json = String::from_utf8(bytes)
            .map_err(|e| ConversationMessageError::InvalidCursor(format!("utf8 decode: {e}")))?;
        serde_json::from_str(&json)
            .map_err(|e| ConversationMessageError::InvalidCursor(format!("json parse: {e}")))
    }
}

/// Paginated response for conversation messages
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ConversationMessagesPage {
    pub messages: Vec<ConversationMessage>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
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

    /// Fetch paginated messages for a conversation session.
    ///
    /// Returns messages in creation order (oldest first) for natural conversation flow.
    /// Uses cursor-based pagination with (created_at, id) for stable ordering.
    pub async fn find_paginated_by_conversation_session_id(
        pool: &SqlitePool,
        conversation_session_id: Uuid,
        cursor: Option<&str>,
        limit: Option<usize>,
    ) -> Result<ConversationMessagesPage, ConversationMessageError> {
        let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT).clamp(1, MAX_PAGE_LIMIT);
        let fetch_limit = (limit + 1) as i64;

        let messages = if let Some(cursor_str) = cursor {
            let cursor = MessageCursor::decode(cursor_str)?;
            let cursor_time = cursor.created_at;
            let cursor_id = cursor.id;

            sqlx::query_as!(
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
                     AND (created_at > $2 OR (created_at = $2 AND id > $3))
                   ORDER BY created_at ASC, id ASC
                   LIMIT $4"#,
                conversation_session_id,
                cursor_time,
                cursor_id,
                fetch_limit
            )
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query_as!(
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
                   ORDER BY created_at ASC, id ASC
                   LIMIT $2"#,
                conversation_session_id,
                fetch_limit
            )
            .fetch_all(pool)
            .await?
        };

        let has_more = messages.len() > limit;
        let messages: Vec<Self> = if has_more {
            messages.into_iter().take(limit).collect()
        } else {
            messages
        };

        let next_cursor = if has_more {
            messages
                .last()
                .map(|m| MessageCursor::new(m.created_at, m.id).encode())
        } else {
            None
        };

        Ok(ConversationMessagesPage {
            messages,
            next_cursor,
            has_more,
        })
    }

    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<u64, ConversationMessageError> {
        let result = sqlx::query!("DELETE FROM conversation_messages WHERE id = $1", id)
            .execute(pool)
            .await?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn test_message_cursor_encode_decode_roundtrip() {
        let created_at = Utc.with_ymd_and_hms(2024, 6, 15, 10, 30, 0).unwrap();
        let id = Uuid::new_v4();

        let cursor = MessageCursor::new(created_at, id);
        let encoded = cursor.encode();
        let decoded = MessageCursor::decode(&encoded).unwrap();

        assert_eq!(decoded.created_at, created_at);
        assert_eq!(decoded.id, id);
    }

    #[test]
    fn test_message_cursor_decode_invalid_base64() {
        let result = MessageCursor::decode("not-valid-base64!!!");
        assert!(matches!(
            result,
            Err(ConversationMessageError::InvalidCursor(msg)) if msg.contains("base64")
        ));
    }

    #[test]
    fn test_message_cursor_decode_invalid_json() {
        let invalid_json = URL_SAFE_NO_PAD.encode(b"not json");
        let result = MessageCursor::decode(&invalid_json);
        assert!(matches!(
            result,
            Err(ConversationMessageError::InvalidCursor(msg)) if msg.contains("json")
        ));
    }

    #[test]
    fn test_message_cursor_decode_missing_fields() {
        let partial_json = URL_SAFE_NO_PAD.encode(b"{\"created_at\": \"2024-01-01T00:00:00Z\"}");
        let result = MessageCursor::decode(&partial_json);
        assert!(matches!(
            result,
            Err(ConversationMessageError::InvalidCursor(msg)) if msg.contains("json")
        ));
    }
}
