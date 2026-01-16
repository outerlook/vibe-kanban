use std::path::PathBuf;
use std::sync::Arc;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use tokio::process::Child;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum ServerMode {
    Local,
    Remote { url: String },
}

impl Default for ServerMode {
    fn default() -> Self {
        Self::Local
    }
}

pub struct AppState {
    pub mode: Arc<RwLock<ServerMode>>,
    pub embedded_server_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    pub server_url: Arc<RwLock<String>>,
    /// Handle to the MCP server child process, if running
    pub mcp_process_handle: Arc<Mutex<Option<Child>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            mode: Arc::new(RwLock::new(ServerMode::Local)),
            embedded_server_handle: Arc::new(Mutex::new(None)),
            server_url: Arc::new(RwLock::new(String::new())),
            mcp_process_handle: Arc::new(Mutex::new(None)),
        }
    }

    fn config_path() -> Option<PathBuf> {
        let proj = if cfg!(debug_assertions) {
            ProjectDirs::from("ai", "bloop-dev", "vibe-kanban")
        } else {
            ProjectDirs::from("ai", "bloop", "vibe-kanban")
        }?;

        Some(proj.config_dir().join("tauri-config.json"))
    }

    pub fn load_from_config() -> Self {
        let state = Self::new();

        if let Some(config_path) = Self::config_path() {
            if config_path.exists() {
                if let Ok(contents) = std::fs::read_to_string(&config_path) {
                    if let Ok(mode) = serde_json::from_str::<ServerMode>(&contents) {
                        *state.mode.blocking_write() = mode;
                    }
                }
            }
        }

        state
    }

    pub fn save_to_config(&self) -> anyhow::Result<()> {
        let config_path = Self::config_path().ok_or_else(|| {
            anyhow::anyhow!("Could not determine config directory")
        })?;

        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mode = self.mode.blocking_read();
        let contents = serde_json::to_string_pretty(&*mode)?;
        std::fs::write(&config_path, contents)?;

        Ok(())
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_mode_serializes_local() {
        let mode = ServerMode::Local;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, r#"{"mode":"local"}"#);
    }

    #[test]
    fn server_mode_serializes_remote() {
        let mode = ServerMode::Remote {
            url: "https://example.com".to_string(),
        };
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, r#"{"mode":"remote","url":"https://example.com"}"#);
    }

    #[test]
    fn server_mode_deserializes_local() {
        let mode: ServerMode = serde_json::from_str(r#"{"mode":"local"}"#).unwrap();
        assert_eq!(mode, ServerMode::Local);
    }

    #[test]
    fn server_mode_deserializes_remote() {
        let mode: ServerMode =
            serde_json::from_str(r#"{"mode":"remote","url":"https://example.com"}"#).unwrap();
        assert_eq!(
            mode,
            ServerMode::Remote {
                url: "https://example.com".to_string()
            }
        );
    }
}
