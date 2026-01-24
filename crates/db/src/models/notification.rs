use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool, Type};
use strum_macros::{Display, EnumString};
use ts_rs::TS;
use uuid::Uuid;

#[derive(
    Debug, Clone, Type, Serialize, Deserialize, PartialEq, TS, EnumString, Display, Default,
)]
#[sqlx(type_name = "notification_type", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum NotificationType {
    #[default]
    AgentComplete,
    AgentApprovalNeeded,
    AgentError,
    ConversationResponse,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct Notification {
    pub id: Uuid,
    pub project_id: Option<Uuid>,
    pub notification_type: NotificationType,
    pub title: String,
    pub message: String,
    pub is_read: bool,
    pub metadata: Option<serde_json::Value>,
    pub workspace_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub conversation_session_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreateNotification {
    pub project_id: Option<Uuid>,
    pub notification_type: NotificationType,
    pub title: String,
    pub message: String,
    pub metadata: Option<serde_json::Value>,
    pub workspace_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub conversation_session_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct UpdateNotification {
    pub title: Option<String>,
    pub message: Option<String>,
    pub is_read: Option<bool>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct NotificationStats {
    pub total: i64,
    pub unread: i64,
}

impl Notification {
    pub async fn create(pool: &SqlitePool, data: &CreateNotification) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();
        let metadata_json = data.metadata.as_ref().map(|m| m.to_string());
        let notification_type_str = data.notification_type.to_string();

        sqlx::query_as!(
            Notification,
            r#"INSERT INTO notifications (id, project_id, notification_type, title, message, metadata, workspace_id, session_id, conversation_session_id)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
               RETURNING id AS "id!: Uuid",
                         project_id AS "project_id: Uuid",
                         notification_type AS "notification_type!: NotificationType",
                         title,
                         message,
                         is_read AS "is_read!: bool",
                         metadata AS "metadata: serde_json::Value",
                         workspace_id AS "workspace_id: Uuid",
                         session_id AS "session_id: Uuid",
                         conversation_session_id AS "conversation_session_id: Uuid",
                         created_at AS "created_at!: DateTime<Utc>",
                         updated_at AS "updated_at!: DateTime<Utc>""#,
            id,
            data.project_id,
            notification_type_str,
            data.title,
            data.message,
            metadata_json,
            data.workspace_id,
            data.session_id,
            data.conversation_session_id
        )
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Notification,
            r#"SELECT id AS "id!: Uuid",
                      project_id AS "project_id: Uuid",
                      notification_type AS "notification_type!: NotificationType",
                      title,
                      message,
                      is_read AS "is_read!: bool",
                      metadata AS "metadata: serde_json::Value",
                      workspace_id AS "workspace_id: Uuid",
                      session_id AS "session_id: Uuid",
                      conversation_session_id AS "conversation_session_id: Uuid",
                      created_at AS "created_at!: DateTime<Utc>",
                      updated_at AS "updated_at!: DateTime<Utc>"
               FROM notifications
               WHERE id = $1"#,
            id
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_rowid(pool: &SqlitePool, rowid: i64) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Notification,
            r#"SELECT id AS "id!: Uuid",
                      project_id AS "project_id: Uuid",
                      notification_type AS "notification_type!: NotificationType",
                      title,
                      message,
                      is_read AS "is_read!: bool",
                      metadata AS "metadata: serde_json::Value",
                      workspace_id AS "workspace_id: Uuid",
                      session_id AS "session_id: Uuid",
                      conversation_session_id AS "conversation_session_id: Uuid",
                      created_at AS "created_at!: DateTime<Utc>",
                      updated_at AS "updated_at!: DateTime<Utc>"
               FROM notifications
               WHERE rowid = $1"#,
            rowid
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_project_id(
        pool: &SqlitePool,
        project_id: Uuid,
        limit: Option<i64>,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let limit = limit.unwrap_or(100);
        sqlx::query_as!(
            Notification,
            r#"SELECT id AS "id!: Uuid",
                      project_id AS "project_id: Uuid",
                      notification_type AS "notification_type!: NotificationType",
                      title,
                      message,
                      is_read AS "is_read!: bool",
                      metadata AS "metadata: serde_json::Value",
                      workspace_id AS "workspace_id: Uuid",
                      session_id AS "session_id: Uuid",
                      conversation_session_id AS "conversation_session_id: Uuid",
                      created_at AS "created_at!: DateTime<Utc>",
                      updated_at AS "updated_at!: DateTime<Utc>"
               FROM notifications
               WHERE project_id = $1
               ORDER BY created_at DESC
               LIMIT $2"#,
            project_id,
            limit
        )
        .fetch_all(pool)
        .await
    }

    pub async fn find_global(
        pool: &SqlitePool,
        limit: Option<i64>,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let limit = limit.unwrap_or(100);
        sqlx::query_as!(
            Notification,
            r#"SELECT id AS "id!: Uuid",
                      project_id AS "project_id: Uuid",
                      notification_type AS "notification_type!: NotificationType",
                      title,
                      message,
                      is_read AS "is_read!: bool",
                      metadata AS "metadata: serde_json::Value",
                      workspace_id AS "workspace_id: Uuid",
                      session_id AS "session_id: Uuid",
                      conversation_session_id AS "conversation_session_id: Uuid",
                      created_at AS "created_at!: DateTime<Utc>",
                      updated_at AS "updated_at!: DateTime<Utc>"
               FROM notifications
               WHERE project_id IS NULL
               ORDER BY created_at DESC
               LIMIT $1"#,
            limit
        )
        .fetch_all(pool)
        .await
    }

    pub async fn update(
        pool: &SqlitePool,
        id: Uuid,
        update: &UpdateNotification,
    ) -> Result<Option<Self>, sqlx::Error> {
        let update_metadata = update.metadata.is_some();
        let metadata_json = update.metadata.as_ref().map(|m| m.to_string());

        sqlx::query_as!(
            Notification,
            r#"UPDATE notifications
               SET title = COALESCE($2, title),
                   message = COALESCE($3, message),
                   is_read = COALESCE($4, is_read),
                   metadata = CASE WHEN $5 THEN $6 ELSE metadata END,
                   updated_at = datetime('now', 'subsec')
               WHERE id = $1
               RETURNING id AS "id!: Uuid",
                         project_id AS "project_id: Uuid",
                         notification_type AS "notification_type!: NotificationType",
                         title,
                         message,
                         is_read AS "is_read!: bool",
                         metadata AS "metadata: serde_json::Value",
                         workspace_id AS "workspace_id: Uuid",
                         session_id AS "session_id: Uuid",
                         conversation_session_id AS "conversation_session_id: Uuid",
                         created_at AS "created_at!: DateTime<Utc>",
                         updated_at AS "updated_at!: DateTime<Utc>""#,
            id,
            update.title,
            update.message,
            update.is_read,
            update_metadata,
            metadata_json
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!("DELETE FROM notifications WHERE id = $1", id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn mark_all_read(
        pool: &SqlitePool,
        project_id: Option<Uuid>,
    ) -> Result<u64, sqlx::Error> {
        let result = if let Some(pid) = project_id {
            sqlx::query!(
                "UPDATE notifications SET is_read = 1, updated_at = datetime('now', 'subsec') WHERE project_id = $1 AND is_read = 0",
                pid
            )
            .execute(pool)
            .await?
        } else {
            sqlx::query!(
                "UPDATE notifications SET is_read = 1, updated_at = datetime('now', 'subsec') WHERE project_id IS NULL AND is_read = 0"
            )
            .execute(pool)
            .await?
        };
        Ok(result.rows_affected())
    }

    pub async fn get_stats(
        pool: &SqlitePool,
        project_id: Option<Uuid>,
    ) -> Result<NotificationStats, sqlx::Error> {
        if let Some(pid) = project_id {
            let stats = sqlx::query!(
                r#"SELECT
                    COUNT(*) AS "total!: i64",
                    SUM(CASE WHEN is_read = 0 THEN 1 ELSE 0 END) AS "unread!: i64"
                   FROM notifications
                   WHERE project_id = $1"#,
                pid
            )
            .fetch_one(pool)
            .await?;

            Ok(NotificationStats {
                total: stats.total,
                unread: stats.unread,
            })
        } else {
            let stats = sqlx::query!(
                r#"SELECT
                    COUNT(*) AS "total!: i64",
                    SUM(CASE WHEN is_read = 0 THEN 1 ELSE 0 END) AS "unread!: i64"
                   FROM notifications
                   WHERE project_id IS NULL"#
            )
            .fetch_one(pool)
            .await?;

            Ok(NotificationStats {
                total: stats.total,
                unread: stats.unread,
            })
        }
    }
}
