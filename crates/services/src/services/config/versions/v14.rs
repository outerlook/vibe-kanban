use anyhow::Error;
use executors::{executors::BaseCodingAgent, profile::ExecutorProfileId};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
pub use v13::{
    BackupConfig, EditorConfig, EditorType, GitHubConfig, ShowcaseState, SoundFile, ThemeMode,
    UiLanguage,
};

use crate::services::config::versions::{v13, v2};

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

fn default_autopilot_enabled() -> bool {
    false
}

fn default_frontend_sounds_enabled() -> bool {
    false
}

fn default_error_sound_file() -> SoundFile {
    SoundFile::ErrorBuzzer
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct NotificationConfig {
    pub sound_enabled: bool,
    pub push_enabled: bool,
    pub sound_file: SoundFile,
    #[serde(default = "default_error_sound_file")]
    pub error_sound_file: SoundFile,
    #[serde(default)]
    pub custom_sound_path: Option<String>,
    /// When true, the frontend handles sound playback instead of the backend.
    /// This is useful for remote access where backend sound playback is not desired.
    #[serde(default = "default_frontend_sounds_enabled")]
    pub frontend_sounds_enabled: bool,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            sound_enabled: true,
            push_enabled: true,
            sound_file: SoundFile::CowMooing,
            error_sound_file: SoundFile::ErrorBuzzer,
            custom_sound_path: None,
            frontend_sounds_enabled: false,
        }
    }
}

impl NotificationConfig {
    /// Returns the effective sound to play.
    /// If `custom_sound_path` is set, returns `Custom`, otherwise returns `Bundled`.
    pub fn effective_sound(&self) -> v2::EffectiveSound {
        match &self.custom_sound_path {
            Some(path) => v2::EffectiveSound::Custom(path.clone()),
            None => v2::EffectiveSound::Bundled(self.sound_file.clone()),
        }
    }
}

impl From<v13::NotificationConfig> for NotificationConfig {
    fn from(old: v13::NotificationConfig) -> Self {
        Self {
            sound_enabled: old.sound_enabled,
            push_enabled: old.push_enabled,
            sound_file: old.sound_file,
            error_sound_file: old.error_sound_file,
            custom_sound_path: old.custom_sound_path,
            frontend_sounds_enabled: false,
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
    /// Executor profile for the review attention agent.
    /// When Some, review attention uses the specified executor.
    /// When None, review attention is disabled.
    #[serde(default)]
    pub review_attention_executor_profile: Option<ExecutorProfileId>,
    /// When enabled, completed tasks are automatically merged and dependent tasks are queued.
    #[serde(default = "default_autopilot_enabled")]
    pub autopilot_enabled: bool,
}

impl Config {
    fn from_v13_config(old_config: v13::Config) -> Self {
        Self {
            config_version: "v14".to_string(),
            theme: old_config.theme,
            executor_profile: old_config.executor_profile,
            disclaimer_acknowledged: old_config.disclaimer_acknowledged,
            onboarding_acknowledged: old_config.onboarding_acknowledged,
            notifications: NotificationConfig::from(old_config.notifications),
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
            review_attention_executor_profile: old_config.review_attention_executor_profile,
            autopilot_enabled: old_config.autopilot_enabled,
        }
    }

    pub fn from_previous_version(raw_config: &str) -> Result<Self, Error> {
        let old_config = v13::Config::from(raw_config.to_string());
        Ok(Self::from_v13_config(old_config))
    }
}

impl From<String> for Config {
    fn from(raw_config: String) -> Self {
        if let Ok(config) = serde_json::from_str::<Config>(&raw_config)
            && config.config_version == "v14"
        {
            return config;
        }

        match Self::from_previous_version(&raw_config) {
            Ok(config) => {
                tracing::info!("Config upgraded to v14");
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
            config_version: "v14".to_string(),
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
            autopilot_enabled: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_v13_to_v14_migration() {
        let v13_config = v13::Config::default();
        let v13_json = serde_json::to_string(&v13_config).unwrap();

        let v14_config = Config::from(v13_json);

        assert_eq!(v14_config.config_version, "v14");
        // Verify v13 fields are preserved
        assert_eq!(v14_config.analytics_enabled, v13_config.analytics_enabled);
        assert_eq!(
            v14_config.max_concurrent_agents,
            v13_config.max_concurrent_agents
        );
        assert_eq!(v14_config.git_branch_prefix, v13_config.git_branch_prefix);
        assert_eq!(v14_config.langfuse_enabled, v13_config.langfuse_enabled);
        assert_eq!(v14_config.backup.enabled, v13_config.backup.enabled);
        assert_eq!(
            v14_config.review_attention_executor_profile,
            v13_config.review_attention_executor_profile
        );
        assert_eq!(v14_config.autopilot_enabled, v13_config.autopilot_enabled);
        // Verify new field has default false
        assert!(!v14_config.notifications.frontend_sounds_enabled);
    }

    #[test]
    fn test_v14_roundtrip() {
        let config = Config {
            notifications: NotificationConfig {
                frontend_sounds_enabled: true,
                ..NotificationConfig::default()
            },
            ..Config::default()
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed = Config::from(json);

        assert_eq!(parsed.config_version, "v14");
        assert!(parsed.notifications.frontend_sounds_enabled);
    }

    #[test]
    fn test_v14_deserialize_without_frontend_sounds_field() {
        // Serialize a default config, then remove the frontend_sounds_enabled field
        // to simulate loading a config file that was saved before this field existed
        let config = Config::default();
        let mut json_value: serde_json::Value = serde_json::to_value(&config).unwrap();

        // Remove the new field from notifications to simulate an old config file
        json_value
            .get_mut("notifications")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("frontend_sounds_enabled");

        // Deserialize - should succeed with false as default
        let parsed: Config = serde_json::from_value(json_value).unwrap();
        assert_eq!(parsed.config_version, "v14");
        assert!(!parsed.notifications.frontend_sounds_enabled);
    }

    #[test]
    fn test_notification_config_effective_sound() {
        let config = NotificationConfig::default();
        assert_eq!(
            config.effective_sound(),
            v2::EffectiveSound::Bundled(SoundFile::CowMooing)
        );

        let config_with_custom = NotificationConfig {
            custom_sound_path: Some("custom.wav".to_string()),
            ..NotificationConfig::default()
        };
        assert_eq!(
            config_with_custom.effective_sound(),
            v2::EffectiveSound::Custom("custom.wav".to_string())
        );
    }
}
