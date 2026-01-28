use axum::{
    Json, Router,
    extract::Path,
    http::StatusCode,
    response::Json as ResponseJson,
    routing::{delete, get, post, put},
};
use utils::{
    claude_accounts::{
        ClaudeAccountError, SaveAccountRequest, SavedAccount, UpdateNameRequest, delete_account,
        get_current_hash, list_accounts, load_account, save_account,
        set_secure_file_permissions, update_account_name,
    },
    response::ApiResponse,
};

use crate::{DeploymentImpl, error::ApiError};

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/claude-accounts", get(list_accounts_handler))
        .route("/claude-accounts/save", post(save_current_account_handler))
        .route("/claude-accounts/current", get(get_current_account_handler))
        .route(
            "/claude-accounts/{hash}",
            delete(delete_account_handler),
        )
        .route(
            "/claude-accounts/{hash}/name",
            put(update_account_name_handler),
        )
        .route(
            "/claude-accounts/switch/{hash}",
            post(switch_account_handler),
        )
}

/// GET /api/claude-accounts - List all saved accounts
async fn list_accounts_handler() -> Result<ResponseJson<ApiResponse<Vec<SavedAccount>>>, ApiError> {
    let accounts = list_accounts()
        .await
        .map_err(map_claude_account_error)?;

    tracing::info!(count = accounts.len(), "Listed Claude accounts");
    Ok(ResponseJson(ApiResponse::success(accounts)))
}

/// POST /api/claude-accounts/save - Save the current account
async fn save_current_account_handler(
    Json(request): Json<SaveAccountRequest>,
) -> Result<(StatusCode, ResponseJson<ApiResponse<SavedAccount>>), ApiError> {
    let account = save_account(request.name)
        .await
        .map_err(map_claude_account_error)?;

    tracing::info!(
        hash_prefix = %account.hash_prefix,
        name = ?account.name,
        "Saved Claude account"
    );

    Ok((
        StatusCode::CREATED,
        ResponseJson(ApiResponse::success(account)),
    ))
}

/// POST /api/claude-accounts/switch/{hash} - Switch to a saved account
async fn switch_account_handler(
    Path(hash): Path<String>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let credentials = load_account(&hash)
        .await
        .map_err(map_claude_account_error)?;

    let credentials_path = dirs::home_dir()
        .ok_or_else(|| ApiError::Internal("Could not determine home directory".to_string()))?
        .join(".claude")
        .join(".credentials.json");

    // Ensure parent directory exists
    if let Some(parent) = credentials_path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| {
            tracing::warn!(error = %e, "Failed to create .claude directory");
            ApiError::Internal(format!("Failed to create .claude directory: {}", e))
        })?;
    }

    // Write credentials atomically
    let contents = serde_json::to_string_pretty(&credentials).map_err(|e| {
        ApiError::Internal(format!("Failed to serialize credentials: {}", e))
    })?;

    tokio::fs::write(&credentials_path, contents).await.map_err(|e| {
        tracing::warn!(error = %e, path = ?credentials_path, "Failed to write credentials file");
        ApiError::Internal(format!("Failed to write credentials: {}", e))
    })?;

    set_secure_file_permissions(&credentials_path)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "Failed to set credentials file permissions");
            ApiError::Internal(format!("Failed to set file permissions: {}", e))
        })?;

    tracing::info!(hash_prefix = %hash, "Switched Claude account");
    Ok(ResponseJson(ApiResponse::success(())))
}

/// PUT /api/claude-accounts/{hash}/name - Update account name
async fn update_account_name_handler(
    Path(hash): Path<String>,
    Json(request): Json<UpdateNameRequest>,
) -> Result<ResponseJson<ApiResponse<SavedAccount>>, ApiError> {
    let account = update_account_name(&hash, request.name)
        .await
        .map_err(map_claude_account_error)?;

    tracing::info!(
        hash_prefix = %hash,
        name = ?account.name,
        "Updated Claude account name"
    );

    Ok(ResponseJson(ApiResponse::success(account)))
}

/// DELETE /api/claude-accounts/{hash} - Delete a saved account
async fn delete_account_handler(Path(hash): Path<String>) -> Result<StatusCode, ApiError> {
    delete_account(&hash)
        .await
        .map_err(map_claude_account_error)?;

    tracing::info!(hash_prefix = %hash, "Deleted Claude account");
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/claude-accounts/current - Get the hash of the current account
async fn get_current_account_handler(
) -> Result<ResponseJson<ApiResponse<Option<String>>>, ApiError> {
    let current_hash = get_current_hash()
        .await
        .map_err(map_claude_account_error)?;

    Ok(ResponseJson(ApiResponse::success(current_hash)))
}

fn map_claude_account_error(err: ClaudeAccountError) -> ApiError {
    match err {
        ClaudeAccountError::NotFound(hash) => {
            ApiError::NotFound(format!("Account not found: {}", hash))
        }
        ClaudeAccountError::NoCredentials => ApiError::NotFound(
            "No Claude credentials found. Please log in to Claude first.".to_string(),
        ),
        ClaudeAccountError::InvalidCredentials => ApiError::BadRequest(
            "Invalid credentials file: missing required fields".to_string(),
        ),
        ClaudeAccountError::Io(e) => {
            tracing::warn!(error = %e, "IO error in claude_accounts");
            ApiError::Internal(format!("File system error: {}", e))
        }
        ClaudeAccountError::Json(e) => {
            tracing::warn!(error = %e, "JSON error in claude_accounts");
            ApiError::Internal(format!("JSON parsing error: {}", e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_not_found_error() {
        let err = ClaudeAccountError::NotFound("abc123".to_string());
        let api_err = map_claude_account_error(err);
        assert!(matches!(api_err, ApiError::NotFound(_)));
    }

    #[test]
    fn test_map_no_credentials_error() {
        let err = ClaudeAccountError::NoCredentials;
        let api_err = map_claude_account_error(err);
        assert!(matches!(api_err, ApiError::NotFound(_)));
    }

    #[test]
    fn test_map_invalid_credentials_error() {
        let err = ClaudeAccountError::InvalidCredentials;
        let api_err = map_claude_account_error(err);
        assert!(matches!(api_err, ApiError::BadRequest(_)));
    }
}
