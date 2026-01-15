use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Row, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

use super::task::Task;

/// Expected embedding dimension for BGE-small-en-v1.5
pub const EMBEDDING_DIMENSION: usize = 384;

/// Status tracking for task embeddings.
/// Tracks whether a task needs embedding and when it was last embedded.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct EmbeddingStatus {
    pub task_id: Uuid,
    pub needs_embedding: bool,
    pub last_embedded_at: Option<DateTime<Utc>>,
    pub embedding_model: Option<String>,
}

/// Result of a vector similarity search.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SimilarTask {
    pub task: Task,
    /// Cosine distance (0 = identical, 2 = opposite)
    pub distance: f64,
}

impl EmbeddingStatus {
    /// Find tasks that need embeddings generated.
    /// Returns tasks where needs_embedding = 1, limited by the specified count.
    pub async fn find_pending(pool: &SqlitePool, limit: i64) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            EmbeddingStatus,
            r#"SELECT
                task_id as "task_id!: Uuid",
                needs_embedding as "needs_embedding!: bool",
                last_embedded_at as "last_embedded_at: DateTime<Utc>",
                embedding_model
            FROM task_embedding_status
            WHERE needs_embedding = 1
            LIMIT $1"#,
            limit
        )
        .fetch_all(pool)
        .await
    }

    /// Mark a task as embedded with the given model name.
    /// Sets needs_embedding = 0 and updates last_embedded_at to now.
    pub async fn mark_embedded(
        pool: &SqlitePool,
        task_id: Uuid,
        model_name: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE task_embedding_status
               SET needs_embedding = 0,
                   last_embedded_at = CURRENT_TIMESTAMP,
                   embedding_model = $2
               WHERE task_id = $1"#,
            task_id,
            model_name
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Get the embedding status for a specific task.
    pub async fn find_by_task_id(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            EmbeddingStatus,
            r#"SELECT
                task_id as "task_id!: Uuid",
                needs_embedding as "needs_embedding!: bool",
                last_embedded_at as "last_embedded_at: DateTime<Utc>",
                embedding_model
            FROM task_embedding_status
            WHERE task_id = $1"#,
            task_id
        )
        .fetch_optional(pool)
        .await
    }

    /// Mark a task as needing re-embedding.
    /// This is called when task content changes.
    pub async fn invalidate(pool: &SqlitePool, task_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE task_embedding_status
               SET needs_embedding = 1
               WHERE task_id = $1"#,
            task_id
        )
        .execute(pool)
        .await?;
        Ok(())
    }
}

/// Operations for task embeddings stored in the sqlite-vec virtual table.
/// Note: The task_embeddings table must be created at runtime after loading sqlite-vec.
pub struct TaskEmbedding;

impl TaskEmbedding {
    /// Serialize a float vector to bytes for sqlite-vec storage.
    /// Uses little-endian format as expected by sqlite-vec.
    pub fn serialize_embedding(embedding: &[f32]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(embedding.len() * 4);
        for &value in embedding {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    /// Deserialize bytes back to a float vector.
    pub fn deserialize_embedding(bytes: &[u8]) -> Vec<f32> {
        bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect()
    }

    /// Create the task_embeddings virtual table if it doesn't exist.
    /// This must be called after sqlite-vec extension is loaded.
    /// Returns Ok(true) if table was created, Ok(false) if it already exists.
    pub async fn ensure_table_exists(pool: &SqlitePool) -> Result<bool, sqlx::Error> {
        // Check if table exists
        let exists: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(
                SELECT 1 FROM sqlite_master
                WHERE type = 'table' AND name = 'task_embeddings'
            )"#,
        )
        .fetch_one(pool)
        .await?;

        if exists {
            return Ok(false);
        }

        // Create the vec0 virtual table
        // 384 dimensions for BGE-small-en-v1.5
        sqlx::query(
            r#"CREATE VIRTUAL TABLE task_embeddings USING vec0(
                task_rowid INTEGER PRIMARY KEY,
                embedding FLOAT[384]
            )"#,
        )
        .execute(pool)
        .await?;

        Ok(true)
    }

    /// Insert or update an embedding for a task.
    /// Uses the task's rowid as the key for efficient joins.
    pub async fn upsert(
        pool: &SqlitePool,
        task_rowid: i64,
        embedding: &[f32],
    ) -> Result<(), sqlx::Error> {
        if embedding.len() != EMBEDDING_DIMENSION {
            return Err(sqlx::Error::Protocol(format!(
                "Embedding dimension mismatch: expected {}, got {}",
                EMBEDDING_DIMENSION,
                embedding.len()
            )));
        }

        let embedding_bytes = Self::serialize_embedding(embedding);

        // sqlite-vec uses INSERT OR REPLACE semantics with vec0
        sqlx::query(
            r#"INSERT OR REPLACE INTO task_embeddings(task_rowid, embedding)
               VALUES ($1, $2)"#,
        )
        .bind(task_rowid)
        .bind(&embedding_bytes)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Delete an embedding for a task.
    pub async fn delete(pool: &SqlitePool, task_rowid: i64) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM task_embeddings WHERE task_rowid = $1")
            .bind(task_rowid)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Search for similar tasks using vector cosine distance.
    /// Returns tasks ordered by similarity (closest first).
    ///
    /// The query vector is compared against all task embeddings in the specified project.
    /// Results are filtered by project_id and limited to the top N matches.
    pub async fn search_similar(
        pool: &SqlitePool,
        query_embedding: &[f32],
        project_id: Uuid,
        limit: i64,
    ) -> Result<Vec<SimilarTask>, sqlx::Error> {
        if query_embedding.len() != EMBEDDING_DIMENSION {
            return Err(sqlx::Error::Protocol(format!(
                "Query embedding dimension mismatch: expected {}, got {}",
                EMBEDDING_DIMENSION,
                query_embedding.len()
            )));
        }

        let query_bytes = Self::serialize_embedding(query_embedding);

        // Use sqlite-vec's KNN search with project filtering
        // vec_distance_cosine returns distance (0 = identical, 2 = opposite)
        let rows = sqlx::query(
            r#"SELECT
                t.id,
                t.project_id,
                t.title,
                t.description,
                t.status,
                t.parent_workspace_id,
                t.shared_task_id,
                t.task_group_id,
                t.created_at,
                t.updated_at,
                vec_distance_cosine(te.embedding, $1) as distance
            FROM task_embeddings te
            JOIN tasks t ON t.rowid = te.task_rowid
            WHERE t.project_id = $2
            ORDER BY distance ASC
            LIMIT $3"#,
        )
        .bind(&query_bytes)
        .bind(project_id)
        .bind(limit)
        .fetch_all(pool)
        .await?;

        let results = rows
            .into_iter()
            .map(|row| {
                let task = Task {
                    id: row.get("id"),
                    project_id: row.get("project_id"),
                    title: row.get("title"),
                    description: row.get("description"),
                    status: row.get("status"),
                    parent_workspace_id: row.get("parent_workspace_id"),
                    shared_task_id: row.get("shared_task_id"),
                    task_group_id: row.get("task_group_id"),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                };
                let distance: f64 = row.get("distance");
                SimilarTask { task, distance }
            })
            .collect();

        Ok(results)
    }

    /// Check if an embedding exists for a task.
    pub async fn exists(pool: &SqlitePool, task_rowid: i64) -> Result<bool, sqlx::Error> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM task_embeddings WHERE task_rowid = $1)",
        )
        .bind(task_rowid)
        .fetch_one(pool)
        .await?;
        Ok(exists)
    }

    /// Get the rowid for a task by its UUID.
    /// This is needed because task_embeddings uses rowid, not UUID.
    pub async fn get_task_rowid(pool: &SqlitePool, task_id: Uuid) -> Result<Option<i64>, sqlx::Error> {
        let rowid: Option<i64> =
            sqlx::query_scalar("SELECT rowid FROM tasks WHERE id = $1")
                .bind(task_id)
                .fetch_optional(pool)
                .await?;
        Ok(rowid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_deserialize_embedding() {
        let original: Vec<f32> = vec![0.1, 0.2, -0.3, 0.0, 1.5];
        let bytes = TaskEmbedding::serialize_embedding(&original);
        let restored = TaskEmbedding::deserialize_embedding(&bytes);

        assert_eq!(original.len(), restored.len());
        for (a, b) in original.iter().zip(restored.iter()) {
            assert!((a - b).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn test_serialize_embedding_length() {
        let embedding: Vec<f32> = vec![0.0; EMBEDDING_DIMENSION];
        let bytes = TaskEmbedding::serialize_embedding(&embedding);
        assert_eq!(bytes.len(), EMBEDDING_DIMENSION * 4);
    }

    #[test]
    fn test_deserialize_empty() {
        let bytes: Vec<u8> = vec![];
        let result = TaskEmbedding::deserialize_embedding(&bytes);
        assert!(result.is_empty());
    }
}
