use anyhow::Error;
use executors::{executors::BaseCodingAgent, profile::ExecutorProfileId};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
pub use v10::{
    EditorConfig, EditorType, GitHubConfig, NotificationConfig, ShowcaseState, SoundFile,
    ThemeMode, UiLanguage,
};

use crate::services::config::versions::v10;

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

fn default_backup_enabled() -> bool {
    true
}

fn default_backup_interval_hours() -> u32 {
    6
}

fn default_backup_retention_hours_all() -> u32 {
    24
}

fn default_backup_retention_daily_days() -> u32 {
    7
}

fn default_backup_retention_weekly_weeks() -> u32 {
    4
}

fn default_backup_retention_monthly_months() -> u32 {
    12
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BackupConfig {
    #[serde(default = "default_backup_enabled")]
    pub enabled: bool,
    #[serde(default = "default_backup_interval_hours")]
    pub interval_hours: u32,
    #[serde(default = "default_backup_retention_hours_all")]
    pub retention_hours_all: u32,
    #[serde(default = "default_backup_retention_daily_days")]
    pub retention_daily_days: u32,
    #[serde(default = "default_backup_retention_weekly_weeks")]
    pub retention_weekly_weeks: u32,
    #[serde(default = "default_backup_retention_monthly_months")]
    pub retention_monthly_months: u32,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            enabled: default_backup_enabled(),
            interval_hours: default_backup_interval_hours(),
            retention_hours_all: default_backup_retention_hours_all(),
            retention_daily_days: default_backup_retention_daily_days(),
            retention_weekly_weeks: default_backup_retention_weekly_weeks(),
            retention_monthly_months: default_backup_retention_monthly_months(),
        }
    }
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
}

impl Config {
    fn from_v10_config(old_config: v10::Config) -> Self {
        Self {
            config_version: "v11".to_string(),
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
            // New Langfuse fields with defaults
            langfuse_enabled: false,
            langfuse_public_key: None,
            langfuse_secret_key: None,
            langfuse_host: default_langfuse_host(),
            backup: BackupConfig::default(),
        }
    }

    pub fn from_previous_version(raw_config: &str) -> Result<Self, Error> {
        let old_config = v10::Config::from(raw_config.to_string());
        Ok(Self::from_v10_config(old_config))
    }
}

impl From<String> for Config {
    fn from(raw_config: String) -> Self {
        if let Ok(config) = serde_json::from_str::<Config>(&raw_config)
            && config.config_version == "v11"
        {
            return config;
        }

        match Self::from_previous_version(&raw_config) {
            Ok(config) => {
                tracing::info!("Config upgraded to v11");
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
            config_version: "v11".to_string(),
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_v10_to_v11_migration() {
        let v10_config = v10::Config::default();
        let v10_json = serde_json::to_string(&v10_config).unwrap();

        let v11_config = Config::from(v10_json);

        assert_eq!(v11_config.config_version, "v11");
        // Verify v10 fields are preserved
        assert_eq!(v11_config.analytics_enabled, v10_config.analytics_enabled);
        assert_eq!(
            v11_config.max_concurrent_agents,
            v10_config.max_concurrent_agents
        );
        assert_eq!(v11_config.git_branch_prefix, v10_config.git_branch_prefix);
        // Verify new Langfuse fields have defaults
        assert!(!v11_config.langfuse_enabled);
        assert!(v11_config.langfuse_public_key.is_none());
        assert!(v11_config.langfuse_secret_key.is_none());
        assert_eq!(
            v11_config.langfuse_host,
            Some("https://cloud.langfuse.com".to_string())
        );
        // Verify new Backup fields have defaults
        assert!(v11_config.backup.enabled);
        assert_eq!(v11_config.backup.interval_hours, 6);
    }

    #[test]
    fn test_v11_roundtrip() {
        let config = Config {
            langfuse_enabled: true,
            langfuse_public_key: Some("pk-test".to_string()),
            langfuse_secret_key: Some("sk-test".to_string()),
            langfuse_host: Some("https://custom.langfuse.com".to_string()),
            ..Config::default()
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed = Config::from(json);

        assert_eq!(parsed.config_version, "v11");
        assert!(parsed.langfuse_enabled);
        assert_eq!(parsed.langfuse_public_key, Some("pk-test".to_string()));
        assert_eq!(parsed.langfuse_secret_key, Some("sk-test".to_string()));
        assert_eq!(
            parsed.langfuse_host,
            Some("https://custom.langfuse.com".to_string())
        );
    }

    #[test]
    fn test_backup_config_serialization() {
        let backup = BackupConfig::default();
        let json = serde_json::to_string(&backup).unwrap();
        let deserialized: BackupConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(backup.enabled, deserialized.enabled);
        assert_eq!(backup.interval_hours, deserialized.interval_hours);
        assert_eq!(backup.retention_hours_all, deserialized.retention_hours_all);
        assert_eq!(
            backup.retention_daily_days,
            deserialized.retention_daily_days
        );
        assert_eq!(
            backup.retention_weekly_weeks,
            deserialized.retention_weekly_weeks
        );
        assert_eq!(
            backup.retention_monthly_months,
            deserialized.retention_monthly_months
        );
    }
}
