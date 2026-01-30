use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::Json as ResponseJson,
    routing::{delete, get, post, put},
};
use db::models::app_settings::{AppSettings, GitHubSettingsStatus};
use deployment::Deployment;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utils::response::ApiResponse;

use crate::{DeploymentImpl, error::ApiError};

mod encryption;

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/settings/github", get(get_github_settings))
        .route("/settings/github", put(set_github_token))
        .route("/settings/github", delete(delete_github_token))
        .route("/settings/github/import", post(import_github_token))
}

/// Request body for setting GitHub token
#[derive(Debug, Deserialize, TS)]
pub struct SetGitHubTokenRequest {
    pub token: String,
}

/// Response for successful GitHub token import
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct GitHubImportResponse {
    pub success: bool,
    pub message: String,
}

/// GET /api/settings/github - Check if GitHub token is configured
async fn get_github_settings(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<GitHubSettingsStatus>>, ApiError> {
    let pool = &deployment.db().pool;
    let configured = AppSettings::is_github_configured(pool).await?;

    Ok(ResponseJson(ApiResponse::success(GitHubSettingsStatus {
        configured,
    })))
}

/// PUT /api/settings/github - Set GitHub token
async fn set_github_token(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<SetGitHubTokenRequest>,
) -> Result<ResponseJson<ApiResponse<GitHubSettingsStatus>>, ApiError> {
    if payload.token.trim().is_empty() {
        return Err(ApiError::BadRequest("Token cannot be empty".to_string()));
    }

    let pool = &deployment.db().pool;

    // Encrypt the token before storing
    let encrypted = encryption::encrypt_token(&payload.token)
        .map_err(|e| ApiError::Internal(format!("Encryption failed: {}", e)))?;

    AppSettings::set_github_token(pool, Some(&encrypted)).await?;

    Ok(ResponseJson(ApiResponse::success(GitHubSettingsStatus {
        configured: true,
    })))
}

/// DELETE /api/settings/github - Clear GitHub token
async fn delete_github_token(
    State(deployment): State<DeploymentImpl>,
) -> Result<(StatusCode, ResponseJson<ApiResponse<GitHubSettingsStatus>>), ApiError> {
    let pool = &deployment.db().pool;

    AppSettings::set_github_token(pool, None).await?;

    Ok((
        StatusCode::OK,
        ResponseJson(ApiResponse::success(GitHubSettingsStatus {
            configured: false,
        })),
    ))
}

/// POST /api/settings/github/import - Import GitHub token from gh CLI
async fn import_github_token(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<GitHubImportResponse>>, ApiError> {
    // Run `gh auth token` to get the token
    let output = tokio::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .await
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ApiError::BadRequest(
                    "GitHub CLI (gh) is not installed. Please install it from https://cli.github.com/".to_string(),
                )
            } else {
                ApiError::Internal(format!("Failed to run gh command: {}", e))
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not logged") || stderr.contains("no oauth token") {
            return Err(ApiError::BadRequest(
                "Not logged into GitHub CLI. Run 'gh auth login' first.".to_string(),
            ));
        }
        return Err(ApiError::BadRequest(format!(
            "Failed to get token from gh CLI: {}",
            stderr.trim()
        )));
    }

    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if token.is_empty() {
        return Err(ApiError::BadRequest(
            "GitHub CLI returned empty token. Run 'gh auth login' first.".to_string(),
        ));
    }

    let pool = &deployment.db().pool;

    // Encrypt the token before storing
    let encrypted = encryption::encrypt_token(&token)
        .map_err(|e| ApiError::Internal(format!("Encryption failed: {}", e)))?;

    AppSettings::set_github_token(pool, Some(&encrypted)).await?;

    Ok(ResponseJson(ApiResponse::success(GitHubImportResponse {
        success: true,
        message: "GitHub token imported successfully from gh CLI".to_string(),
    })))
}

/// Retrieve the decrypted GitHub token (for internal use only, not exposed via API)
pub async fn get_github_token(pool: &sqlx::SqlitePool) -> Result<Option<String>, ApiError> {
    let settings = AppSettings::get(pool).await?;

    match settings.github_token_encrypted {
        Some(encrypted) => {
            let token = encryption::decrypt_token(&encrypted)
                .map_err(|e| ApiError::Internal(format!("Decryption failed: {}", e)))?;
            Ok(Some(token))
        }
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_roundtrip() {
        let original = "ghp_test_token_12345";
        let encrypted = encryption::encrypt_token(original).unwrap();
        let decrypted = encryption::decrypt_token(&encrypted).unwrap();
        assert_eq!(original, decrypted);
    }

    #[test]
    fn test_encryption_produces_different_output() {
        let token = "ghp_test_token_12345";
        let encrypted1 = encryption::encrypt_token(token).unwrap();
        let encrypted2 = encryption::encrypt_token(token).unwrap();
        // Due to random nonce, encrypted values should be different
        assert_ne!(encrypted1, encrypted2);
        // But both should decrypt to the same value
        assert_eq!(
            encryption::decrypt_token(&encrypted1).unwrap(),
            encryption::decrypt_token(&encrypted2).unwrap()
        );
    }
}
