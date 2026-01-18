use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::{process::Child, sync::Mutex};
use utils::assets::asset_dir;

/// Default MCP bridge port (matches tauri-plugin-mcp-bridge default)
pub const DEFAULT_MCP_PORT: u16 = 9223;

/// Default backend URL
pub const DEFAULT_BACKEND_URL: &str = "http://127.0.0.1:9876";

/// Configuration stored in tauri-config.json
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TauriConfig {
    /// Backend server URL (always required, no auto-discovery)
    #[serde(default = "default_backend_url")]
    pub backend_url: String,
    /// MCP bridge WebSocket server port
    #[serde(default = "default_mcp_port")]
    pub mcp_port: u16,
}

impl Default for TauriConfig {
    fn default() -> Self {
        Self {
            backend_url: DEFAULT_BACKEND_URL.to_string(),
            mcp_port: DEFAULT_MCP_PORT,
        }
    }
}

fn default_backend_url() -> String {
    DEFAULT_BACKEND_URL.to_string()
}

fn default_mcp_port() -> u16 {
    DEFAULT_MCP_PORT
}

pub struct AppState {
    /// Backend server URL
    pub backend_url: Arc<Mutex<String>>,
    /// Handle to the MCP server child process, if running
    pub mcp_process_handle: Arc<Mutex<Option<Child>>>,
    /// MCP bridge port from config
    pub mcp_port: u16,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            backend_url: Arc::new(Mutex::new(DEFAULT_BACKEND_URL.to_string())),
            mcp_process_handle: Arc::new(Mutex::new(None)),
            mcp_port: DEFAULT_MCP_PORT,
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
                            "Loaded config from {}: backend_url={}, mcp_port={}",
                            config_path.display(),
                            config.backend_url,
                            config.mcp_port
                        );
                        *state.backend_url.blocking_lock() = config.backend_url;
                        state.mcp_port = config.mcp_port;
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
                "No config found at {}, using defaults (backend_url={}, mcp_port={})",
                config_path.display(),
                DEFAULT_BACKEND_URL,
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

        let url = self.backend_url.blocking_lock();
        let config = TauriConfig {
            backend_url: url.clone(),
            mcp_port: self.mcp_port,
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
    fn tauri_config_serializes() {
        let config = TauriConfig {
            backend_url: "https://example.com".to_string(),
            mcp_port: 9876,
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains(r#""backend_url":"https://example.com""#));
        assert!(json.contains(r#""mcp_port":9876"#));
    }

    #[test]
    fn tauri_config_deserializes_with_url() {
        let config: TauriConfig =
            serde_json::from_str(r#"{"backend_url":"https://example.com","mcp_port":9876}"#)
                .unwrap();
        assert_eq!(config.backend_url, "https://example.com");
        assert_eq!(config.mcp_port, 9876);
    }

    #[test]
    fn tauri_config_deserializes_empty_object_uses_defaults() {
        let config: TauriConfig = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(config.backend_url, DEFAULT_BACKEND_URL);
        assert_eq!(config.mcp_port, DEFAULT_MCP_PORT);
    }

    #[test]
    fn tauri_config_default() {
        let config = TauriConfig::default();
        assert_eq!(config.backend_url, DEFAULT_BACKEND_URL);
        assert_eq!(config.mcp_port, DEFAULT_MCP_PORT);
    }
}
