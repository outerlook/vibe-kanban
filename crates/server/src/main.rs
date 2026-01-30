use std::sync::Arc;

use anyhow::{self, Error as AnyhowError};
use deployment::{Deployment, DeploymentError};
use server::{DeploymentImpl, perform_cleanup_actions, routes, shutdown_signal};
use services::services::container::ContainerService;
use sqlx::Error as SqlxError;
use strip_ansi_escapes::strip;
use thiserror::Error;
use tracing_subscriber::{EnvFilter, prelude::*};
use utils::{
    assets::{alerts_dir, asset_dir},
    browser::open_browser,
    sentry::{self as sentry_utils, SentrySource, sentry_layer},
    server_log_layer::ServerLogLayer,
    server_log_store::ServerLogStore,
};

#[derive(Debug, Error)]
pub enum VibeKanbanError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sqlx(#[from] SqlxError),
    #[error(transparent)]
    Deployment(#[from] DeploymentError),
    #[error(transparent)]
    Other(#[from] AnyhowError),
}

#[tokio::main]
async fn main() -> Result<(), VibeKanbanError> {
    // Install rustls crypto provider before any TLS operations (required for GitHub API calls)
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    sentry_utils::init_once(SentrySource::Backend);

    // Create the server log store for capturing logs to stream via WebSocket
    let server_log_store = Arc::new(ServerLogStore::new());

    let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let filter_string = format!(
        "warn,server={level},services={level},db={level},executors={level},deployment={level},local_deployment={level},utils={level}",
        level = log_level
    );
    let fmt_filter = EnvFilter::try_new(&filter_string).expect("Failed to create tracing filter");
    let log_layer_filter =
        EnvFilter::try_new(&filter_string).expect("Failed to create tracing filter");
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(fmt_filter))
        .with(ServerLogLayer::new(server_log_store.clone()).with_filter(log_layer_filter))
        .with(sentry_layer())
        .init();

    // Create asset directory if it doesn't exist
    if !asset_dir().exists() {
        std::fs::create_dir_all(asset_dir())?;
    }
    let alerts_dir = alerts_dir();
    tracing::info!("Alerts directory: {}", alerts_dir.display());
    if let Err(e) = std::fs::create_dir_all(&alerts_dir) {
        tracing::warn!(
            "Failed to create alerts directory {}: {}",
            alerts_dir.display(),
            e
        );
    }

    let deployment = DeploymentImpl::new().await?;
    deployment.update_sentry_scope().await?;
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
    deployment.spawn_pr_monitor_service().await;
    deployment.spawn_embedding_worker();
    deployment.spawn_backup_service().await;
    deployment
        .track_if_analytics_allowed("session_start", serde_json::json!({}))
        .await;
    // Pre-warm file search cache for most active projects
    let deployment_for_cache = deployment.clone();
    tokio::spawn(async move {
        if let Err(e) = deployment_for_cache
            .file_search_cache()
            .warm_most_active(&deployment_for_cache.db().pool, 3)
            .await
        {
            tracing::warn!("Failed to warm file search cache: {}", e);
        }
    });

    // Verify shared tasks in background
    let deployment_for_verification = deployment.clone();
    tokio::spawn(async move {
        if let Some(publisher) = deployment_for_verification.container().share_publisher()
            && let Err(e) = publisher.cleanup_shared_tasks().await
        {
            tracing::warn!("Failed to verify shared tasks: {}", e);
        }
    });

    let app_router = routes::router(deployment.clone());

    let port_str = std::env::var("BACKEND_PORT")
        .or_else(|_| std::env::var("PORT"))
        .map_err(|_| anyhow::anyhow!("BACKEND_PORT or PORT environment variable must be set"))?;

    // remove any ANSI codes, then parse
    let cleaned =
        String::from_utf8(strip(port_str.as_bytes())).expect("UTF-8 after stripping ANSI");
    let port: u16 = cleaned
        .trim()
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid port value '{}': {}", cleaned.trim(), e))?;

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let listener = tokio::net::TcpListener::bind(format!("{host}:{port}")).await?;
    let actual_port = listener.local_addr()?.port(); // get → 53427 (example)

    tracing::info!("Server running on http://{host}:{actual_port}");

    if !cfg!(debug_assertions) {
        tracing::info!("Opening browser...");
        tokio::spawn(async move {
            if let Err(e) = open_browser(&format!("http://127.0.0.1:{actual_port}")).await {
                tracing::warn!(
                    "Failed to open browser automatically: {}. Please open http://127.0.0.1:{} manually.",
                    e,
                    actual_port
                );
            }
        });
    }

    axum::serve(listener, app_router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    perform_cleanup_actions(&deployment).await;

    Ok(())
}
