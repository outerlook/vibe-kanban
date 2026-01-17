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
            backup: BackupConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_from_v10() {
        let v10_config = r#"{
            "config_version": "v10",
            "theme": "SYSTEM",
            "executor_profile": { "executor": "CLAUDE_CODE" },
            "disclaimer_acknowledged": true,
            "onboarding_acknowledged": true,
            "notifications": {
                "sound_enabled": true,
                "push_enabled": false,
                "sound_file": "COW_MOOING",
                "error_sound_file": "ERROR_BUZZER",
                "custom_sound_path": null
            },
            "editor": {
                "editor_type": "VS_CODE"
            },
            "github": {
                "oauth_token": null
            },
            "analytics_enabled": true,
            "workspace_dir": null,
            "last_app_version": "1.0.0",
            "show_release_notes": false,
            "language": "EN",
            "git_branch_prefix": "feature",
            "showcases": {},
            "pr_auto_description_enabled": true,
            "pr_auto_description_prompt": null,
            "default_clone_directory": null,
            "commit_message_auto_generate_enabled": true,
            "commit_message_prompt": null,
            "commit_message_executor_profile": null,
            "max_concurrent_agents": 2
        }"#;

        let config = Config::from(v10_config.to_string());

        assert_eq!(config.config_version, "v11");
        assert!(config.disclaimer_acknowledged);
        assert_eq!(config.git_branch_prefix, "feature");
        assert_eq!(config.max_concurrent_agents, 2);
        // Backup should have defaults
        assert!(config.backup.enabled);
        assert_eq!(config.backup.interval_hours, 6);
        assert_eq!(config.backup.retention_hours_all, 24);
        assert_eq!(config.backup.retention_daily_days, 7);
        assert_eq!(config.backup.retention_weekly_weeks, 4);
        assert_eq!(config.backup.retention_monthly_months, 12);
    }

    #[test]
    fn test_backup_config_serialization() {
        let backup = BackupConfig::default();
        let json = serde_json::to_string(&backup).unwrap();
        let deserialized: BackupConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(backup.enabled, deserialized.enabled);
        assert_eq!(backup.interval_hours, deserialized.interval_hours);
        assert_eq!(backup.retention_hours_all, deserialized.retention_hours_all);
        assert_eq!(backup.retention_daily_days, deserialized.retention_daily_days);
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
