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
    pub usage: Option<ClaudeUsage>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeUsage {
    pub five_hour: UsageLimit,
    pub seven_day: UsageLimit,
    pub seven_day_opus: Option<UsageLimit>,
    pub seven_day_sonnet: Option<UsageLimit>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UsageLimit {
    /// Usage percentage from 0 to 100
    pub used_percent: f64,
    /// ISO 8601 timestamp when this limit resets
    pub resets_at: String,
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
    access_token: Option<String>,
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

/// Response from Anthropic usage API
#[derive(Debug, Deserialize)]
struct AnthropicUsageResponse {
    five_hour: AnthropicUsageLimit,
    seven_day: AnthropicUsageLimit,
    seven_day_opus: Option<AnthropicUsageLimit>,
    seven_day_sonnet: Option<AnthropicUsageLimit>,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsageLimit {
    /// Utilization percentage from 0.0 to 100.0
    utilization: f64,
    resets_at: String,
}

async fn fetch_claude_usage(access_token: &str) -> Option<ClaudeUsage> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    let response = client
        .get("https://api.anthropic.com/api/oauth/usage")
        .header("Authorization", format!("Bearer {}", access_token))
        .header("anthropic-beta", "oauth-2025-04-20")
        .send()
        .await;

    match response {
        Ok(resp) => {
            if !resp.status().is_success() {
                tracing::warn!("Claude usage API returned status: {}", resp.status());
                return None;
            }
            match resp.json::<AnthropicUsageResponse>().await {
                Ok(usage) => Some(ClaudeUsage {
                    five_hour: UsageLimit {
                        used_percent: usage.five_hour.utilization,
                        resets_at: usage.five_hour.resets_at,
                    },
                    seven_day: UsageLimit {
                        used_percent: usage.seven_day.utilization,
                        resets_at: usage.seven_day.resets_at,
                    },
                    seven_day_opus: usage.seven_day_opus.map(|limit| UsageLimit {
                        used_percent: limit.utilization,
                        resets_at: limit.resets_at,
                    }),
                    seven_day_sonnet: usage.seven_day_sonnet.map(|limit| UsageLimit {
                        used_percent: limit.utilization,
                        resets_at: limit.resets_at,
                    }),
                }),
                Err(e) => {
                    tracing::warn!("Failed to parse Claude usage response: {}", e);
                    None
                }
            }
        }
        Err(e) => {
            tracing::warn!("Failed to fetch Claude usage: {}", e);
            None
        }
    }
}

async fn read_claude_account_info() -> Option<ClaudeAccountInfo> {
    let credentials_path = dirs::home_dir()?.join(".claude").join(".credentials.json");

    let contents = std::fs::read_to_string(&credentials_path).ok()?;
    let credentials: ClaudeCredentialsFile = serde_json::from_str(&contents).ok()?;
    let oauth = credentials.claude_ai_oauth?;

    let usage = match &oauth.access_token {
        Some(token) => fetch_claude_usage(token).await,
        None => None,
    };

    Some(ClaudeAccountInfo {
        subscription_type: oauth.subscription_type?,
        rate_limit_tier: oauth.rate_limit_tier,
        usage,
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
    let claude = read_claude_account_info().await;
    let codex = read_codex_account_info();

    ResponseJson(ApiResponse::success(AccountInfo { claude, codex }))
}
