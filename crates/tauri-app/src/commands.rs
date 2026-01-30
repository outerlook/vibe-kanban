//! Tauri commands for server URL management.
//!
//! These commands allow the frontend to:
//! - Get the current server URL
//! - Set a custom server URL
//! - Launch/stop the MCP server

use tauri::{AppHandle, State};

use crate::{mcp_launcher, state::AppState};

/// Returns the backend URL from config.
async fn get_backend_url(state: &AppState) -> String {
    state.backend_url.lock().await.clone()
}

/// Returns the current server URL from config.
#[tauri::command]
pub async fn get_server_url(state: State<'_, AppState>) -> Result<String, String> {
    Ok(get_backend_url(&state).await)
}

/// Sets a custom server URL.
///
/// The setting is persisted to the config file.
#[tauri::command]
pub async fn set_server_url(url: String, state: State<'_, AppState>) -> Result<(), String> {
    // Update state
    *state.backend_url.lock().await = url.clone();

    // Persist to config
    state
        .save_to_config()
        .map_err(|e| format!("Failed to save config: {}", e))?;

    tracing::info!("Set backend URL: {}", url);

    Ok(())
}

/// Launches the MCP server as a child process.
///
/// The MCP server connects to the backend URL from config.
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

    let backend_url = get_backend_url(&state).await;

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
