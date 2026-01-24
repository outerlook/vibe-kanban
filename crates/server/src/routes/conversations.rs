use std::str::FromStr;

use axum::{
    Extension, Json, Router, extract::State, http::StatusCode, middleware::from_fn_with_state,
    response::Json as ResponseJson, routing::get,
};
use db::models::{
    conversation_message::{ConversationMessage, ConversationMessagesPage},
    conversation_session::{
        ConversationSession, ConversationSessionStatus, UpdateConversationSession,
    },
    execution_process::ExecutionProcess,
};
use deployment::Deployment;
use executors::{
    actions::{
        ExecutorAction, ExecutorActionType, coding_agent_follow_up::CodingAgentFollowUpRequest,
        coding_agent_initial::CodingAgentInitialRequest,
    },
    executors::BaseCodingAgent,
    profile::{ExecutorConfigs, ExecutorProfileId},
};
use serde::{Deserialize, Serialize};
use services::services::{
    container::ContainerService,
    conversation::{ConversationService, ConversationWithMessages, SendMessageResponse},
};
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
    pub executor_profile_id: Option<ExecutorProfileId>,
}

#[derive(Debug, Serialize, TS)]
pub struct CreateConversationResponse {
    pub session: ConversationSession,
    pub initial_message: db::models::conversation_message::ConversationMessage,
    pub execution_process_id: Uuid,
}

#[derive(Debug, Deserialize, TS)]
pub struct UpdateConversationRequest {
    pub title: Option<String>,
    pub status: Option<ConversationSessionStatus>,
}

#[derive(Debug, Deserialize, TS)]
pub struct SendMessageRequest {
    pub content: String,
    pub variant: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GetMessagesQuery {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
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
    // Determine executor profile: use provided or default to CLAUDE_CODE
    let executor_profile_id = payload
        .executor_profile_id
        .clone()
        .unwrap_or_else(|| ExecutorProfileId::new(BaseCodingAgent::ClaudeCode));

    // Validate the executor profile exists
    if ExecutorConfigs::get_cached()
        .get_coding_agent(&executor_profile_id)
        .is_none()
    {
        return Err(ApiError::BadRequest(format!(
            "Invalid executor profile: {}",
            executor_profile_id
        )));
    }

    // Store executor name in session for future messages
    let executor_name = Some(executor_profile_id.executor.to_string());

    let (session, initial_message) = ConversationService::create_conversation(
        &deployment.db().pool,
        project_id,
        payload.title,
        payload.initial_message.clone(),
        executor_name,
    )
    .await?;

    // Build ExecutorAction for initial conversation
    let action_type = ExecutorActionType::CodingAgentInitialRequest(CodingAgentInitialRequest {
        prompt: payload.initial_message,
        executor_profile_id,
        working_dir: None,
    });
    let executor_action = ExecutorAction::new(action_type, None);

    // Start conversation execution
    let execution_process = deployment
        .container()
        .start_conversation_execution(&session, &executor_action)
        .await?;

    Ok(ResponseJson(ApiResponse::success(
        CreateConversationResponse {
            session,
            initial_message,
            execution_process_id: execution_process.id,
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

    Ok(ResponseJson(ApiResponse::success(
        conversation_with_messages,
    )))
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

/// Send a message to a conversation and start agent execution.
/// Creates a user ConversationMessage, then starts an ExecutionProcess with run_reason=DisposableConversation.
pub async fn send_message(
    Extension(conversation): Extension<ConversationSession>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<SendMessageRequest>,
) -> Result<ResponseJson<ApiResponse<SendMessageResponse>>, ApiError> {
    let pool = &deployment.db().pool;

    // Create user message
    let user_message =
        ConversationService::add_user_message(pool, conversation.id, payload.content.clone())
            .await?;

    // Get the executor from the conversation session, or use a default
    let executor_name = conversation
        .executor
        .clone()
        .unwrap_or("CLAUDE_CODE".to_string());

    // Parse executor name to BaseCodingAgent
    let normalized_executor = executor_name.replace('-', "_").to_ascii_uppercase();
    let base_executor = BaseCodingAgent::from_str(&normalized_executor)
        .map_err(|_| ApiError::BadRequest(format!("Unknown executor: {}", executor_name)))?;

    // Build executor profile
    let executor_profile_id = ExecutorProfileId {
        executor: base_executor,
        variant: payload.variant.clone(),
    };

    // Validate the executor profile exists
    if ExecutorConfigs::get_cached()
        .get_coding_agent(&executor_profile_id)
        .is_none()
    {
        return Err(ApiError::BadRequest(format!(
            "Invalid executor profile: {}",
            executor_profile_id
        )));
    }

    // Get conversation history for context
    let conversation_history =
        ConversationService::get_conversation_history_for_prompt(pool, conversation.id).await?;

    // Check if we have a previous agent session to continue
    let latest_agent_session_id =
        ConversationService::get_latest_agent_session_id(pool, conversation.id).await?;

    // Build the prompt with conversation history
    let prompt = if conversation_history.is_empty() {
        payload.content.clone()
    } else {
        format!(
            "Previous conversation:\n{}\n\nUser: {}",
            conversation_history, payload.content
        )
    };

    // Build ExecutorAction
    let action_type = if let Some(agent_session_id) = latest_agent_session_id {
        ExecutorActionType::CodingAgentFollowUpRequest(CodingAgentFollowUpRequest {
            prompt,
            session_id: agent_session_id,
            executor_profile_id: executor_profile_id.clone(),
            working_dir: None,
        })
    } else {
        ExecutorActionType::CodingAgentInitialRequest(CodingAgentInitialRequest {
            prompt,
            executor_profile_id: executor_profile_id.clone(),
            working_dir: None,
        })
    };

    let executor_action = ExecutorAction::new(action_type, None);

    // Start conversation execution
    let execution_process = deployment
        .container()
        .start_conversation_execution(&conversation, &executor_action)
        .await?;

    Ok(ResponseJson(ApiResponse::success(SendMessageResponse {
        user_message,
        execution_process_id: execution_process.id,
    })))
}

/// Get paginated messages in a conversation
pub async fn get_messages(
    Extension(conversation): Extension<ConversationSession>,
    State(deployment): State<DeploymentImpl>,
    axum::extract::Query(query): axum::extract::Query<GetMessagesQuery>,
) -> Result<ResponseJson<ApiResponse<ConversationMessagesPage>>, ApiError> {
    let page = ConversationMessage::find_paginated_by_conversation_session_id(
        &deployment.db().pool,
        conversation.id,
        query.cursor.as_deref(),
        query.limit,
    )
    .await?;

    Ok(ResponseJson(ApiResponse::success(page)))
}

/// Get execution processes for a conversation
pub async fn get_executions(
    Extension(conversation): Extension<ConversationSession>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<ExecutionProcess>>>, ApiError> {
    let executions =
        ExecutionProcess::find_by_conversation_session_id(&deployment.db().pool, conversation.id, false)
            .await?;

    Ok(ResponseJson(ApiResponse::success(executions)))
}

pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let conversation_actions = Router::new()
        .route(
            "/",
            get(get_conversation)
                .patch(update_conversation)
                .delete(delete_conversation),
        )
        .route("/messages", get(get_messages).post(send_message))
        .route("/executions", get(get_executions))
        .layer(from_fn_with_state(
            deployment.clone(),
            load_conversation_middleware,
        ));

    let project_conversations =
        Router::new().route("/", get(list_conversations).post(create_conversation));

    Router::new()
        .nest(
            "/projects/{project_id}/conversations",
            project_conversations,
        )
        .nest("/conversations/{conversation_id}", conversation_actions)
}
