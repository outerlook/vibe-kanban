use axum::{
    Extension, Router,
    extract::{
        Path, Query, State,
        ws::{WebSocket, WebSocketUpgrade},
    },
    middleware::from_fn_with_state,
    response::{IntoResponse, Json as ResponseJson},
    routing::get,
};
use db::models::{gantt::GanttTask, project::Project};
use deployment::Deployment;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{
    routes::ws_helpers::forward_stream_to_ws, DeploymentImpl, error::ApiError,
    middleware::load_project_middleware,
};

#[derive(Debug, Deserialize)]
pub struct GanttQuery {
    pub offset: Option<i32>,
    pub limit: Option<i32>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PaginatedGanttTasks {
    pub tasks: Vec<GanttTask>,
    pub total: i64,
    pub has_more: bool,
}

pub async fn get_gantt_data(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<GanttQuery>,
) -> Result<ResponseJson<ApiResponse<PaginatedGanttTasks>>, ApiError> {
    const DEFAULT_LIMIT: i32 = 50;
    const MAX_LIMIT: i32 = 200;
    const DEFAULT_OFFSET: i32 = 0;

    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(0, MAX_LIMIT) as i64;
    let offset = query.offset.unwrap_or(DEFAULT_OFFSET).max(0) as i64;

    let (tasks, total) =
        GanttTask::find_paginated_by_project_id(&deployment.db().pool, project.id, limit, offset)
            .await?;

    let has_more = offset + (tasks.len() as i64) < total;

    Ok(ResponseJson(ApiResponse::success(PaginatedGanttTasks {
        tasks,
        total,
        has_more,
    })))
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
    let stream = deployment.events().stream_gantt_raw(project_id).await?;
    forward_stream_to_ws(socket, stream).await
}

pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let project_gantt =
        Router::new()
            .route("/gantt", get(get_gantt_data))
            .layer(from_fn_with_state(
                deployment.clone(),
                load_project_middleware,
            ));

    Router::new()
        .nest("/projects/{project_id}", project_gantt)
        .route(
            "/projects/{project_id}/gantt/stream/ws",
            get(stream_gantt_ws),
        )
}
