use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::{
    process::Child,
    sync::{Mutex, RwLock},
    task::JoinHandle,
};
use utils::assets::asset_dir;

/// Default MCP bridge port (matches tauri-plugin-mcp-bridge default)
pub const DEFAULT_MCP_PORT: u16 = 9223;

/// Default backend API port (0 = auto-assign)
pub const DEFAULT_BACKEND_PORT: u16 = 0;

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

/// Configuration stored in tauri-config.json
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct TauriConfig {
    /// Server mode (local or remote)
    #[serde(flatten)]
    pub server_mode: ServerMode,
    /// MCP bridge WebSocket server port
    #[serde(default = "default_mcp_port")]
    pub mcp_port: u16,
    /// Backend API port (0 = auto-assign)
    #[serde(default = "default_backend_port")]
    pub backend_port: u16,
}

fn default_mcp_port() -> u16 {
    DEFAULT_MCP_PORT
}

fn default_backend_port() -> u16 {
    DEFAULT_BACKEND_PORT
}

pub struct AppState {
    pub mode: Arc<RwLock<ServerMode>>,
    pub embedded_server_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    pub server_url: Arc<RwLock<String>>,
    /// Handle to the MCP server child process, if running
    pub mcp_process_handle: Arc<Mutex<Option<Child>>>,
    /// MCP bridge port from config
    pub mcp_port: u16,
    /// Backend API port from config (0 = auto-assign)
    pub backend_port: u16,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            mode: Arc::new(RwLock::new(ServerMode::Local)),
            embedded_server_handle: Arc::new(Mutex::new(None)),
            server_url: Arc::new(RwLock::new(String::new())),
            mcp_process_handle: Arc::new(Mutex::new(None)),
            mcp_port: DEFAULT_MCP_PORT,
            backend_port: DEFAULT_BACKEND_PORT,
        }
    }

    fn config_path() -> std::path::PathBuf {
        asset_dir().join("tauri-config.json")
    }

    pub fn load_from_config() -> Self {
        let mut state = Self::new();
        let config_path = Self::config_path();

        if config_path.exists() {
            if let Ok(contents) = std::fs::read_to_string(&config_path) {
                match serde_json::from_str::<TauriConfig>(&contents) {
                    Ok(config) => {
                        tracing::info!(
                            "Loaded config from {}: mode={:?}, mcp_port={}, backend_port={}",
                            config_path.display(),
                            config.server_mode,
                            config.mcp_port,
                            config.backend_port
                        );
                        *state.mode.blocking_write() = config.server_mode;
                        state.mcp_port = config.mcp_port;
                        state.backend_port = config.backend_port;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to parse config at {}: {}, using defaults",
                            config_path.display(),
                            e
                        );
                    }
                }
            }
        } else {
            tracing::info!(
                "No config found at {}, using defaults (mcp_port={})",
                config_path.display(),
                DEFAULT_MCP_PORT
            );
        }

        state
    }

    pub fn save_to_config(&self) -> anyhow::Result<()> {
        let config_path = Self::config_path();

        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mode = self.mode.blocking_read();
        let config = TauriConfig {
            server_mode: mode.clone(),
            mcp_port: self.mcp_port,
            backend_port: self.backend_port,
        };
        let contents = serde_json::to_string_pretty(&config)?;
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

    #[test]
    fn tauri_config_deserializes_with_mcp_port() {
        let config: TauriConfig =
            serde_json::from_str(r#"{"mode":"local","mcp_port":9876}"#).unwrap();
        assert_eq!(config.server_mode, ServerMode::Local);
        assert_eq!(config.mcp_port, 9876);
    }

    #[test]
    fn tauri_config_deserializes_without_mcp_port_uses_default() {
        let config: TauriConfig = serde_json::from_str(r#"{"mode":"local"}"#).unwrap();
        assert_eq!(config.server_mode, ServerMode::Local);
        assert_eq!(config.mcp_port, DEFAULT_MCP_PORT);
    }

    #[test]
    fn tauri_config_serializes_with_ports() {
        let config = TauriConfig {
            server_mode: ServerMode::Local,
            mcp_port: 9876,
            backend_port: 8080,
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains(r#""mode":"local""#));
        assert!(json.contains(r#""mcp_port":9876"#));
        assert!(json.contains(r#""backend_port":8080"#));
    }

    #[test]
    fn tauri_config_deserializes_with_backend_port() {
        let config: TauriConfig =
            serde_json::from_str(r#"{"mode":"local","backend_port":8080}"#).unwrap();
        assert_eq!(config.server_mode, ServerMode::Local);
        assert_eq!(config.backend_port, 8080);
        assert_eq!(config.mcp_port, DEFAULT_MCP_PORT); // defaults when not specified
    }

    #[test]
    fn tauri_config_deserializes_without_backend_port_uses_default() {
        let config: TauriConfig = serde_json::from_str(r#"{"mode":"local"}"#).unwrap();
        assert_eq!(config.backend_port, DEFAULT_BACKEND_PORT);
    }
}
