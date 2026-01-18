#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri_app_lib::state::AppState;

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
            tauri_app_lib::commands::set_server_url,
            tauri_app_lib::commands::launch_mcp_server,
            tauri_app_lib::commands::stop_mcp_server,
            tauri_app_lib::commands::is_mcp_server_running
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
