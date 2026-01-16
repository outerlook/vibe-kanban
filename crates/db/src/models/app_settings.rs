use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use thiserror::Error;
use ts_rs::TS;

#[derive(Debug, Error)]
pub enum AppSettingsError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Application-wide settings stored as a singleton row
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct AppSettings {
    pub id: i64,
    pub github_token_encrypted: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Response type for GitHub settings status (never exposes the actual token)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct GitHubSettingsStatus {
    pub configured: bool,
}

impl AppSettings {
    /// Get the singleton app settings row
    pub async fn get(pool: &SqlitePool) -> Result<Self, sqlx::Error> {
        sqlx::query_as!(
            AppSettings,
            r#"SELECT id,
                      github_token_encrypted,
                      created_at AS "created_at!: DateTime<Utc>",
                      updated_at AS "updated_at!: DateTime<Utc>"
               FROM app_settings
               WHERE id = 1"#
        )
        .fetch_one(pool)
        .await
    }

    /// Update the encrypted GitHub token
    pub async fn set_github_token(
        pool: &SqlitePool,
        encrypted_token: Option<&str>,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as!(
            AppSettings,
            r#"UPDATE app_settings
               SET github_token_encrypted = $1,
                   updated_at = datetime('now', 'subsec')
               WHERE id = 1
               RETURNING id,
                         github_token_encrypted,
                         created_at AS "created_at!: DateTime<Utc>",
                         updated_at AS "updated_at!: DateTime<Utc>""#,
            encrypted_token
        )
        .fetch_one(pool)
        .await
    }

    /// Check if GitHub token is configured
    pub async fn is_github_configured(pool: &SqlitePool) -> Result<bool, sqlx::Error> {
        let settings = Self::get(pool).await?;
        Ok(settings.github_token_encrypted.is_some())
    }
}
