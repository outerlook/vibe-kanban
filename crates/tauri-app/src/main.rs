#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;
use tauri_app_lib::commands::initialize_local_mode;
use tauri_app_lib::state::AppState;

fn main() {
    // Initialize tracing for logging
    tracing_subscriber::fmt::init();

    // Load saved state from config
    let app_state = AppState::load_from_config();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            tauri_app_lib::commands::get_server_url,
            tauri_app_lib::commands::get_server_mode,
            tauri_app_lib::commands::set_server_mode
        ])
        .setup(|app| {
            let state = app.state::<AppState>();
            let state_clone = AppState {
                mode: state.mode.clone(),
                embedded_server_handle: state.embedded_server_handle.clone(),
                server_url: state.server_url.clone(),
            };

            // Start embedded server if in Local mode
            tauri::async_runtime::spawn(async move {
                if let Err(e) = initialize_local_mode(&state_clone).await {
                    tracing::error!("Failed to initialize local mode: {}", e);
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
