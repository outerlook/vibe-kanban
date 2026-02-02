use axum::{
    Extension, Router,
    extract::{
        Query, State,
        ws::{WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    middleware::from_fn_with_state,
    response::{IntoResponse, Json as ResponseJson},
    routing::{get, post},
};
use db::models::notification::{
    CreateNotification, Notification, NotificationStats, UpdateNotification,
};
use deployment::Deployment;
use serde::Deserialize;
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{
    routes::ws_helpers::forward_stream_to_ws, DeploymentImpl, error::ApiError,
    middleware::load_notification_middleware,
};

#[derive(Debug, Deserialize)]
pub struct ListNotificationsQuery {
    pub project_id: Option<Uuid>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct NotificationStatsQuery {
    pub project_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, TS)]
pub struct CreateNotificationRequest {
    pub project_id: Option<Uuid>,
    pub notification_type: db::models::notification::NotificationType,
    pub title: String,
    pub message: String,
    pub metadata: Option<serde_json::Value>,
    pub workspace_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub conversation_session_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, TS)]
pub struct UpdateNotificationRequest {
    pub title: Option<String>,
    pub message: Option<String>,
    pub is_read: Option<bool>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, TS)]
pub struct MarkAllReadRequest {
    pub project_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct NotificationStreamQuery {
    pub project_id: Option<Uuid>,
    #[serde(default)]
    pub include_snapshot: bool,
}

pub async fn list_notifications(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ListNotificationsQuery>,
) -> Result<ResponseJson<ApiResponse<Vec<Notification>>>, ApiError> {
    let notifications = if let Some(project_id) = query.project_id {
        Notification::find_by_project_id(&deployment.db().pool, project_id, query.limit).await?
    } else {
        Notification::find_global(&deployment.db().pool, query.limit).await?
    };
    Ok(ResponseJson(ApiResponse::success(notifications)))
}

pub async fn get_notification(
    Extension(notification): Extension<Notification>,
) -> Result<ResponseJson<ApiResponse<Notification>>, ApiError> {
    Ok(ResponseJson(ApiResponse::success(notification)))
}

pub async fn create_notification(
    State(deployment): State<DeploymentImpl>,
    axum::Json(payload): axum::Json<CreateNotificationRequest>,
) -> Result<ResponseJson<ApiResponse<Notification>>, ApiError> {
    let create_data = CreateNotification {
        project_id: payload.project_id,
        notification_type: payload.notification_type,
        title: payload.title,
        message: payload.message,
        metadata: payload.metadata,
        workspace_id: payload.workspace_id,
        session_id: payload.session_id,
        conversation_session_id: payload.conversation_session_id,
    };

    let notification = Notification::create(&deployment.db().pool, &create_data).await?;
    Ok(ResponseJson(ApiResponse::success(notification)))
}

pub async fn update_notification(
    Extension(existing): Extension<Notification>,
    State(deployment): State<DeploymentImpl>,
    axum::Json(payload): axum::Json<UpdateNotificationRequest>,
) -> Result<ResponseJson<ApiResponse<Notification>>, ApiError> {
    let update = UpdateNotification {
        title: payload.title,
        message: payload.message,
        is_read: payload.is_read,
        metadata: payload.metadata,
    };

    let notification = Notification::update(&deployment.db().pool, existing.id, &update)
        .await?
        .ok_or_else(|| ApiError::NotFound("Notification not found".to_string()))?;

    Ok(ResponseJson(ApiResponse::success(notification)))
}

pub async fn delete_notification(
    Extension(notification): Extension<Notification>,
    State(deployment): State<DeploymentImpl>,
) -> Result<(StatusCode, ResponseJson<ApiResponse<()>>), ApiError> {
    let rows_affected = Notification::delete(&deployment.db().pool, notification.id).await?;

    if rows_affected == 0 {
        return Err(ApiError::NotFound("Notification not found".to_string()));
    }

    Ok((StatusCode::OK, ResponseJson(ApiResponse::success(()))))
}

pub async fn mark_all_read(
    State(deployment): State<DeploymentImpl>,
    axum::Json(payload): axum::Json<MarkAllReadRequest>,
) -> Result<ResponseJson<ApiResponse<u64>>, ApiError> {
    let updated_count =
        Notification::mark_all_read(&deployment.db().pool, payload.project_id).await?;
    Ok(ResponseJson(ApiResponse::success(updated_count)))
}

pub async fn get_stats(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<NotificationStatsQuery>,
) -> Result<ResponseJson<ApiResponse<NotificationStats>>, ApiError> {
    let stats = Notification::get_stats(&deployment.db().pool, query.project_id).await?;
    Ok(ResponseJson(ApiResponse::success(stats)))
}

pub async fn stream_notifications_ws(
    ws: WebSocketUpgrade,
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<NotificationStreamQuery>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        if let Err(e) =
            handle_notifications_ws(socket, deployment, query.project_id, query.include_snapshot)
                .await
        {
            tracing::warn!("notifications WS closed: {}", e);
        }
    })
}

async fn handle_notifications_ws(
    socket: WebSocket,
    deployment: DeploymentImpl,
    project_id: Option<Uuid>,
    include_snapshot: bool,
) -> anyhow::Result<()> {
    let stream = deployment
        .events()
        .stream_notifications_raw(project_id, include_snapshot)
        .await?;
    forward_stream_to_ws(socket, stream).await
}

pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let notification_actions = Router::new()
        .route(
            "/",
            get(get_notification)
                .patch(update_notification)
                .delete(delete_notification),
        )
        .layer(from_fn_with_state(
            deployment.clone(),
            load_notification_middleware,
        ));

    let inner = Router::new()
        .route("/", get(list_notifications).post(create_notification))
        .route("/mark-all-read", post(mark_all_read))
        .route("/stats", get(get_stats))
        .route("/stream/ws", get(stream_notifications_ws))
        .nest("/{notification_id}", notification_actions);

    Router::new().nest("/notifications", inner)
}
