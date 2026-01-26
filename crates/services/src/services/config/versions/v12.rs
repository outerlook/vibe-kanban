use anyhow::Error;
use executors::{executors::BaseCodingAgent, profile::ExecutorProfileId};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
pub use v11::{
    BackupConfig, EditorConfig, EditorType, GitHubConfig, NotificationConfig, ShowcaseState,
    SoundFile, ThemeMode, UiLanguage,
};

use crate::services::config::versions::v11;

fn default_git_branch_prefix() -> String {
    "vk".to_string()
}

fn default_pr_auto_description_enabled() -> bool {
    true
}

fn default_commit_message_auto_generate_enabled() -> bool {
    true
}

fn default_langfuse_host() -> Option<String> {
    Some("https://cloud.langfuse.com".to_string())
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct Config {
    pub config_version: String,
    pub theme: ThemeMode,
    pub executor_profile: ExecutorProfileId,
    pub disclaimer_acknowledged: bool,
    pub onboarding_acknowledged: bool,
    pub notifications: NotificationConfig,
    pub editor: EditorConfig,
    pub github: GitHubConfig,
    pub analytics_enabled: bool,
    pub workspace_dir: Option<String>,
    pub last_app_version: Option<String>,
    pub show_release_notes: bool,
    #[serde(default)]
    pub language: UiLanguage,
    #[serde(default = "default_git_branch_prefix")]
    pub git_branch_prefix: String,
    #[serde(default)]
    pub showcases: ShowcaseState,
    #[serde(default = "default_pr_auto_description_enabled")]
    pub pr_auto_description_enabled: bool,
    #[serde(default)]
    pub pr_auto_description_prompt: Option<String>,
    #[serde(default)]
    pub default_clone_directory: Option<String>,
    #[serde(default = "default_commit_message_auto_generate_enabled")]
    pub commit_message_auto_generate_enabled: bool,
    #[serde(default)]
    pub commit_message_prompt: Option<String>,
    #[serde(default)]
    pub commit_message_executor_profile: Option<ExecutorProfileId>,
    /// Maximum concurrent agent executions (0 = unlimited)
    #[serde(default)]
    pub max_concurrent_agents: u32,
    // Langfuse configuration
    #[serde(default)]
    pub langfuse_enabled: bool,
    #[serde(default)]
    pub langfuse_public_key: Option<String>,
    #[serde(default)]
    pub langfuse_secret_key: Option<String>,
    #[serde(default = "default_langfuse_host")]
    pub langfuse_host: Option<String>,
    #[serde(default)]
    pub backup: BackupConfig,
    /// Executor profile for the review attention agent.
    /// When Some, review attention uses the specified executor.
    /// When None, review attention is disabled.
    #[serde(default)]
    pub review_attention_executor_profile: Option<ExecutorProfileId>,
}

impl Config {
    fn from_v11_config(old_config: v11::Config) -> Self {
        Self {
            config_version: "v12".to_string(),
            theme: old_config.theme,
            executor_profile: old_config.executor_profile,
            disclaimer_acknowledged: old_config.disclaimer_acknowledged,
            onboarding_acknowledged: old_config.onboarding_acknowledged,
            notifications: old_config.notifications,
            editor: old_config.editor,
            github: old_config.github,
            analytics_enabled: old_config.analytics_enabled,
            workspace_dir: old_config.workspace_dir,
            last_app_version: old_config.last_app_version,
            show_release_notes: old_config.show_release_notes,
            language: old_config.language,
            git_branch_prefix: old_config.git_branch_prefix,
            showcases: old_config.showcases,
            pr_auto_description_enabled: old_config.pr_auto_description_enabled,
            pr_auto_description_prompt: old_config.pr_auto_description_prompt,
            default_clone_directory: old_config.default_clone_directory,
            commit_message_auto_generate_enabled: old_config.commit_message_auto_generate_enabled,
            commit_message_prompt: old_config.commit_message_prompt,
            commit_message_executor_profile: old_config.commit_message_executor_profile,
            max_concurrent_agents: old_config.max_concurrent_agents,
            langfuse_enabled: old_config.langfuse_enabled,
            langfuse_public_key: old_config.langfuse_public_key,
            langfuse_secret_key: old_config.langfuse_secret_key,
            langfuse_host: old_config.langfuse_host,
            backup: old_config.backup,
            // New field with default None (feature disabled)
            review_attention_executor_profile: None,
        }
    }

    pub fn from_previous_version(raw_config: &str) -> Result<Self, Error> {
        let old_config = v11::Config::from(raw_config.to_string());
        Ok(Self::from_v11_config(old_config))
    }
}

impl From<String> for Config {
    fn from(raw_config: String) -> Self {
        if let Ok(config) = serde_json::from_str::<Config>(&raw_config)
            && config.config_version == "v12"
        {
            return config;
        }

        match Self::from_previous_version(&raw_config) {
            Ok(config) => {
                tracing::info!("Config upgraded to v12");
                config
            }
            Err(e) => {
                tracing::warn!("Config migration failed: {}, using default", e);
                Self::default()
            }
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            config_version: "v12".to_string(),
            theme: ThemeMode::System,
            executor_profile: ExecutorProfileId::new(BaseCodingAgent::ClaudeCode),
            disclaimer_acknowledged: false,
            onboarding_acknowledged: false,
            notifications: NotificationConfig::default(),
            editor: EditorConfig::default(),
            github: GitHubConfig::default(),
            analytics_enabled: true,
            workspace_dir: None,
            last_app_version: None,
            show_release_notes: false,
            language: UiLanguage::default(),
            git_branch_prefix: default_git_branch_prefix(),
            showcases: ShowcaseState::default(),
            pr_auto_description_enabled: true,
            pr_auto_description_prompt: None,
            default_clone_directory: None,
            commit_message_auto_generate_enabled: true,
            commit_message_prompt: None,
            commit_message_executor_profile: None,
            max_concurrent_agents: 0,
            langfuse_enabled: false,
            langfuse_public_key: None,
            langfuse_secret_key: None,
            langfuse_host: default_langfuse_host(),
            backup: BackupConfig::default(),
            review_attention_executor_profile: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_v11_to_v12_migration() {
        let v11_config = v11::Config::default();
        let v11_json = serde_json::to_string(&v11_config).unwrap();

        let v12_config = Config::from(v11_json);

        assert_eq!(v12_config.config_version, "v12");
        // Verify v11 fields are preserved
        assert_eq!(v12_config.analytics_enabled, v11_config.analytics_enabled);
        assert_eq!(
            v12_config.max_concurrent_agents,
            v11_config.max_concurrent_agents
        );
        assert_eq!(v12_config.git_branch_prefix, v11_config.git_branch_prefix);
        assert_eq!(v12_config.langfuse_enabled, v11_config.langfuse_enabled);
        assert_eq!(v12_config.backup.enabled, v11_config.backup.enabled);
        // Verify new field has default None
        assert!(v12_config.review_attention_executor_profile.is_none());
    }

    #[test]
    fn test_v12_roundtrip() {
        let config = Config {
            review_attention_executor_profile: Some(ExecutorProfileId::with_variant(
                BaseCodingAgent::ClaudeCode,
                "HAIKU".to_string(),
            )),
            ..Config::default()
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed = Config::from(json);

        assert_eq!(parsed.config_version, "v12");
        assert!(parsed.review_attention_executor_profile.is_some());
        let profile = parsed.review_attention_executor_profile.unwrap();
        assert_eq!(profile.executor, BaseCodingAgent::ClaudeCode);
        assert_eq!(profile.variant, Some("HAIKU".to_string()));
    }

    #[test]
    fn test_v12_with_none_review_attention() {
        let config = Config::default();
        assert!(config.review_attention_executor_profile.is_none());

        let json = serde_json::to_string(&config).unwrap();
        let parsed = Config::from(json);

        assert_eq!(parsed.config_version, "v12");
        assert!(parsed.review_attention_executor_profile.is_none());
    }

    #[test]
    fn test_v12_deserialize_without_review_attention_field() {
        // Serialize a default config, then remove the review_attention_executor_profile field
        // to simulate loading a config file that was saved before this field existed
        let config = Config::default();
        let mut json_value: serde_json::Value = serde_json::to_value(&config).unwrap();

        // Remove the new field to simulate an old config file
        json_value
            .as_object_mut()
            .unwrap()
            .remove("review_attention_executor_profile");

        // Deserialize - should succeed with None as default
        let parsed: Config = serde_json::from_value(json_value).unwrap();
        assert_eq!(parsed.config_version, "v12");
        assert!(parsed.review_attention_executor_profile.is_none());
    }
}
