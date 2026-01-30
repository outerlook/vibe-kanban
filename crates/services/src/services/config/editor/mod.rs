use std::{path::Path, str::FromStr};

use executors::{command::CommandBuilder, executors::ExecutorError};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumIter, EnumString};
use thiserror::Error;
use ts_rs::TS;
use uuid::Uuid;

use crate::services::config::custom_editors::{CustomEditor, CustomEditorsConfig};

#[derive(Debug, Clone, Serialize, Deserialize, TS, Error)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum EditorOpenError {
    #[error("Editor executable '{executable}' not found in PATH")]
    ExecutableNotFound {
        executable: String,
        editor_type: EditorType,
    },
    #[error("Editor command for {editor_type:?} is invalid: {details}")]
    InvalidCommand {
        details: String,
        editor_type: EditorType,
    },
    #[error("Failed to launch '{executable}' for {editor_type:?}: {details}")]
    LaunchFailed {
        executable: String,
        details: String,
        editor_type: EditorType,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct EditorConfig {
    editor_type: EditorType,
    custom_command: Option<String>,
    #[serde(default)]
    custom_editor_id: Option<Uuid>,
    #[serde(default)]
    remote_ssh_host: Option<String>,
    #[serde(default)]
    remote_ssh_user: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, EnumString, EnumIter, PartialEq, Eq)]
#[ts(use_ts_enum)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum EditorType {
    VsCode,
    Cursor,
    Windsurf,
    IntelliJ,
    Zed,
    Xcode,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum EditorIdentifier {
    BuiltIn(EditorType),
    Custom(Uuid),
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            editor_type: EditorType::VsCode,
            custom_command: None,
            custom_editor_id: None,
            remote_ssh_host: None,
            remote_ssh_user: None,
        }
    }
}

impl EditorConfig {
    /// Create a new EditorConfig. This is primarily used by version migrations.
    pub fn new(
        editor_type: EditorType,
        custom_command: Option<String>,
        custom_editor_id: Option<Uuid>,
        remote_ssh_host: Option<String>,
        remote_ssh_user: Option<String>,
    ) -> Self {
        Self {
            editor_type,
            custom_command,
            custom_editor_id,
            remote_ssh_host,
            remote_ssh_user,
        }
    }

    pub fn resolve_identifier(&self) -> EditorIdentifier {
        match (self.editor_type.clone(), self.custom_editor_id) {
            (EditorType::Custom, Some(id)) => EditorIdentifier::Custom(id),
            (editor_type, _) => EditorIdentifier::BuiltIn(editor_type),
        }
    }

    pub fn get_command(&self) -> Result<CommandBuilder, EditorOpenError> {
        let base_command = match self.resolve_identifier() {
            EditorIdentifier::BuiltIn(editor_type) => match editor_type {
                EditorType::VsCode => "code".to_string(),
                EditorType::Cursor => "cursor".to_string(),
                EditorType::Windsurf => "windsurf".to_string(),
                EditorType::IntelliJ => "idea".to_string(),
                EditorType::Zed => "zed".to_string(),
                EditorType::Xcode => "xed".to_string(),
                EditorType::Custom => self
                    .custom_command
                    .clone()
                    .unwrap_or_else(|| "code".to_string()),
            },
            EditorIdentifier::Custom(editor_id) => {
                let custom_editors = CustomEditorsConfig::get_cached();
                let editor = custom_editors.get(editor_id).ok_or_else(|| {
                    EditorOpenError::ExecutableNotFound {
                        executable: editor_id.to_string(),
                        editor_type: EditorType::Custom,
                    }
                })?;
                editor.command.clone()
            }
        };
        Ok(CommandBuilder::new(base_command))
    }

    /// Resolve the editor command to an executable path and args.
    /// This is shared logic used by both check_availability() and spawn_local().
    async fn resolve_command(&self) -> Result<(std::path::PathBuf, Vec<String>), EditorOpenError> {
        let command_builder = self.get_command()?;
        let command_parts =
            command_builder
                .build_initial()
                .map_err(|e| EditorOpenError::InvalidCommand {
                    details: e.to_string(),
                    editor_type: self.editor_type.clone(),
                })?;

        let (executable, args) = command_parts.into_resolved().await.map_err(|e| match e {
            ExecutorError::ExecutableNotFound { program } => EditorOpenError::ExecutableNotFound {
                executable: program,
                editor_type: self.editor_type.clone(),
            },
            _ => EditorOpenError::InvalidCommand {
                details: e.to_string(),
                editor_type: self.editor_type.clone(),
            },
        })?;

        Ok((executable, args))
    }

    /// Check if the editor is available on the system.
    /// Uses the same command resolution logic as spawn_local().
    pub async fn check_availability(&self) -> bool {
        self.resolve_command().await.is_ok()
    }

    pub async fn open_file(&self, path: &Path) -> Result<Option<String>, EditorOpenError> {
        if let Some(url) = self.remote_url(path) {
            return Ok(Some(url));
        }
        self.spawn_local(path).await?;
        Ok(None)
    }

    fn remote_url(&self, path: &Path) -> Option<String> {
        let remote_host = self.remote_ssh_host.as_ref()?;
        let scheme = match self.editor_type {
            EditorType::VsCode => "vscode",
            EditorType::Cursor => "cursor",
            EditorType::Windsurf => "windsurf",
            _ => return None,
        };
        let user_part = self
            .remote_ssh_user
            .as_ref()
            .map(|u| format!("{u}@"))
            .unwrap_or_default();
        // files must contain a line and column number
        let line_col = if path.is_file() { ":1:1" } else { "" };
        let path = path.to_string_lossy();
        Some(format!(
            "{scheme}://vscode-remote/ssh-remote+{user_part}{remote_host}{path}{line_col}"
        ))
    }

    /// Substitute placeholders in argument template.
    /// - `%d` is replaced with the directory path
    /// - `%f` is replaced with the full file path
    /// If neither placeholder is present, returns None (path should be appended as last arg).
    fn substitute_placeholders(template: &str, path: &Path) -> Option<String> {
        let has_d = template.contains("%d");
        let has_f = template.contains("%f");

        if !has_d && !has_f {
            return None;
        }

        let dir_path = if path.is_dir() {
            path.to_string_lossy().to_string()
        } else {
            path.parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string())
        };

        let file_path = path.to_string_lossy().to_string();

        Some(template.replace("%d", &dir_path).replace("%f", &file_path))
    }

    /// Get the custom editor if this config points to one.
    fn get_custom_editor(&self) -> Option<CustomEditor> {
        match self.resolve_identifier() {
            EditorIdentifier::Custom(editor_id) => {
                let custom_editors = CustomEditorsConfig::get_cached();
                custom_editors.get(editor_id).cloned()
            }
            EditorIdentifier::BuiltIn(_) => None,
        }
    }

    pub async fn spawn_local(&self, path: &Path) -> Result<(), EditorOpenError> {
        let (executable, mut args) = self.resolve_command().await?;

        // For custom editors, substitute placeholders in the argument template
        if let Some(custom_editor) = self.get_custom_editor() {
            if let Some(substituted) = Self::substitute_placeholders(&custom_editor.argument, path)
            {
                // Parse the substituted argument as shell words
                args.extend(shell_words::split(&substituted).unwrap_or_else(|_| vec![substituted]));
            } else {
                // No placeholders found - fallback to appending path
                args.push(path.to_string_lossy().to_string());
            }
        } else {
            // Built-in editors: append path as last argument
            args.push(path.to_string_lossy().to_string());
        }

        let mut cmd = std::process::Command::new(&executable);
        cmd.args(&args);
        cmd.spawn().map_err(|e| EditorOpenError::LaunchFailed {
            executable: executable.to_string_lossy().into_owned(),
            details: e.to_string(),
            editor_type: self.editor_type.clone(),
        })?;
        Ok(())
    }

    pub fn with_override(&self, editor_type_str: Option<&str>) -> Result<Self, EditorOpenError> {
        if let Some(editor_type_str) = editor_type_str {
            let (editor_type, custom_editor_id) =
                if let Some(custom_id_str) = editor_type_str.strip_prefix("custom:") {
                    let custom_id = Uuid::parse_str(custom_id_str).map_err(|e| {
                        EditorOpenError::InvalidCommand {
                            details: format!("Invalid custom editor id '{custom_id_str}': {e}"),
                            editor_type: EditorType::Custom,
                        }
                    })?;
                    (EditorType::Custom, Some(custom_id))
                } else {
                    (
                        EditorType::from_str(editor_type_str).unwrap_or(self.editor_type.clone()),
                        None,
                    )
                };
            Ok(EditorConfig {
                editor_type,
                custom_command: self.custom_command.clone(),
                custom_editor_id,
                remote_ssh_host: self.remote_ssh_host.clone(),
                remote_ssh_user: self.remote_ssh_user.clone(),
            })
        } else {
            Ok(self.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{LazyLock, Mutex};

    use uuid::Uuid;

    use super::*;
    use crate::services::config::custom_editors::{CustomEditor, CustomEditorsConfig};

    static EDITOR_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    #[test]
    fn test_with_override_built_in() {
        let config = EditorConfig::new(EditorType::Cursor, None, None, None, None);
        let overridden = config.with_override(Some("VS_CODE")).unwrap();

        assert_eq!(overridden.editor_type, EditorType::VsCode);
        assert!(overridden.custom_editor_id.is_none());
    }

    #[test]
    fn test_with_override_custom_identifier() {
        let config = EditorConfig::default();
        let custom_id = Uuid::new_v4();
        let override_str = format!("custom:{custom_id}");

        let overridden = config.with_override(Some(&override_str)).unwrap();

        assert_eq!(overridden.editor_type, EditorType::Custom);
        assert_eq!(overridden.custom_editor_id, Some(custom_id));
    }

    #[test]
    fn test_with_override_invalid_custom_identifier() {
        let config = EditorConfig::default();

        let err = config.with_override(Some("custom:not-a-uuid")).unwrap_err();

        assert!(matches!(
            err,
            EditorOpenError::InvalidCommand {
                editor_type: EditorType::Custom,
                ..
            }
        ));
    }

    #[test]
    fn test_get_command_custom_editor() {
        let _guard = EDITOR_TEST_LOCK.lock().unwrap();

        let custom_id = Uuid::new_v4();
        let mut config = CustomEditorsConfig::default();
        config.custom_editors.insert(
            custom_id,
            CustomEditor {
                id: custom_id,
                name: "My Editor".to_string(),
                command: "my-editor".to_string(),
                argument: "%d".to_string(),
                icon: None,
                created_at: "now".to_string(),
            },
        );
        CustomEditorsConfig::set_cached_for_tests(config);

        let editor_config =
            EditorConfig::new(EditorType::Custom, None, Some(custom_id), None, None);
        let command = editor_config.get_command().unwrap();

        assert_eq!(command.base, "my-editor");

        CustomEditorsConfig::set_cached_for_tests(CustomEditorsConfig::default());
    }

    #[test]
    fn test_get_command_custom_editor_missing() {
        let _guard = EDITOR_TEST_LOCK.lock().unwrap();

        CustomEditorsConfig::set_cached_for_tests(CustomEditorsConfig::default());

        let missing_id = Uuid::new_v4();
        let editor_config =
            EditorConfig::new(EditorType::Custom, None, Some(missing_id), None, None);
        let err = editor_config.get_command().unwrap_err();

        assert!(matches!(
            err,
            EditorOpenError::ExecutableNotFound {
                editor_type: EditorType::Custom,
                ..
            }
        ));
    }

    #[test]
    fn test_substitute_placeholders_with_d_for_file() {
        let path = Path::new("/home/user/project/src/main.rs");
        let result = EditorConfig::substitute_placeholders("%d", path);
        assert_eq!(result, Some("/home/user/project/src".to_string()));
    }

    #[test]
    fn test_substitute_placeholders_with_d_for_directory() {
        let path = Path::new("/home/user/project");
        // Note: is_dir() returns false for non-existent paths, so we test the else branch
        // In real usage, the path would exist. The fallback uses parent() for files.
        let result = EditorConfig::substitute_placeholders("%d", path);
        // For a non-existent path, is_dir() returns false, so it takes parent
        assert_eq!(result, Some("/home/user".to_string()));
    }

    #[test]
    fn test_substitute_placeholders_with_f() {
        let path = Path::new("/home/user/project/src/main.rs");
        let result = EditorConfig::substitute_placeholders("%f", path);
        assert_eq!(result, Some("/home/user/project/src/main.rs".to_string()));
    }

    #[test]
    fn test_substitute_placeholders_with_both() {
        let path = Path::new("/home/user/project/src/main.rs");
        let result = EditorConfig::substitute_placeholders("--folder %d --file %f", path);
        assert_eq!(
            result,
            Some(
                "--folder /home/user/project/src --file /home/user/project/src/main.rs".to_string()
            )
        );
    }

    #[test]
    fn test_substitute_placeholders_no_placeholders() {
        let path = Path::new("/home/user/project");
        let result = EditorConfig::substitute_placeholders("--wait", path);
        assert_eq!(result, None);
    }

    #[test]
    fn test_substitute_placeholders_empty_template() {
        let path = Path::new("/home/user/project");
        let result = EditorConfig::substitute_placeholders("", path);
        assert_eq!(result, None);
    }
}
