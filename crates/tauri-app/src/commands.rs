//! Tauri commands for server URL and mode management.
//!
//! These commands allow the frontend to:
//! - Get the current server URL (embedded or remote)
//! - Get/set the server mode (local or remote)

use tauri::State;

use crate::embedded_server::start_embedded_server;
use crate::state::{AppState, ServerMode};

/// Returns the current server URL.
///
/// In Local mode, this is the URL of the embedded server (e.g., `http://127.0.0.1:54321`).
/// In Remote mode, this is the configured remote server URL.
#[tauri::command]
pub async fn get_server_url(state: State<'_, AppState>) -> Result<String, String> {
    let url = state.server_url.read().await;
    Ok(url.clone())
}

/// Returns the current server mode (Local or Remote).
#[tauri::command]
pub async fn get_server_mode(state: State<'_, AppState>) -> Result<ServerMode, String> {
    let mode = state.mode.read().await;
    Ok(mode.clone())
}

/// Sets the server mode and handles the transition.
///
/// - Switching to Local: Starts the embedded server
/// - Switching to Remote: Stops the embedded server (if running), stores the remote URL
///
/// The mode is persisted to the config file for use on the next app launch.
#[tauri::command]
pub async fn set_server_mode(mode: ServerMode, state: State<'_, AppState>) -> Result<(), String> {
    let current_mode = state.mode.read().await.clone();

    // Early return if mode hasn't changed
    if current_mode == mode {
        return Ok(());
    }

    // Stop embedded server if currently running
    {
        let mut handle_guard = state.embedded_server_handle.lock().await;
        if let Some(handle) = handle_guard.take() {
            tracing::info!("Stopping embedded server...");
            handle.abort();
        }
    }

    // Handle the mode transition
    match &mode {
        ServerMode::Local => {
            tracing::info!("Switching to Local mode, starting embedded server...");

            let (url, handle) = start_embedded_server()
                .await
                .map_err(|e| format!("Failed to start embedded server: {}", e))?;

            // Update state
            *state.server_url.write().await = url;
            *state.embedded_server_handle.lock().await = Some(handle);
        }
        ServerMode::Remote { url } => {
            tracing::info!("Switching to Remote mode with URL: {}", url);
            *state.server_url.write().await = url.clone();
        }
    }

    // Update mode and persist
    *state.mode.write().await = mode;

    // Persist to config (blocking operation, but runs quickly)
    state
        .save_to_config()
        .map_err(|e| format!("Failed to save config: {}", e))?;

    Ok(())
}

/// Starts the embedded server for Local mode.
///
/// This is called during app initialization when the saved mode is Local.
/// It updates the AppState with the server URL and handle.
pub async fn initialize_local_mode(state: &AppState) -> Result<(), String> {
    let mode = state.mode.read().await.clone();

    if mode != ServerMode::Local {
        return Ok(());
    }

    tracing::info!("Initializing Local mode, starting embedded server...");

    let (url, handle) = start_embedded_server()
        .await
        .map_err(|e| format!("Failed to start embedded server: {}", e))?;

    *state.server_url.write().await = url;
    *state.embedded_server_handle.lock().await = Some(handle);

    Ok(())
}
