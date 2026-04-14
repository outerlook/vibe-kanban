use std::path::PathBuf;

use thiserror::Error;

pub mod custom_editors;
pub mod editor;
mod versions;

pub use editor::EditorOpenError;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("Validation error: {0}")]
    ValidationError(String),
}

pub type Config = versions::v16::Config;
pub type NotificationConfig = versions::v16::NotificationConfig;
pub type EditorConfig = versions::v16::EditorConfig;
pub type ThemeMode = versions::v16::ThemeMode;
pub type SoundFile = versions::v16::SoundFile;
pub use versions::v2::EffectiveSound;
pub type EditorType = versions::v16::EditorType;
pub type GitHubConfig = versions::v16::GitHubConfig;
pub type UiLanguage = versions::v16::UiLanguage;
pub type ShowcaseState = versions::v16::ShowcaseState;
pub type BackupConfig = versions::v16::BackupConfig;

/// Will always return config, trying old schemas or eventually returning default
pub async fn load_config_from_file(config_path: &PathBuf) -> Config {
    match tokio::fs::read_to_string(config_path).await {
        Ok(raw_config) => Config::from(raw_config),
        Err(_) => {
            tracing::info!("No config file found, creating one");
            Config::default()
        }
    }
}

/// Saves the config to the given path
pub async fn save_config_to_file(
    config: &Config,
    config_path: &PathBuf,
) -> Result<(), ConfigError> {
    config.validate()?;
    let raw_config = serde_json::to_string_pretty(config)?;
    tokio::fs::write(config_path, raw_config).await?;
    Ok(())
}
