use axum::{Router, response::Json as ResponseJson, routing::get};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utils::response::ApiResponse;

use crate::DeploymentImpl;

pub fn router() -> Router<DeploymentImpl> {
    Router::new().route("/account-info", get(get_account_info))
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub claude: Option<ClaudeAccountInfo>,
    pub codex: Option<CodexAccountInfo>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeAccountInfo {
    pub subscription_type: String,
    pub rate_limit_tier: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CodexAccountInfo {
    pub plan_type: String,
    pub subscription_active_until: Option<String>,
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
}

/// Codex auth file structure at ~/.codex/auth.json
#[derive(Debug, Deserialize)]
struct CodexAuthFile {
    tokens: Option<CodexTokens>,
}

#[derive(Debug, Deserialize)]
struct CodexTokens {
    id_token: Option<String>,
}

/// JWT claims from Codex id_token
#[derive(Debug, Deserialize)]
struct CodexJwtClaims {
    #[serde(rename = "https://api.openai.com/auth")]
    openai_auth: Option<OpenAiAuthClaims>,
}

#[derive(Debug, Deserialize)]
struct OpenAiAuthClaims {
    chatgpt_plan_type: Option<String>,
    chatgpt_subscription_active_until: Option<String>,
}

fn read_claude_account_info() -> Option<ClaudeAccountInfo> {
    let credentials_path = dirs::home_dir()?.join(".claude").join(".credentials.json");

    let contents = std::fs::read_to_string(&credentials_path).ok()?;
    let credentials: ClaudeCredentialsFile = serde_json::from_str(&contents).ok()?;
    let oauth = credentials.claude_ai_oauth?;

    Some(ClaudeAccountInfo {
        subscription_type: oauth.subscription_type?,
        rate_limit_tier: oauth.rate_limit_tier,
    })
}

fn read_codex_account_info() -> Option<CodexAccountInfo> {
    let auth_path = dirs::home_dir()?.join(".codex").join("auth.json");

    let contents = std::fs::read_to_string(&auth_path).ok()?;
    let auth_file: CodexAuthFile = serde_json::from_str(&contents).ok()?;
    let id_token = auth_file.tokens?.id_token?;

    let claims: CodexJwtClaims = utils::jwt::extract_custom_claims(&id_token).ok()?;
    let openai_auth = claims.openai_auth?;

    Some(CodexAccountInfo {
        plan_type: openai_auth.chatgpt_plan_type?,
        subscription_active_until: openai_auth.chatgpt_subscription_active_until,
    })
}

async fn get_account_info() -> ResponseJson<ApiResponse<AccountInfo>> {
    let claude = read_claude_account_info();
    let codex = read_codex_account_info();

    ResponseJson(ApiResponse::success(AccountInfo { claude, codex }))
}
