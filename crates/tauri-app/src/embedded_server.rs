//! Embedded server module for running the Axum server within the Tauri app.
//!
//! This module provides lifecycle management for the embedded HTTP server,
//! spawning it in a background Tokio task so it doesn't block the Tauri main thread.

use deployment::{Deployment, DeploymentError};
use local_deployment::LocalDeployment;
use server::ServerError;
use thiserror::Error;
use tokio::task::JoinHandle;

/// Errors that can occur when starting the embedded server.
#[derive(Debug, Error)]
pub enum EmbeddedServerError {
    /// Failed to initialize the deployment (database, services, etc.)
    #[error("Deployment initialization failed: {0}")]
    Deployment(#[from] DeploymentError),

    /// Failed to start the HTTP server (binding, etc.)
    #[error("Server startup failed: {0}")]
    Server(#[from] ServerError),
}

/// Starts the embedded Axum server for local mode.
///
/// This function:
/// - Creates a new `LocalDeployment` instance with all required services
/// - Starts the Axum HTTP server on the specified port (0 = auto-assign)
/// - Returns the server URL and a join handle for the background task
///
/// # Arguments
/// * `port` - Port to bind to (0 for auto-assign)
///
/// # Returns
/// * `Ok((url, handle))` - The server URL (e.g., `http://127.0.0.1:54321`) and task handle
/// * `Err(EmbeddedServerError)` - If deployment or server initialization fails
///
/// # Example
/// ```ignore
/// let (url, handle) = start_embedded_server(8080).await?;
/// println!("Server running at {}", url);
/// // Later, to wait for server shutdown:
/// handle.await?;
/// ```
pub async fn start_embedded_server(port: u16) -> Result<(String, JoinHandle<()>), EmbeddedServerError> {
    tracing::info!("Initializing embedded server on port {}...", port);

    let deployment = LocalDeployment::new().await?;
    tracing::info!("Deployment initialized successfully");

    let (url, handle) = server::start_server(deployment, port).await?;
    tracing::info!("Embedded server started at {}", url);

    Ok((url, handle))
}
