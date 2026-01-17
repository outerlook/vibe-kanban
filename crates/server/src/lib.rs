pub mod error;
pub mod mcp;
pub mod middleware;
pub mod routes;

use std::net::SocketAddr;

use thiserror::Error;
use tokio::task::JoinHandle;

// #[cfg(feature = "cloud")]
// type DeploymentImpl = vibe_kanban_cloud::deployment::CloudDeployment;
// #[cfg(not(feature = "cloud"))]
pub type DeploymentImpl = local_deployment::LocalDeployment;

/// Error type for server startup
#[derive(Debug, Error)]
pub enum ServerError {
    #[error("Failed to bind to address: {0}")]
    Bind(#[from] std::io::Error),
}

/// Spawns background tasks for startup cleanup and data backfills.
/// These tasks run asynchronously and don't block server startup.
fn spawn_startup_tasks(deployment: &DeploymentImpl) {
    use deployment::Deployment;
    use services::services::container::ContainerService;

    // Cleanup orphaned executions (processes that were running when the server crashed)
    let deployment_for_orphan_cleanup = deployment.clone();
    tokio::spawn(async move {
        if let Err(e) = deployment_for_orphan_cleanup
            .container()
            .cleanup_orphan_executions()
            .await
        {
            tracing::error!("Failed to cleanup orphan executions: {}", e);
        }
    });

    // Backfill before_head_commit for legacy execution processes
    let deployment_for_before_head_backfill = deployment.clone();
    tokio::spawn(async move {
        if let Err(e) = deployment_for_before_head_backfill
            .container()
            .backfill_before_head_commits()
            .await
        {
            tracing::error!("Failed to backfill before head commits: {}", e);
        }
    });

    // Backfill repo names for legacy workspace repos
    let deployment_for_repo_name_backfill = deployment.clone();
    tokio::spawn(async move {
        if let Err(e) = deployment_for_repo_name_backfill
            .container()
            .backfill_repo_names()
            .await
        {
            tracing::error!("Failed to backfill repo names: {}", e);
        }
    });
}

/// Starts the Axum HTTP server with the given deployment.
///
/// This function:
/// - Creates the Axum router from the deployment
/// - Binds to `127.0.0.1:<port>` (0 = auto-assign port)
/// - Spawns the server task with graceful shutdown handling
/// - Returns the server URL and a `JoinHandle` for the server task
///
/// # Arguments
/// * `deployment` - A pre-created `DeploymentImpl` instance
/// * `port` - Port to bind to (0 for auto-assign)
///
/// # Returns
/// * `Ok((url, handle))` - The server URL (e.g., `http://127.0.0.1:54321`) and a `JoinHandle`
/// * `Err(ServerError)` - If binding to the address fails
pub async fn start_server(
    deployment: DeploymentImpl,
    port: u16,
) -> Result<(String, JoinHandle<()>), ServerError> {
    // Spawn background tasks for startup cleanup and backfills
    spawn_startup_tasks(&deployment);

    let app_router = routes::router(deployment);

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    let addr: SocketAddr = listener.local_addr()?;
    let url = format!("http://{}", addr);

    tracing::info!("Server running on {}", url);

    let handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app_router)
            .with_graceful_shutdown(shutdown_signal())
            .await
        {
            tracing::error!("Server error: {}", e);
        }
    });

    Ok((url, handle))
}

/// Waits for shutdown signals (Ctrl+C or SIGTERM on Unix).
pub async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!("Failed to install Ctrl+C handler: {e}");
        }
    };

    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let terminate = async {
            if let Ok(mut sigterm) = signal(SignalKind::terminate()) {
                sigterm.recv().await;
            } else {
                tracing::error!("Failed to install SIGTERM handler");
                std::future::pending::<()>().await;
            }
        };

        tokio::select! {
            _ = ctrl_c => {},
            _ = terminate => {},
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await;
    }
}

/// Performs cleanup actions on server shutdown.
pub async fn perform_cleanup_actions(deployment: &DeploymentImpl) {
    use deployment::Deployment;
    use services::services::container::ContainerService;
    deployment
        .container()
        .kill_all_running_processes()
        .await
        .expect("Failed to cleanly kill running execution processes");
}
