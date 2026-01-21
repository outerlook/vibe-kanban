use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, LazyLock, RwLock},
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::services::config::ConfigError;

static CUSTOM_EDITORS_CACHE: LazyLock<RwLock<Arc<CustomEditorsConfig>>> =
    LazyLock::new(|| RwLock::new(Arc::new(CustomEditorsConfig::load_sync())));

fn default_argument() -> String {
    "%d".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct CustomEditor {
    pub id: Uuid,
    pub name: String,
    pub command: String,
    #[serde(default = "default_argument")]
    pub argument: String,
    pub icon: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct CustomEditorsConfig {
    #[serde(default)]
    pub custom_editors: HashMap<Uuid, CustomEditor>,
}

impl CustomEditorsConfig {
    /// Get cached custom editors config.
    pub fn get_cached() -> Arc<Self> {
        Arc::clone(&CUSTOM_EDITORS_CACHE.read().unwrap())
    }

    /// Reload custom editors config from disk.
    pub async fn reload() -> Result<(), ConfigError> {
        let config = Self::load_from_path(&utils::assets::editors_path()).await?;
        let mut cache = CUSTOM_EDITORS_CACHE.write().unwrap();
        *cache = Arc::new(config);
        Ok(())
    }

    pub fn get(&self, id: Uuid) -> Option<&CustomEditor> {
        self.custom_editors.get(&id)
    }

    #[cfg(test)]
    pub(super) fn set_cached_for_tests(config: CustomEditorsConfig) {
        let mut cache = CUSTOM_EDITORS_CACHE.write().unwrap();
        *cache = Arc::new(config);
    }

    pub async fn create(
        name: String,
        command: String,
        argument: Option<String>,
    ) -> Result<Uuid, ConfigError> {
        let mut config = Self::get_cached().as_ref().clone();
        let id = config.create_in_memory(name, command, argument)?;
        config.save_to_path(&utils::assets::editors_path()).await?;
        let mut cache = CUSTOM_EDITORS_CACHE.write().unwrap();
        *cache = Arc::new(config);
        Ok(id)
    }

    pub async fn update(
        id: Uuid,
        name: String,
        command: String,
        argument: Option<String>,
    ) -> Result<(), ConfigError> {
        let mut config = Self::get_cached().as_ref().clone();
        config.update_in_memory(id, name, command, argument)?;
        config.save_to_path(&utils::assets::editors_path()).await?;
        let mut cache = CUSTOM_EDITORS_CACHE.write().unwrap();
        *cache = Arc::new(config);
        Ok(())
    }

    pub async fn delete(id: Uuid) -> Result<(), ConfigError> {
        let mut config = Self::get_cached().as_ref().clone();
        config.delete_in_memory(id)?;
        config.save_to_path(&utils::assets::editors_path()).await?;
        let mut cache = CUSTOM_EDITORS_CACHE.write().unwrap();
        *cache = Arc::new(config);
        Ok(())
    }

    fn create_in_memory(
        &mut self,
        name: String,
        command: String,
        argument: Option<String>,
    ) -> Result<Uuid, ConfigError> {
        self.validate_unique_name(&name, None)?;
        self.validate_command(&command)?;
        let id = Uuid::new_v4();
        let editor = CustomEditor {
            id,
            name,
            command,
            argument: argument.unwrap_or_else(default_argument),
            icon: None,
            created_at: Utc::now().to_rfc3339(),
        };
        self.custom_editors.insert(id, editor);
        Ok(id)
    }

    fn update_in_memory(
        &mut self,
        id: Uuid,
        name: String,
        command: String,
        argument: Option<String>,
    ) -> Result<(), ConfigError> {
        self.validate_unique_name(&name, Some(id))?;
        self.validate_command(&command)?;
        let editor = self.custom_editors.get_mut(&id).ok_or_else(|| {
            ConfigError::ValidationError(format!("Custom editor '{id}' not found"))
        })?;
        editor.name = name;
        editor.command = command;
        if let Some(arg) = argument {
            editor.argument = arg;
        }
        Ok(())
    }

    fn delete_in_memory(&mut self, id: Uuid) -> Result<(), ConfigError> {
        let removed = self.custom_editors.remove(&id);
        if removed.is_none() {
            return Err(ConfigError::ValidationError(format!(
                "Custom editor '{id}' not found"
            )));
        }
        Ok(())
    }

    fn validate_unique_name(
        &self,
        name: &str,
        exclude_id: Option<Uuid>,
    ) -> Result<(), ConfigError> {
        let duplicate = self
            .custom_editors
            .values()
            .any(|editor| editor.name == name && Some(editor.id) != exclude_id);
        if duplicate {
            return Err(ConfigError::ValidationError(format!(
                "Custom editor name '{name}' already exists"
            )));
        }
        Ok(())
    }

    fn validate_command(&self, command: &str) -> Result<(), ConfigError> {
        if command.trim().is_empty() {
            return Err(ConfigError::ValidationError(
                "Custom editor command cannot be empty".to_string(),
            ));
        }
        Ok(())
    }

    async fn load_from_path(path: &Path) -> Result<Self, ConfigError> {
        match tokio::fs::read_to_string(path).await {
            Ok(content) => {
                if content.trim().is_empty() {
                    return Ok(Self::default());
                }
                Ok(serde_json::from_str(&content)?)
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(err.into()),
        }
    }

    async fn save_to_path(&self, path: &Path) -> Result<(), ConfigError> {
        let raw_config = serde_json::to_string_pretty(self)?;
        tokio::fs::write(path, raw_config).await?;
        Ok(())
    }

    fn load_sync() -> Self {
        let path = utils::assets::editors_path();
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                if content.trim().is_empty() {
                    return Self::default();
                }
                serde_json::from_str(&content).unwrap_or_else(|err| {
                    tracing::error!("Failed to parse custom editors: {}", err);
                    Self::default()
                })
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(err) => {
                tracing::error!("Failed to load custom editors: {}", err);
                Self::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn test_crud_operations() {
        let mut config = CustomEditorsConfig::default();

        // Test create with default argument
        let id = config
            .create_in_memory("Editor".to_string(), "code".to_string(), None)
            .unwrap();
        let editor = config.get(id).unwrap();
        assert_eq!(editor.name, "Editor");
        assert_eq!(editor.command, "code");
        assert_eq!(editor.argument, "%d");

        // Test create with custom argument
        let id2 = config
            .create_in_memory(
                "Editor2".to_string(),
                "vim".to_string(),
                Some("%f:%l".to_string()),
            )
            .unwrap();
        let editor2 = config.get(id2).unwrap();
        assert_eq!(editor2.argument, "%f:%l");

        // Test update without changing argument
        config
            .update_in_memory(id, "Updated".to_string(), "vim".to_string(), None)
            .unwrap();
        let editor = config.get(id).unwrap();
        assert_eq!(editor.name, "Updated");
        assert_eq!(editor.command, "vim");
        assert_eq!(editor.argument, "%d");

        // Test update with new argument
        config
            .update_in_memory(
                id,
                "Updated".to_string(),
                "vim".to_string(),
                Some("%f".to_string()),
            )
            .unwrap();
        let editor = config.get(id).unwrap();
        assert_eq!(editor.argument, "%f");

        config.delete_in_memory(id).unwrap();
        assert!(config.get(id).is_none());
    }

    #[tokio::test]
    async fn test_name_uniqueness_validation() {
        let mut config = CustomEditorsConfig::default();
        config
            .create_in_memory("Editor".to_string(), "code".to_string(), None)
            .unwrap();
        let result = config.create_in_memory("Editor".to_string(), "vim".to_string(), None);
        assert!(matches!(result, Err(ConfigError::ValidationError(_))));
    }

    #[tokio::test]
    async fn test_load_save_round_trip() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("editors.json");

        let mut config = CustomEditorsConfig::default();
        config
            .create_in_memory("Editor".to_string(), "code".to_string(), None)
            .unwrap();
        config.save_to_path(&path).await.unwrap();

        let loaded = CustomEditorsConfig::load_from_path(&path).await.unwrap();
        assert_eq!(loaded, config);
    }

    #[tokio::test]
    async fn test_load_missing_file_is_empty() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("editors.json");

        let loaded = CustomEditorsConfig::load_from_path(&path).await.unwrap();
        assert!(loaded.custom_editors.is_empty());
    }
}
