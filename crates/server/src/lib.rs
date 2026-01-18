pub mod error;
pub mod mcp;
pub mod middleware;
pub mod routes;

use thiserror::Error;

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
