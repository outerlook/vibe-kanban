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
    pub worktree_path: Option<String>,
    pub worktree_branch: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreateConversationSession {
    pub project_id: Uuid,
    pub title: String,
    pub executor: Option<String>,
    pub worktree_path: Option<String>,
    pub worktree_branch: Option<String>,
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
            r#"INSERT INTO conversation_sessions (id, project_id, title, status, executor, worktree_path, worktree_branch)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               RETURNING id AS "id!: Uuid",
                         project_id AS "project_id!: Uuid",
                         title,
                         status AS "status!: ConversationSessionStatus",
                         executor,
                         worktree_path,
                         worktree_branch,
                         created_at AS "created_at!: DateTime<Utc>",
                         updated_at AS "updated_at!: DateTime<Utc>""#,
            id,
            data.project_id,
            data.title,
            status,
            data.executor,
            data.worktree_path,
            data.worktree_branch,
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
                      worktree_path,
                      worktree_branch,
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

    /// Find conversations by project with optional worktree filtering.
    ///
    /// # Filter modes:
    /// - `None` or `Some("__all__")` - Returns all conversations
    /// - `Some("__main__")` - Returns only conversations with null worktree_path (main repo)
    /// - `Some(path)` - Returns conversations matching the specific worktree path
    pub async fn find_by_project_id(
        pool: &SqlitePool,
        project_id: Uuid,
        worktree_path: Option<&str>,
    ) -> Result<Vec<Self>, ConversationSessionError> {
        // Handle special filter values
        let (filter_all, filter_main, filter_path) = match worktree_path {
            None | Some("__all__") => (true, false, None),
            Some("__main__") => (false, true, None),
            Some(path) => (false, false, Some(path)),
        };

        let sessions = sqlx::query_as!(
            Self,
            r#"SELECT id AS "id!: Uuid",
                      project_id AS "project_id!: Uuid",
                      title,
                      status AS "status!: ConversationSessionStatus",
                      executor,
                      worktree_path,
                      worktree_branch,
                      created_at AS "created_at!: DateTime<Utc>",
                      updated_at AS "updated_at!: DateTime<Utc>"
               FROM conversation_sessions
               WHERE project_id = $1
                 AND (
                     $2 = 1  -- filter_all: return everything
                     OR ($3 = 1 AND worktree_path IS NULL)  -- filter_main: only null worktree_path
                     OR worktree_path = $4  -- filter_path: match specific path
                 )
               ORDER BY updated_at DESC"#,
            project_id,
            filter_all,
            filter_main,
            filter_path
        )
        .fetch_all(pool)
        .await?;

        Ok(sessions)
    }

    /// Find active conversations by project with optional worktree filtering.
    ///
    /// # Filter modes:
    /// - `None` or `Some("__all__")` - Returns all active conversations
    /// - `Some("__main__")` - Returns only active conversations with null worktree_path (main repo)
    /// - `Some(path)` - Returns active conversations matching the specific worktree path
    pub async fn find_active_by_project_id(
        pool: &SqlitePool,
        project_id: Uuid,
        worktree_path: Option<&str>,
    ) -> Result<Vec<Self>, ConversationSessionError> {
        let status = ConversationSessionStatus::Active;

        // Handle special filter values
        let (filter_all, filter_main, filter_path) = match worktree_path {
            None | Some("__all__") => (true, false, None),
            Some("__main__") => (false, true, None),
            Some(path) => (false, false, Some(path)),
        };

        let sessions = sqlx::query_as!(
            Self,
            r#"SELECT id AS "id!: Uuid",
                      project_id AS "project_id!: Uuid",
                      title,
                      status AS "status!: ConversationSessionStatus",
                      executor,
                      worktree_path,
                      worktree_branch,
                      created_at AS "created_at!: DateTime<Utc>",
                      updated_at AS "updated_at!: DateTime<Utc>"
               FROM conversation_sessions
               WHERE project_id = $1 AND status = $2
                 AND (
                     $3 = 1  -- filter_all: return everything
                     OR ($4 = 1 AND worktree_path IS NULL)  -- filter_main: only null worktree_path
                     OR worktree_path = $5  -- filter_path: match specific path
                 )
               ORDER BY updated_at DESC"#,
            project_id,
            status,
            filter_all,
            filter_main,
            filter_path
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
                         worktree_path,
                         worktree_branch,
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
