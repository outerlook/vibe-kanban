use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct Image {
    pub id: Uuid,
    pub file_path: String, // relative path within cache/images/
    pub original_name: String,
    pub mime_type: Option<String>,
    pub size_bytes: i64,
    pub hash: String, // SHA256 hash for deduplication
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, TS)]
pub struct CreateImage {
    pub file_path: String,
    pub original_name: String,
    pub mime_type: Option<String>,
    pub size_bytes: i64,
    pub hash: String,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct TaskImage {
    pub id: Uuid,
    pub task_id: Uuid,
    pub image_id: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, TS)]
pub struct CreateTaskImage {
    pub task_id: Uuid,
    pub image_id: Uuid,
}

impl Image {
    pub async fn create(pool: &SqlitePool, data: &CreateImage) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();
        sqlx::query_as!(
            Image,
            r#"INSERT INTO images (id, file_path, original_name, mime_type, size_bytes, hash)
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING id as "id!: Uuid", 
                         file_path as "file_path!", 
                         original_name as "original_name!", 
                         mime_type,
                         size_bytes as "size_bytes!",
                         hash as "hash!",
                         created_at as "created_at!: DateTime<Utc>", 
                         updated_at as "updated_at!: DateTime<Utc>""#,
            id,
            data.file_path,
            data.original_name,
            data.mime_type,
            data.size_bytes,
            data.hash,
        )
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_hash(pool: &SqlitePool, hash: &str) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Image,
            r#"SELECT id as "id!: Uuid",
                      file_path as "file_path!",
                      original_name as "original_name!",
                      mime_type,
                      size_bytes as "size_bytes!",
                      hash as "hash!",
                      created_at as "created_at!: DateTime<Utc>",
                      updated_at as "updated_at!: DateTime<Utc>"
               FROM images
               WHERE hash = $1"#,
            hash
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Image,
            r#"SELECT id as "id!: Uuid",
                      file_path as "file_path!",
                      original_name as "original_name!",
                      mime_type,
                      size_bytes as "size_bytes!",
                      hash as "hash!",
                      created_at as "created_at!: DateTime<Utc>",
                      updated_at as "updated_at!: DateTime<Utc>"
               FROM images
               WHERE id = $1"#,
            id
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_file_path(
        pool: &SqlitePool,
        file_path: &str,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Image,
            r#"SELECT id as "id!: Uuid",
                      file_path as "file_path!",
                      original_name as "original_name!",
                      mime_type,
                      size_bytes as "size_bytes!",
                      hash as "hash!",
                      created_at as "created_at!: DateTime<Utc>",
                      updated_at as "updated_at!: DateTime<Utc>"
               FROM images
               WHERE file_path = $1"#,
            file_path
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_task_id(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Image,
            r#"SELECT i.id as "id!: Uuid",
                      i.file_path as "file_path!",
                      i.original_name as "original_name!",
                      i.mime_type,
                      i.size_bytes as "size_bytes!",
                      i.hash as "hash!",
                      i.created_at as "created_at!: DateTime<Utc>",
                      i.updated_at as "updated_at!: DateTime<Utc>"
               FROM images i
               JOIN task_images ti ON i.id = ti.image_id
               WHERE ti.task_id = $1
               ORDER BY ti.created_at"#,
            task_id
        )
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_conversation_session_id(
        pool: &SqlitePool,
        conversation_session_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Image,
            r#"SELECT i.id as "id!: Uuid",
                      i.file_path as "file_path!",
                      i.original_name as "original_name!",
                      i.mime_type,
                      i.size_bytes as "size_bytes!",
                      i.hash as "hash!",
                      i.created_at as "created_at!: DateTime<Utc>",
                      i.updated_at as "updated_at!: DateTime<Utc>"
               FROM images i
               JOIN conversation_images ci ON i.id = ci.image_id
               WHERE ci.conversation_session_id = $1
               ORDER BY ci.created_at"#,
            conversation_session_id
        )
        .fetch_all(pool)
        .await
    }

    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query!(r#"DELETE FROM images WHERE id = $1"#, id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn find_orphaned_images(pool: &SqlitePool) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Image,
            r#"SELECT i.id as "id!: Uuid",
                      i.file_path as "file_path!",
                      i.original_name as "original_name!",
                      i.mime_type,
                      i.size_bytes as "size_bytes!",
                      i.hash as "hash!",
                      i.created_at as "created_at!: DateTime<Utc>",
                      i.updated_at as "updated_at!: DateTime<Utc>"
               FROM images i
               LEFT JOIN task_images ti ON i.id = ti.image_id
               LEFT JOIN conversation_images ci ON i.id = ci.image_id
               WHERE ti.task_id IS NULL
                 AND ci.conversation_session_id IS NULL"#
        )
        .fetch_all(pool)
        .await
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct ConversationImage {
    pub id: Uuid,
    pub conversation_session_id: Uuid,
    pub image_id: Uuid,
    pub created_at: DateTime<Utc>,
}

impl TaskImage {
    /// Associate multiple images with a task, skipping duplicates.
    pub async fn associate_many_dedup(
        pool: &SqlitePool,
        task_id: Uuid,
        image_ids: &[Uuid],
    ) -> Result<(), sqlx::Error> {
        if image_ids.is_empty() {
            return Ok(());
        }

        let mut qb: QueryBuilder<sqlx::Sqlite> =
            QueryBuilder::new("INSERT OR IGNORE INTO task_images (id, task_id, image_id) ");

        qb.push_values(image_ids, |mut b, image_id| {
            b.push_bind(Uuid::new_v4())
                .push_bind(task_id)
                .push_bind(*image_id);
        });

        qb.build().execute(pool).await?;
        Ok(())
    }

    pub async fn delete_by_task_id(pool: &SqlitePool, task_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query!(r#"DELETE FROM task_images WHERE task_id = $1"#, task_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Check if an image is associated with a specific task.
    pub async fn is_associated(
        pool: &SqlitePool,
        task_id: Uuid,
        image_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r#"SELECT EXISTS(
                SELECT 1
                FROM task_images
                WHERE task_id = $1 AND image_id = $2
               ) AS "exists!: bool"
            "#,
            task_id,
            image_id
        )
        .fetch_one(pool)
        .await?;
        Ok(result)
    }
}

impl ConversationImage {
    /// Associate multiple images with a conversation session, skipping duplicates.
    pub async fn associate_many_dedup(
        pool: &SqlitePool,
        conversation_session_id: Uuid,
        image_ids: &[Uuid],
    ) -> Result<(), sqlx::Error> {
        if image_ids.is_empty() {
            return Ok(());
        }

        let mut qb: QueryBuilder<sqlx::Sqlite> = QueryBuilder::new(
            "INSERT OR IGNORE INTO conversation_images (id, conversation_session_id, image_id) ",
        );

        qb.push_values(image_ids, |mut b, image_id| {
            b.push_bind(Uuid::new_v4())
                .push_bind(conversation_session_id)
                .push_bind(*image_id);
        });

        qb.build().execute(pool).await?;
        Ok(())
    }

    pub async fn delete_by_conversation_session_id(
        pool: &SqlitePool,
        conversation_session_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"DELETE FROM conversation_images WHERE conversation_session_id = $1"#,
            conversation_session_id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Check if an image is associated with a specific conversation session.
    pub async fn is_associated(
        pool: &SqlitePool,
        conversation_session_id: Uuid,
        image_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r#"SELECT EXISTS(
                SELECT 1
                FROM conversation_images
                WHERE conversation_session_id = $1 AND image_id = $2
               ) AS "exists!: bool"
            "#,
            conversation_session_id,
            image_id
        )
        .fetch_one(pool)
        .await?;
        Ok(result)
    }
}
