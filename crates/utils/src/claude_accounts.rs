use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use ts_rs::TS;

use crate::assets::asset_dir;

#[derive(Debug, Error)]
pub enum ClaudeAccountError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Account not found: {0}")]
    NotFound(String),

    #[error("No credentials found at ~/.claude/.credentials.json")]
    NoCredentials,

    #[error("Invalid credentials file: missing required fields")]
    InvalidCredentials,
}

/// Saved Claude account information
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SavedAccount {
    /// First 8 characters of SHA256 hash of the access token
    pub hash_prefix: String,
    /// User-defined name for this account
    pub name: Option<String>,
    /// Subscription type (e.g., "pro", "free")
    pub subscription_type: String,
    /// Rate limit tier if available
    pub rate_limit_tier: Option<String>,
    /// When this account was saved
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Request to save the current account
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SaveAccountRequest {
    /// Optional name for this account
    pub name: Option<String>,
}

/// Request to update account name
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateNameRequest {
    /// New name for this account
    pub name: String,
}

/// Claude credentials file structure at ~/.claude/.credentials.json
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeCredentialsFile {
    claude_ai_oauth: Option<ClaudeOAuthData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeOAuthData {
    subscription_type: Option<String>,
    rate_limit_tier: Option<String>,
    access_token: Option<String>,
}

/// Internal storage format for saved accounts (includes full credentials)
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAccount {
    /// Account metadata
    #[serde(flatten)]
    metadata: SavedAccount,
    /// Full credentials data (stored but not exposed in public API)
    credentials: serde_json::Value,
}

/// Returns the directory where Claude accounts are stored
pub fn accounts_dir() -> PathBuf {
    asset_dir().join("claude-accounts")
}

/// Returns the path to an account file given its hash prefix
fn account_file_path(hash_prefix: &str) -> PathBuf {
    accounts_dir().join(format!("{}.json", hash_prefix))
}

/// Read a stored account from disk
async fn read_stored_account(hash_prefix: &str) -> Result<StoredAccount, ClaudeAccountError> {
    let file_path = account_file_path(hash_prefix);

    if !file_path.exists() {
        return Err(ClaudeAccountError::NotFound(hash_prefix.to_string()));
    }

    let contents = tokio::fs::read_to_string(&file_path).await?;
    let stored: StoredAccount = serde_json::from_str(&contents)?;
    Ok(stored)
}

/// Hash a token using SHA256 and return the first 8 hex characters
pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..4])
}

/// Ensure the accounts directory exists with proper permissions
async fn ensure_accounts_dir() -> Result<(), ClaudeAccountError> {
    let dir = accounts_dir();
    if !dir.exists() {
        tokio::fs::create_dir_all(&dir).await?;

        // Set directory permissions to 0700 on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::Permissions::from_mode(0o700);
            tokio::fs::set_permissions(&dir, permissions).await?;
        }
    }
    Ok(())
}

/// Set file permissions to 0600 on Unix (no-op on other platforms)
pub async fn set_secure_file_permissions(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o600);
        tokio::fs::set_permissions(path, permissions).await?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

async fn set_file_permissions(path: &PathBuf) -> Result<(), ClaudeAccountError> {
    set_secure_file_permissions(path).await?;
    Ok(())
}

/// Get the path to Claude credentials file
fn claude_credentials_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".claude").join(".credentials.json"))
}

/// Read the current Claude credentials file
async fn read_credentials() -> Result<(ClaudeCredentialsFile, serde_json::Value), ClaudeAccountError>
{
    let path = claude_credentials_path().ok_or(ClaudeAccountError::NoCredentials)?;

    let contents = tokio::fs::read_to_string(&path)
        .await
        .map_err(|_| ClaudeAccountError::NoCredentials)?;

    let parsed: ClaudeCredentialsFile = serde_json::from_str(&contents)?;
    let raw: serde_json::Value = serde_json::from_str(&contents)?;

    Ok((parsed, raw))
}

/// List all saved accounts
pub async fn list_accounts() -> Result<Vec<SavedAccount>, ClaudeAccountError> {
    let dir = accounts_dir();

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut accounts = Vec::new();
    let mut read_dir = tokio::fs::read_dir(&dir).await?;

    while let Some(entry) = read_dir.next_entry().await? {
        let path = entry.path();

        // Only process .json files
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        match tokio::fs::read_to_string(&path).await {
            Ok(contents) => match serde_json::from_str::<StoredAccount>(&contents) {
                Ok(stored) => accounts.push(stored.metadata),
                Err(e) => {
                    tracing::warn!("Failed to parse account file {:?}: {}", path, e);
                }
            },
            Err(e) => {
                tracing::warn!("Failed to read account file {:?}: {}", path, e);
            }
        }
    }

    // Sort by created_at descending (newest first)
    accounts.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(accounts)
}

/// Save the current Claude account
pub async fn save_account(name: Option<String>) -> Result<SavedAccount, ClaudeAccountError> {
    let (parsed, raw) = read_credentials().await?;

    let oauth = parsed
        .claude_ai_oauth
        .ok_or(ClaudeAccountError::InvalidCredentials)?;

    let access_token = oauth
        .access_token
        .ok_or(ClaudeAccountError::InvalidCredentials)?;

    let subscription_type = oauth
        .subscription_type
        .ok_or(ClaudeAccountError::InvalidCredentials)?;

    let hash_prefix = hash_token(&access_token);

    let metadata = SavedAccount {
        hash_prefix: hash_prefix.clone(),
        name,
        subscription_type,
        rate_limit_tier: oauth.rate_limit_tier,
        created_at: chrono::Utc::now(),
    };

    let stored = StoredAccount {
        metadata: metadata.clone(),
        credentials: raw,
    };

    ensure_accounts_dir().await?;

    let file_path = account_file_path(&hash_prefix);
    let contents = serde_json::to_string_pretty(&stored)?;
    tokio::fs::write(&file_path, contents).await?;
    set_file_permissions(&file_path).await?;

    Ok(metadata)
}

/// Load full credentials for an account
pub async fn load_account(hash_prefix: &str) -> Result<serde_json::Value, ClaudeAccountError> {
    let stored = read_stored_account(hash_prefix).await?;
    Ok(stored.credentials)
}

/// Delete a saved account
pub async fn delete_account(hash_prefix: &str) -> Result<(), ClaudeAccountError> {
    let file_path = account_file_path(hash_prefix);

    if !file_path.exists() {
        return Err(ClaudeAccountError::NotFound(hash_prefix.to_string()));
    }

    tokio::fs::remove_file(&file_path).await?;
    Ok(())
}

/// Update the name of a saved account
pub async fn update_account_name(
    hash_prefix: &str,
    name: String,
) -> Result<SavedAccount, ClaudeAccountError> {
    let file_path = account_file_path(hash_prefix);
    let mut stored = read_stored_account(hash_prefix).await?;

    stored.metadata.name = Some(name);

    let updated_contents = serde_json::to_string_pretty(&stored)?;
    tokio::fs::write(&file_path, updated_contents).await?;

    Ok(stored.metadata)
}

/// Get the hash prefix of the currently active account
pub async fn get_current_hash() -> Result<Option<String>, ClaudeAccountError> {
    let (parsed, _) = match read_credentials().await {
        Ok(creds) => creds,
        Err(ClaudeAccountError::NoCredentials) => return Ok(None),
        Err(e) => return Err(e),
    };

    let hash = parsed
        .claude_ai_oauth
        .and_then(|oauth| oauth.access_token)
        .map(|token| hash_token(&token));

    Ok(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_token_produces_consistent_output() {
        let token = "test-access-token-12345";
        let hash1 = hash_token(token);
        let hash2 = hash_token(token);

        assert_eq!(hash1, hash2, "Same token should produce same hash");
        assert_eq!(hash1.len(), 8, "Hash should be 8 hex characters");

        // Verify it's valid hex
        assert!(
            hash1.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash should only contain hex characters"
        );
    }

    #[test]
    fn test_hash_token_different_tokens_different_hashes() {
        let hash1 = hash_token("token-a");
        let hash2 = hash_token("token-b");

        assert_ne!(
            hash1, hash2,
            "Different tokens should produce different hashes"
        );
    }

    #[test]
    fn test_accounts_dir_returns_correct_path() {
        let accounts = accounts_dir();
        let asset = asset_dir();

        assert_eq!(accounts, asset.join("claude-accounts"));
        assert!(accounts.ends_with("claude-accounts"));
    }

    #[test]
    fn test_saved_account_serialization() {
        let account = SavedAccount {
            hash_prefix: "abcd1234".to_string(),
            name: Some("Work Account".to_string()),
            subscription_type: "pro".to_string(),
            rate_limit_tier: Some("tier-1".to_string()),
            created_at: chrono::Utc::now(),
        };

        let json = serde_json::to_string(&account).unwrap();
        assert!(json.contains("\"hashPrefix\":\"abcd1234\""));
        assert!(json.contains("\"name\":\"Work Account\""));
        assert!(json.contains("\"subscriptionType\":\"pro\""));

        // Verify deserialization
        let deserialized: SavedAccount = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.hash_prefix, account.hash_prefix);
        assert_eq!(deserialized.name, account.name);
    }

    #[tokio::test]
    async fn test_list_accounts_missing_directory() {
        // This should return empty vec without error when directory doesn't exist
        // Note: In test environment, the actual accounts_dir may or may not exist
        let result = list_accounts().await;
        assert!(result.is_ok(), "list_accounts should not fail");
    }
}
