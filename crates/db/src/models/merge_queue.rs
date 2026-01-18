use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool, Type};
use ts_rs::TS;
use uuid::Uuid;

/// Status of a merge queue entry
#[derive(Debug, Clone, Copy, PartialEq, Eq, Type, Serialize, Deserialize, TS)]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum MergeQueueStatus {
    Queued,
    Merging,
    Conflict,
    Completed,
}

impl MergeQueueStatus {
    fn as_str(&self) -> &'static str {
        match self {
            MergeQueueStatus::Queued => "queued",
            MergeQueueStatus::Merging => "merging",
            MergeQueueStatus::Conflict => "conflict",
            MergeQueueStatus::Completed => "completed",
        }
    }
}

/// Represents an entry in the merge queue.
/// Entries are processed FIFO (oldest first) per project.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MergeQueue {
    pub id: Uuid,
    pub project_id: Uuid,
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub queued_at: DateTime<Utc>,
    #[ts(type = "string")]
    pub status: MergeQueueStatus,
    pub conflict_message: Option<String>,
    pub commit_message: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl MergeQueue {
    /// Create a new merge queue entry
    pub async fn create(
        pool: &SqlitePool,
        project_id: Uuid,
        workspace_id: Uuid,
        repo_id: Uuid,
        commit_message: Option<&str>,
    ) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();

        sqlx::query!(
            r#"INSERT INTO merge_queue (id, project_id, workspace_id, repo_id, commit_message)
               VALUES (?, ?, ?, ?, ?)"#,
            id,
            project_id,
            workspace_id,
            repo_id,
            commit_message
        )
        .execute(pool)
        .await?;

        Self::find_by_id(pool, id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    /// Find a merge queue entry by ID
    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            MergeQueue,
            r#"SELECT
                id AS "id!: Uuid",
                project_id AS "project_id!: Uuid",
                workspace_id AS "workspace_id!: Uuid",
                repo_id AS "repo_id!: Uuid",
                queued_at AS "queued_at!: DateTime<Utc>",
                status AS "status!: MergeQueueStatus",
                conflict_message,
                commit_message,
                started_at AS "started_at: DateTime<Utc>",
                completed_at AS "completed_at: DateTime<Utc>"
            FROM merge_queue
            WHERE id = ?"#,
            id
        )
        .fetch_optional(pool)
        .await
    }

    /// Pop the next queued entry for a project (SELECT + DELETE atomically) - FIFO by queued_at
    /// Only returns entries with status 'queued'.
    /// Returns None if no queued entries exist for the project.
    pub async fn pop_next(
        pool: &SqlitePool,
        project_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        // Get the oldest queued entry for this project
        let entry = sqlx::query_as!(
            MergeQueue,
            r#"SELECT
                id AS "id!: Uuid",
                project_id AS "project_id!: Uuid",
                workspace_id AS "workspace_id!: Uuid",
                repo_id AS "repo_id!: Uuid",
                queued_at AS "queued_at!: DateTime<Utc>",
                status AS "status!: MergeQueueStatus",
                conflict_message,
                commit_message,
                started_at AS "started_at: DateTime<Utc>",
                completed_at AS "completed_at: DateTime<Utc>"
            FROM merge_queue
            WHERE project_id = ? AND status = 'queued'
            ORDER BY queued_at ASC
            LIMIT 1"#,
            project_id
        )
        .fetch_optional(pool)
        .await?;

        // If found, delete it
        if let Some(ref e) = entry {
            sqlx::query!("DELETE FROM merge_queue WHERE id = ?", e.id)
                .execute(pool)
                .await?;
        }

        Ok(entry)
    }

    /// Claim the next queued entry for processing by atomically updating its status to 'merging'.
    /// Unlike `pop_next`, this preserves the entry in the database for status tracking.
    /// Returns None if no queued entries exist for the project.
    pub async fn claim_next(
        pool: &SqlitePool,
        project_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        // Get the oldest queued entry for this project
        let entry = sqlx::query_as!(
            MergeQueue,
            r#"SELECT
                id AS "id!: Uuid",
                project_id AS "project_id!: Uuid",
                workspace_id AS "workspace_id!: Uuid",
                repo_id AS "repo_id!: Uuid",
                queued_at AS "queued_at!: DateTime<Utc>",
                status AS "status!: MergeQueueStatus",
                conflict_message,
                commit_message,
                started_at AS "started_at: DateTime<Utc>",
                completed_at AS "completed_at: DateTime<Utc>"
            FROM merge_queue
            WHERE project_id = ? AND status = 'queued'
            ORDER BY queued_at ASC
            LIMIT 1"#,
            project_id
        )
        .fetch_optional(pool)
        .await?;

        // If found, update status to 'merging' to claim it
        if let Some(ref e) = entry {
            let now = Utc::now();
            sqlx::query!(
                r#"UPDATE merge_queue
                   SET status = 'merging', started_at = ?
                   WHERE id = ?"#,
                now,
                e.id
            )
            .execute(pool)
            .await?;
        }

        Ok(entry)
    }

    /// Update the status of a merge queue entry
    pub async fn update_status(
        pool: &SqlitePool,
        id: Uuid,
        status: MergeQueueStatus,
        conflict_message: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let status_str = status.as_str();
        let now = Utc::now();

        // Set started_at when transitioning to merging, completed_at when transitioning to terminal states
        let (started_at, completed_at) = match status {
            MergeQueueStatus::Merging => (Some(now), None),
            MergeQueueStatus::Conflict | MergeQueueStatus::Completed => (None, Some(now)),
            MergeQueueStatus::Queued => (None, None),
        };

        sqlx::query!(
            r#"UPDATE merge_queue
               SET status = ?,
                   conflict_message = ?,
                   started_at = COALESCE(?, started_at),
                   completed_at = COALESCE(?, completed_at)
               WHERE id = ?"#,
            status_str,
            conflict_message,
            started_at,
            completed_at,
            id
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Find a merge queue entry by workspace ID
    pub async fn find_by_workspace(
        pool: &SqlitePool,
        workspace_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            MergeQueue,
            r#"SELECT
                id AS "id!: Uuid",
                project_id AS "project_id!: Uuid",
                workspace_id AS "workspace_id!: Uuid",
                repo_id AS "repo_id!: Uuid",
                queued_at AS "queued_at!: DateTime<Utc>",
                status AS "status!: MergeQueueStatus",
                conflict_message,
                commit_message,
                started_at AS "started_at: DateTime<Utc>",
                completed_at AS "completed_at: DateTime<Utc>"
            FROM merge_queue
            WHERE workspace_id = ?"#,
            workspace_id
        )
        .fetch_optional(pool)
        .await
    }

    /// Delete a merge queue entry by workspace ID
    pub async fn delete_by_workspace(
        pool: &SqlitePool,
        workspace_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "DELETE FROM merge_queue WHERE workspace_id = ?",
            workspace_id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Count entries in the merge queue for a project
    pub async fn count_by_project(pool: &SqlitePool, project_id: Uuid) -> Result<i64, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "count!: i64" FROM merge_queue WHERE project_id = ?"#,
            project_id
        )
        .fetch_one(pool)
        .await?;
        Ok(result)
    }

    /// Count entries in the merge queue for a task group
    pub async fn count_by_task_group(
        pool: &SqlitePool,
        task_group_id: Uuid,
    ) -> Result<i64, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "count!: i64"
            FROM merge_queue mq
            JOIN workspaces w ON mq.workspace_id = w.id
            JOIN tasks t ON w.task_id = t.id
            WHERE t.task_group_id = ?"#,
            task_group_id
        )
        .fetch_one(pool)
        .await?;
        Ok(result)
    }

    /// List all merge queue entries for a project, ordered by queued_at (oldest first)
    pub async fn list_by_project(
        pool: &SqlitePool,
        project_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            MergeQueue,
            r#"SELECT
                id AS "id!: Uuid",
                project_id AS "project_id!: Uuid",
                workspace_id AS "workspace_id!: Uuid",
                repo_id AS "repo_id!: Uuid",
                queued_at AS "queued_at!: DateTime<Utc>",
                status AS "status!: MergeQueueStatus",
                conflict_message,
                commit_message,
                started_at AS "started_at: DateTime<Utc>",
                completed_at AS "completed_at: DateTime<Utc>"
            FROM merge_queue
            WHERE project_id = ?
            ORDER BY queued_at ASC"#,
            project_id
        )
        .fetch_all(pool)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_queue_status_as_str() {
        assert_eq!(MergeQueueStatus::Queued.as_str(), "queued");
        assert_eq!(MergeQueueStatus::Merging.as_str(), "merging");
        assert_eq!(MergeQueueStatus::Conflict.as_str(), "conflict");
        assert_eq!(MergeQueueStatus::Completed.as_str(), "completed");
    }

    #[test]
    fn test_merge_queue_status_serialization() {
        // Test that status serializes to lowercase strings
        let queued = serde_json::to_string(&MergeQueueStatus::Queued).unwrap();
        assert_eq!(queued, "\"queued\"");

        let merging = serde_json::to_string(&MergeQueueStatus::Merging).unwrap();
        assert_eq!(merging, "\"merging\"");

        let conflict = serde_json::to_string(&MergeQueueStatus::Conflict).unwrap();
        assert_eq!(conflict, "\"conflict\"");

        let completed = serde_json::to_string(&MergeQueueStatus::Completed).unwrap();
        assert_eq!(completed, "\"completed\"");
    }

    #[test]
    fn test_merge_queue_status_deserialization() {
        let queued: MergeQueueStatus = serde_json::from_str("\"queued\"").unwrap();
        assert_eq!(queued, MergeQueueStatus::Queued);

        let merging: MergeQueueStatus = serde_json::from_str("\"merging\"").unwrap();
        assert_eq!(merging, MergeQueueStatus::Merging);

        let conflict: MergeQueueStatus = serde_json::from_str("\"conflict\"").unwrap();
        assert_eq!(conflict, MergeQueueStatus::Conflict);

        let completed: MergeQueueStatus = serde_json::from_str("\"completed\"").unwrap();
        assert_eq!(completed, MergeQueueStatus::Completed);
    }
}
