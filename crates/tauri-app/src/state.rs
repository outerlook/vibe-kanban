use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::{process::Child, sync::Mutex};
use utils::assets::asset_dir;

/// Default MCP bridge port (matches tauri-plugin-mcp-bridge default)
pub const DEFAULT_MCP_PORT: u16 = 9223;

/// Default backend API port (0 = auto-assign)
pub const DEFAULT_BACKEND_PORT: u16 = 0;

/// Configuration stored in tauri-config.json
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct TauriConfig {
    /// Server URL (None = auto-discovery)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
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
    /// Server URL - configured URL or empty for auto-discovery
    pub server_url: Arc<Mutex<String>>,
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
            server_url: Arc::new(Mutex::new(String::new())),
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
                            "Loaded config from {}: server_url={:?}, mcp_port={}, backend_port={}",
                            config_path.display(),
                            config.server_url,
                            config.mcp_port,
                            config.backend_port
                        );
                        *state.server_url.blocking_lock() =
                            config.server_url.unwrap_or_default();
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

        let url = self.server_url.blocking_lock();
        let config = TauriConfig {
            server_url: if url.is_empty() {
                None
            } else {
                Some(url.clone())
            },
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
    fn tauri_config_serializes_with_url() {
        let config = TauriConfig {
            server_url: Some("https://example.com".to_string()),
            mcp_port: 9876,
            backend_port: 8080,
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains(r#""server_url":"https://example.com""#));
        assert!(json.contains(r#""mcp_port":9876"#));
        assert!(json.contains(r#""backend_port":8080"#));
    }

    #[test]
    fn tauri_config_serializes_without_url() {
        let config = TauriConfig {
            server_url: None,
            mcp_port: 9223,
            backend_port: 0,
        };
        let json = serde_json::to_string(&config).unwrap();
        // server_url should be omitted when None
        assert!(!json.contains("server_url"));
        assert!(json.contains(r#""mcp_port":9223"#));
    }

    #[test]
    fn tauri_config_deserializes_with_url() {
        let config: TauriConfig =
            serde_json::from_str(r#"{"server_url":"https://example.com","mcp_port":9876}"#)
                .unwrap();
        assert_eq!(
            config.server_url,
            Some("https://example.com".to_string())
        );
        assert_eq!(config.mcp_port, 9876);
    }

    #[test]
    fn tauri_config_deserializes_without_url() {
        let config: TauriConfig = serde_json::from_str(r#"{"mcp_port":9223}"#).unwrap();
        assert_eq!(config.server_url, None);
        assert_eq!(config.mcp_port, 9223);
    }

    #[test]
    fn tauri_config_deserializes_empty_object() {
        let config: TauriConfig = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(config.server_url, None);
        assert_eq!(config.mcp_port, DEFAULT_MCP_PORT);
        assert_eq!(config.backend_port, DEFAULT_BACKEND_PORT);
    }

    #[test]
    fn tauri_config_deserializes_with_backend_port() {
        let config: TauriConfig =
            serde_json::from_str(r#"{"backend_port":8080}"#).unwrap();
        assert_eq!(config.server_url, None);
        assert_eq!(config.backend_port, 8080);
        assert_eq!(config.mcp_port, DEFAULT_MCP_PORT);
    }
}
