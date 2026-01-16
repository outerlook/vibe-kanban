use axum::{
    Extension, Json, Router,
    extract::State,
    http::StatusCode,
    middleware::from_fn_with_state,
    response::Json as ResponseJson,
    routing::get,
};
use db::models::conversation_session::{
    ConversationSession, ConversationSessionStatus, UpdateConversationSession,
};
use deployment::Deployment;
use serde::{Deserialize, Serialize};
use services::services::conversation::{ConversationService, ConversationWithMessages};
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError, middleware::load_conversation_middleware};

#[derive(Debug, Deserialize)]
pub struct ListConversationsQuery {
    pub project_id: Uuid,
}

#[derive(Debug, Deserialize, TS)]
pub struct CreateConversationRequest {
    pub title: String,
    pub initial_message: String,
    pub executor: Option<String>,
}

#[derive(Debug, Serialize, TS)]
pub struct CreateConversationResponse {
    pub session: ConversationSession,
    pub initial_message: db::models::conversation_message::ConversationMessage,
}

#[derive(Debug, Deserialize, TS)]
pub struct UpdateConversationRequest {
    pub title: Option<String>,
    pub status: Option<ConversationSessionStatus>,
}

pub async fn list_conversations(
    State(deployment): State<DeploymentImpl>,
    axum::extract::Query(query): axum::extract::Query<ListConversationsQuery>,
) -> Result<ResponseJson<ApiResponse<Vec<ConversationSession>>>, ApiError> {
    let sessions =
        ConversationSession::find_by_project_id(&deployment.db().pool, query.project_id).await?;
    Ok(ResponseJson(ApiResponse::success(sessions)))
}

pub async fn create_conversation(
    State(deployment): State<DeploymentImpl>,
    axum::extract::Path(project_id): axum::extract::Path<Uuid>,
    Json(payload): Json<CreateConversationRequest>,
) -> Result<ResponseJson<ApiResponse<CreateConversationResponse>>, ApiError> {
    let (session, initial_message) = ConversationService::create_conversation(
        &deployment.db().pool,
        project_id,
        payload.title,
        payload.initial_message,
        payload.executor,
    )
    .await?;

    Ok(ResponseJson(ApiResponse::success(
        CreateConversationResponse {
            session,
            initial_message,
        },
    )))
}

pub async fn get_conversation(
    State(deployment): State<DeploymentImpl>,
    Extension(conversation): Extension<ConversationSession>,
) -> Result<ResponseJson<ApiResponse<ConversationWithMessages>>, ApiError> {
    let conversation_with_messages =
        ConversationService::get_conversation_with_messages(&deployment.db().pool, conversation.id)
            .await?;

    Ok(ResponseJson(ApiResponse::success(conversation_with_messages)))
}

pub async fn update_conversation(
    Extension(existing): Extension<ConversationSession>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<UpdateConversationRequest>,
) -> Result<ResponseJson<ApiResponse<ConversationSession>>, ApiError> {
    let update = UpdateConversationSession {
        title: payload.title,
        status: payload.status,
        executor: None,
    };

    let conversation = ConversationSession::update(&deployment.db().pool, existing.id, &update)
        .await?
        .ok_or_else(|| ApiError::NotFound("Conversation not found".to_string()))?;

    Ok(ResponseJson(ApiResponse::success(conversation)))
}

pub async fn delete_conversation(
    Extension(conversation): Extension<ConversationSession>,
    State(deployment): State<DeploymentImpl>,
) -> Result<(StatusCode, ResponseJson<ApiResponse<()>>), ApiError> {
    let rows_affected = ConversationSession::delete(&deployment.db().pool, conversation.id).await?;

    if rows_affected == 0 {
        return Err(ApiError::NotFound("Conversation not found".to_string()));
    }

    Ok((StatusCode::OK, ResponseJson(ApiResponse::success(()))))
}

pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let conversation_actions = Router::new()
        .route(
            "/",
            get(get_conversation)
                .patch(update_conversation)
                .delete(delete_conversation),
        )
        .layer(from_fn_with_state(
            deployment.clone(),
            load_conversation_middleware,
        ));

    let project_conversations = Router::new()
        .route("/", get(list_conversations).post(create_conversation));

    Router::new()
        .nest("/projects/{project_id}/conversations", project_conversations)
        .nest("/conversations/{conversation_id}", conversation_actions)
}
