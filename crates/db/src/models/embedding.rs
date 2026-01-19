use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

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
    ///
    /// Note: vec0 virtual tables don't support INSERT OR REPLACE or UPSERT,
    /// so we use explicit DELETE + INSERT.
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

        // vec0 virtual tables don't support INSERT OR REPLACE or ON CONFLICT,
        // so we manually delete any existing row first
        sqlx::query("DELETE FROM task_embeddings WHERE task_rowid = $1")
            .bind(task_rowid)
            .execute(pool)
            .await?;

        sqlx::query(
            r#"INSERT INTO task_embeddings(task_rowid, embedding)
               VALUES ($1, $2)"#,
        )
        .bind(task_rowid)
        .bind(&embedding_bytes)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Get the rowid for a task by its UUID.
    /// This is needed because task_embeddings uses rowid, not UUID.
    pub async fn get_task_rowid(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Option<i64>, sqlx::Error> {
        let rowid: Option<i64> = sqlx::query_scalar("SELECT rowid FROM tasks WHERE id = $1")
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
