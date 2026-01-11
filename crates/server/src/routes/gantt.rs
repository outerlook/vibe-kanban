use axum::{
    Extension, Router,
    extract::{
        Path, State,
        ws::{WebSocket, WebSocketUpgrade},
    },
    middleware::from_fn_with_state,
    response::{IntoResponse, Json as ResponseJson},
    routing::get,
};
use db::models::{gantt::GanttTask, project::Project};
use deployment::Deployment;
use futures_util::{SinkExt, StreamExt};
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError, middleware::load_project_middleware};

pub async fn get_gantt_data(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<GanttTask>>>, ApiError> {
    let tasks = GanttTask::find_by_project_id(&deployment.db().pool, project.id).await?;
    Ok(ResponseJson(ApiResponse::success(tasks)))
}

pub async fn stream_gantt_ws(
    ws: WebSocketUpgrade,
    State(deployment): State<DeploymentImpl>,
    Path(project_id): Path<Uuid>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_gantt_ws(socket, deployment, project_id).await {
            tracing::warn!("gantt WS closed: {}", e);
        }
    })
}

async fn handle_gantt_ws(
    socket: WebSocket,
    deployment: DeploymentImpl,
    project_id: Uuid,
) -> anyhow::Result<()> {
    // Get the raw stream and convert LogMsg to WebSocket messages
    let mut stream = deployment
        .events()
        .stream_gantt_raw(project_id)
        .await?
        .map(|msg| msg.map(|m| m.to_ws_message_unchecked()));

    // Split socket into sender and receiver
    let (mut sender, mut receiver) = socket.split();

    // Drain (and ignore) any client->server messages so pings/pongs work
    tokio::spawn(async move { while let Some(Ok(_)) = receiver.next().await {} });

    // Forward server messages
    while let Some(item) = stream.next().await {
        match item {
            Ok(msg) => {
                if sender.send(msg).await.is_err() {
                    break; // client disconnected
                }
            }
            Err(e) => {
                tracing::error!("gantt stream error: {}", e);
                break;
            }
        }
    }
    Ok(())
}

pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let project_gantt = Router::new()
        .route("/gantt", get(get_gantt_data))
        .layer(from_fn_with_state(deployment.clone(), load_project_middleware));

    Router::new()
        .nest("/projects/{project_id}", project_gantt)
        .route("/projects/{project_id}/gantt/stream/ws", get(stream_gantt_ws))
}
