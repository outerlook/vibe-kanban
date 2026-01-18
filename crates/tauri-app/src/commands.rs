//! Tauri commands for server URL management.
//!
//! These commands allow the frontend to:
//! - Get the current server URL (auto-discovered or custom)
//! - Set/clear a custom server URL
//! - Launch/stop the MCP server

use tauri::{AppHandle, State};

use crate::{mcp_launcher, state::AppState};

/// Default fallback port when port file is not found.
const DEFAULT_PORT: u16 = 9876;

/// Resolves the server URL from state or auto-discovery.
async fn resolve_server_url(state: &AppState) -> String {
    // Check for custom URL first
    let custom_url = state.server_url.lock().await;
    if !custom_url.is_empty() {
        return custom_url.clone();
    }
    drop(custom_url);

    // Auto-discover via port file
    match utils::port_file::read_port_file("vibe-kanban").await {
        Ok(port) => {
            tracing::debug!("Discovered server on port {} from port file", port);
            format!("http://127.0.0.1:{}", port)
        }
        Err(e) => {
            tracing::debug!(
                "Port file not found or invalid ({}), using fallback port {}",
                e,
                DEFAULT_PORT
            );
            format!("http://127.0.0.1:{}", DEFAULT_PORT)
        }
    }
}

/// Returns the current server URL.
///
/// Resolution order:
/// 1. Custom URL if set in state/config
/// 2. Auto-discovered port from port file → `http://127.0.0.1:{port}`
/// 3. Fallback to `http://127.0.0.1:9876`
#[tauri::command]
pub async fn get_server_url(state: State<'_, AppState>) -> Result<String, String> {
    Ok(resolve_server_url(&state).await)
}

/// Sets or clears a custom server URL.
///
/// - `Some(url)` → Use this URL instead of auto-discovery
/// - `None` → Clear custom URL, use auto-discovery
///
/// The setting is persisted to the config file.
#[tauri::command]
pub async fn set_server_url(
    url: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let new_url = url.unwrap_or_default();

    // Update state
    *state.server_url.lock().await = new_url.clone();

    // Persist to config
    state
        .save_to_config()
        .map_err(|e| format!("Failed to save config: {}", e))?;

    if new_url.is_empty() {
        tracing::info!("Cleared custom server URL, using auto-discovery");
    } else {
        tracing::info!("Set custom server URL: {}", new_url);
    }

    Ok(())
}

/// Launches the MCP server as a child process.
///
/// The MCP server connects to the current backend (discovered or custom).
/// If an MCP server is already running, it will be stopped first.
#[tauri::command]
pub async fn launch_mcp_server(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    // Stop existing MCP server if running
    {
        let mut handle_guard = state.mcp_process_handle.lock().await;
        if let Some(mut child) = handle_guard.take() {
            tracing::info!("Stopping existing MCP server before launching new one...");
            if let Err(e) = mcp_launcher::stop_mcp_server(&mut child).await {
                tracing::warn!("Failed to stop existing MCP server: {}", e);
            }
        }
    }

    let backend_url = resolve_server_url(&state).await;

    let child = mcp_launcher::launch_mcp_server(&app, &backend_url)
        .await
        .map_err(|e| format!("Failed to launch MCP server: {}", e))?;

    *state.mcp_process_handle.lock().await = Some(child);

    tracing::info!("MCP server launched successfully");
    Ok(())
}

/// Stops the running MCP server.
///
/// If no MCP server is running, this is a no-op.
#[tauri::command]
pub async fn stop_mcp_server(state: State<'_, AppState>) -> Result<(), String> {
    let mut handle_guard = state.mcp_process_handle.lock().await;

    if let Some(mut child) = handle_guard.take() {
        mcp_launcher::stop_mcp_server(&mut child)
            .await
            .map_err(|e| format!("Failed to stop MCP server: {}", e))?;
        tracing::info!("MCP server stopped");
    } else {
        tracing::debug!("No MCP server running");
    }

    Ok(())
}

/// Returns whether the MCP server is currently running.
#[tauri::command]
pub async fn is_mcp_server_running(state: State<'_, AppState>) -> Result<bool, String> {
    let handle_guard = state.mcp_process_handle.lock().await;
    Ok(handle_guard.is_some())
}
