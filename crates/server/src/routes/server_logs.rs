use axum::{
    Router,
    extract::{State, ws::{WebSocket, WebSocketUpgrade}},
    response::IntoResponse,
    routing::get,
};
use deployment::Deployment;
use futures_util::{SinkExt, StreamExt, TryStreamExt};

use crate::DeploymentImpl;

/// WebSocket endpoint that streams server logs to clients.
///
/// Sends all historical log entries first, then streams live entries.
pub async fn stream_server_logs_ws(
    ws: WebSocketUpgrade,
    State(deployment): State<DeploymentImpl>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_server_logs_ws(socket, deployment).await {
            tracing::warn!("server logs WS closed: {}", e);
        }
    })
}

async fn handle_server_logs_ws(
    socket: WebSocket,
    deployment: DeploymentImpl,
) -> anyhow::Result<()> {
    let stream = deployment.server_log_store().history_plus_stream();

    // Convert each ServerLogEntry to a JSON WebSocket text message
    let mut stream = stream.map_ok(|entry| {
        let json = serde_json::to_string(&entry).unwrap_or_default();
        axum::extract::ws::Message::Text(json.into())
    });

    let (mut sender, mut receiver) = socket.split();

    // Drain client->server messages so pings/pongs work
    tokio::spawn(async move { while let Some(Ok(_)) = receiver.next().await {} });

    // Forward server log entries to client
    while let Some(item) = stream.next().await {
        match item {
            Ok(msg) => {
                if sender.send(msg).await.is_err() {
                    break; // client disconnected
                }
            }
            Err(e) => {
                tracing::error!("server logs stream error: {}", e);
                break;
            }
        }
    }

    Ok(())
}

pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let _ = deployment; // unused but kept for consistency with other routers
    Router::new().nest(
        "/server-logs",
        Router::new().route("/ws", get(stream_server_logs_ws)),
    )
}
