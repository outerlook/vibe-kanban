#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;
use tauri_app_lib::{commands::initialize_local_mode, state::AppState};

fn main() {
    // Install rustls crypto provider before any HTTPS requests are made.
    // This is required when running inside Tauri as the provider isn't auto-detected.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    // Initialize tracing for logging
    tracing_subscriber::fmt::init();

    // Load saved state from config
    let app_state = AppState::load_from_config();
    let mcp_port = app_state.mcp_port;

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init());

    // Enable MCP bridge plugin with configured port
    builder = builder.plugin(
        tauri_plugin_mcp_bridge::Builder::new()
            .base_port(mcp_port)
            .build(),
    );

    builder
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            tauri_app_lib::commands::get_server_url,
            tauri_app_lib::commands::get_server_mode,
            tauri_app_lib::commands::set_server_mode,
            tauri_app_lib::commands::launch_mcp_server,
            tauri_app_lib::commands::stop_mcp_server,
            tauri_app_lib::commands::is_mcp_server_running
        ])
        .setup(|app| {
            let state = app.state::<AppState>();
            let state_clone = AppState {
                server_url: state.server_url.clone(),
                mcp_process_handle: state.mcp_process_handle.clone(),
                mcp_port: state.mcp_port,
                backend_port: state.backend_port,
            };

            // Initialize local mode - spawn async to not block webview
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
