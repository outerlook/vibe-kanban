//! MCP server launcher for the Tauri app.
//!
//! Handles spawning and stopping the MCP task server binary as a child process.
//! The MCP server connects to the backend (embedded or remote) for task management.

use std::process::Stdio;
use tauri::{AppHandle, Manager};
use thiserror::Error;
use tokio::process::{Child, Command};

#[derive(Error, Debug)]
pub enum McpLauncherError {
    #[error("Failed to resolve MCP binary path: {0}")]
    BinaryNotFound(String),

    #[error("Failed to spawn MCP server process: {0}")]
    SpawnFailed(#[from] std::io::Error),

    #[error("Failed to stop MCP server: {0}")]
    StopFailed(String),
}

/// Resolves the path to the MCP server binary.
///
/// In development, uses the cargo build output directory.
/// In production, uses Tauri's resource resolver to find the bundled binary.
fn resolve_mcp_binary_path(app: &AppHandle) -> Result<std::path::PathBuf, McpLauncherError> {
    // Try to resolve using Tauri's sidecar mechanism
    // The binary is configured as "binaries/mcp_task_server" in tauri.conf.json
    let binary_name = if cfg!(target_os = "windows") {
        "mcp_task_server.exe"
    } else {
        "mcp_task_server"
    };

    // In development, try the cargo target directory first
    if cfg!(debug_assertions) {
        // Try common development paths
        let dev_paths = [
            // When running with `cargo tauri dev`
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.join(binary_name))),
            // Fallback to target/debug
            Some(std::path::PathBuf::from(format!(
                "target/debug/{}",
                binary_name
            ))),
        ];

        for path in dev_paths.into_iter().flatten() {
            if path.exists() {
                tracing::info!("Found MCP binary at dev path: {:?}", path);
                return Ok(path);
            }
        }
    }

    // Use Tauri's resource resolver for bundled binary
    let resource_path = app
        .path()
        .resource_dir()
        .map_err(|e: tauri::Error| McpLauncherError::BinaryNotFound(e.to_string()))?;

    // The external binary is placed alongside the main executable
    let bundled_path = resource_path.join(binary_name);
    if bundled_path.exists() {
        tracing::info!("Found MCP binary at bundled path: {:?}", bundled_path);
        return Ok(bundled_path);
    }

    // Also try in the same directory as the current executable
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let sibling_path = exe_dir.join(binary_name);
            if sibling_path.exists() {
                tracing::info!("Found MCP binary as sibling: {:?}", sibling_path);
                return Ok(sibling_path);
            }
        }
    }

    Err(McpLauncherError::BinaryNotFound(format!(
        "Could not find {} in any expected location",
        binary_name
    )))
}

/// Launches the MCP server as a child process.
///
/// The server is configured to connect to the specified backend URL via
/// the `VIBE_BACKEND_URL` environment variable.
///
/// # Arguments
/// * `app` - The Tauri app handle, used to resolve the binary path
/// * `backend_url` - The URL of the backend server (embedded or remote)
///
/// # Returns
/// The child process handle for lifecycle management.
pub async fn launch_mcp_server(
    app: &AppHandle,
    backend_url: &str,
) -> Result<Child, McpLauncherError> {
    let binary_path = resolve_mcp_binary_path(app)?;

    tracing::info!(
        "Launching MCP server from {:?} with backend URL: {}",
        binary_path,
        backend_url
    );

    let child = Command::new(&binary_path)
        .env("VIBE_BACKEND_URL", backend_url)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    tracing::info!("MCP server started with PID: {:?}", child.id());

    Ok(child)
}

/// Stops the MCP server process.
///
/// Attempts a graceful shutdown first, then falls back to forceful termination.
///
/// # Arguments
/// * `child` - Mutable reference to the child process handle
pub async fn stop_mcp_server(child: &mut Child) -> Result<(), McpLauncherError> {
    let pid = child.id();
    tracing::info!("Stopping MCP server (PID: {:?})...", pid);

    // kill() sends SIGKILL on Unix or TerminateProcess on Windows
    child
        .kill()
        .await
        .map_err(|e| McpLauncherError::StopFailed(e.to_string()))?;

    // Wait for the process to fully terminate
    let _ = child.wait().await;

    tracing::info!("MCP server stopped");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_binary_not_found() {
        let err = McpLauncherError::BinaryNotFound("test path".to_string());
        assert!(err.to_string().contains("MCP binary path"));
    }

    #[test]
    fn error_display_stop_failed() {
        let err = McpLauncherError::StopFailed("test error".to_string());
        assert!(err.to_string().contains("stop MCP server"));
    }
}
