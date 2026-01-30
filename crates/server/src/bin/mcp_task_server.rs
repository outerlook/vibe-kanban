use rmcp::{ServiceExt, transport::stdio};
use server::mcp::task_server::TaskServer;
use tracing_subscriber::{EnvFilter, prelude::*};
use utils::sentry::{self as sentry_utils, SentrySource, sentry_layer};

fn main() -> anyhow::Result<()> {
    sentry_utils::init_once(SentrySource::Mcp);
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tracing_subscriber::registry()
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_writer(std::io::stderr)
                        .with_filter(EnvFilter::new("debug")),
                )
                .with(sentry_layer())
                .init();

            let version = env!("CARGO_PKG_VERSION");
            tracing::debug!("[MCP] Starting MCP task server version {version}...");

            // Read backend URL from environment variable (required)
            let base_url = if let Ok(url) = std::env::var("VIBE_BACKEND_URL") {
                tracing::info!("[MCP] Using backend URL from VIBE_BACKEND_URL: {}", url);
                url
            } else {
                // Fallback to constructing URL from HOST and BACKEND_PORT env vars
                let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
                let port = std::env::var("BACKEND_PORT")
                    .or_else(|_| std::env::var("PORT"))
                    .map_err(|_| {
                        anyhow::anyhow!("VIBE_BACKEND_URL or BACKEND_PORT/PORT must be set")
                    })?;

                let port: u16 = port
                    .parse()
                    .map_err(|e| anyhow::anyhow!("Invalid port value '{}': {}", port, e))?;

                let url = format!("http://{}:{}", host, port);
                tracing::info!("[MCP] Using backend URL: {}", url);
                url
            };

            let service = TaskServer::new(&base_url)
                .init()
                .await
                .serve(stdio())
                .await
                .map_err(|e| {
                    tracing::error!("serving error: {:?}", e);
                    e
                })?;

            service.waiting().await?;
            Ok(())
        })
}
