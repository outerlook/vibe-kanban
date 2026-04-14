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

pub type Config = versions::v17::Config;
pub type NotificationConfig = versions::v17::NotificationConfig;
pub type EditorConfig = versions::v17::EditorConfig;
pub type ThemeMode = versions::v17::ThemeMode;
pub type SoundFile = versions::v17::SoundFile;
pub use versions::v2::EffectiveSound;
pub type EditorType = versions::v17::EditorType;
pub type GitHubConfig = versions::v17::GitHubConfig;
pub type UiLanguage = versions::v17::UiLanguage;
pub type ShowcaseState = versions::v17::ShowcaseState;
pub type BackupConfig = versions::v17::BackupConfig;

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
