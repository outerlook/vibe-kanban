pub mod queue;

use std::str::FromStr;

use axum::{
    Extension, Json, Router,
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::StatusCode,
    middleware::from_fn_with_state,
    response::Json as ResponseJson,
    routing::{get, post},
};
use db::models::{
    conversation_message::{ConversationMessage, ConversationMessagesPage},
    conversation_session::{
        ConversationSession, ConversationSessionStatus, UpdateConversationSession,
    },
    execution_process::ExecutionProcess,
    image::ConversationImage,
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
    domain_events::{DomainEvent, DomainEventEntityIds, FollowUpScope, FollowUpTransitionState},
    orchestration::{OrchestrationConversationContextDto, OrchestrationService},
};
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{
    DeploymentImpl,
    error::ApiError,
    middleware::load_conversation_middleware,
    routes::images::{ImageResponse, process_image_upload},
};

#[derive(Debug, Deserialize)]
pub struct ListConversationsQuery {
    pub project_id: Uuid,
    pub worktree_path: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
pub struct CreateConversationRequest {
    pub title: String,
    pub initial_message: String,
    pub executor_profile_id: Option<ExecutorProfileId>,
    pub worktree_path: Option<String>,
    pub worktree_branch: Option<String>,
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
    let sessions = ConversationSession::find_by_project_id(
        &deployment.db().pool,
        query.project_id,
        query.worktree_path.as_deref(),
    )
    .await?;
    Ok(ResponseJson(ApiResponse::success(sessions)))
}

pub async fn create_conversation(
    State(deployment): State<DeploymentImpl>,
    axum::extract::Path(project_id): axum::extract::Path<Uuid>,
    Json(payload): Json<CreateConversationRequest>,
) -> Result<ResponseJson<ApiResponse<CreateConversationResponse>>, ApiError> {
    let default_executor_profile = deployment.config().read().await.executor_profile.clone();

    // Determine executor profile: use the provided value or the configured VK default.
    let executor_profile_id = payload
        .executor_profile_id
        .clone()
        .unwrap_or(default_executor_profile);

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

    let (session, initial_message) = ConversationService::create_conversation_with_events(
        &deployment.db().pool,
        project_id,
        payload.title,
        payload.initial_message.clone(),
        executor_name,
        payload.worktree_path.clone(),
        payload.worktree_branch.clone(),
        deployment.container().event_dispatch_callback(),
    )
    .await?;

    // Build ExecutorAction for initial conversation
    let action_type = ExecutorActionType::CodingAgentInitialRequest(CodingAgentInitialRequest {
        prompt: payload.initial_message,
        executor_profile_id,
        working_dir: payload.worktree_path.clone(),
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

pub async fn get_conversation_orchestration_context(
    Extension(conversation): Extension<ConversationSession>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<OrchestrationConversationContextDto>>, ApiError> {
    let context =
        OrchestrationService::build_conversation_context(&deployment.db().pool, conversation)
            .await?;

    Ok(ResponseJson(ApiResponse::success(context)))
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
    let event_dispatcher = deployment.container().event_dispatch_callback();

    // Create user message
    let user_message = ConversationService::add_user_message_with_events(
        pool,
        conversation.id,
        payload.content.clone(),
        event_dispatcher.clone(),
    )
    .await?;

    let default_executor_name = deployment
        .config()
        .read()
        .await
        .executor_profile
        .executor
        .to_string();

    // Get the executor from the conversation session, or use the configured VK default.
    let executor_name = conversation
        .executor
        .clone()
        .unwrap_or(default_executor_name);

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

    // Check if we have a previous agent session to continue
    let latest_agent_session_id =
        ConversationService::get_latest_agent_session_id(pool, conversation.id).await?;

    // Build ExecutorAction
    // Note: For follow-up requests, the agent's --resume flag already loads conversation context,
    // so we only send the new user message, not the entire history.
    let action_type = if let Some(agent_session_id) = latest_agent_session_id {
        ExecutorActionType::CodingAgentFollowUpRequest(CodingAgentFollowUpRequest {
            prompt: payload.content.clone(),
            session_id: agent_session_id,
            executor_profile_id: executor_profile_id.clone(),
            working_dir: None,
        })
    } else {
        ExecutorActionType::CodingAgentInitialRequest(CodingAgentInitialRequest {
            prompt: payload.content.clone(),
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

    if let Some(dispatcher) = event_dispatcher {
        dispatcher(DomainEvent::FollowUpTransition {
            state: FollowUpTransitionState::Started,
            scope: FollowUpScope::Conversation,
            queue_kind: None,
            execution_process_id: Some(execution_process.id),
            entity_ids: DomainEventEntityIds {
                session_id: Some(conversation.id),
                execution_process_id: Some(execution_process.id),
                ..DomainEventEntityIds::default()
            },
            occurred_at: chrono::Utc::now(),
        })
        .await;
    }

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
    let executions = ExecutionProcess::find_by_conversation_session_id(
        &deployment.db().pool,
        conversation.id,
        false,
    )
    .await?;

    Ok(ResponseJson(ApiResponse::success(executions)))
}

/// Upload an image for a conversation session.
/// Images are stored in `.vibe-images/` and can be embedded in markdown messages.
pub async fn upload_conversation_image(
    Path(conversation_id): Path<Uuid>,
    State(deployment): State<DeploymentImpl>,
    multipart: Multipart,
) -> Result<ResponseJson<ApiResponse<ImageResponse>>, ApiError> {
    // Validate the conversation exists
    ConversationSession::find_by_id(&deployment.db().pool, conversation_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Conversation not found".to_string()))?;

    // Reuse the existing image upload processing (no task association needed)
    let image_response = process_image_upload(&deployment, multipart, None).await?;

    // Associate the uploaded image with the conversation
    ConversationImage::associate_many_dedup(
        &deployment.db().pool,
        conversation_id,
        std::slice::from_ref(&image_response.id),
    )
    .await?;

    Ok(ResponseJson(ApiResponse::success(image_response)))
}

pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let conversation_actions = Router::new()
        .route(
            "/",
            get(get_conversation)
                .patch(update_conversation)
                .delete(delete_conversation),
        )
        .route(
            "/orchestration-context",
            get(get_conversation_orchestration_context),
        )
        .route("/messages", get(get_messages).post(send_message))
        .route("/executions", get(get_executions))
        .layer(from_fn_with_state(
            deployment.clone(),
            load_conversation_middleware,
        ));

    // Image upload route - outside middleware layer since we validate conversation manually
    let conversation_images = Router::new().route(
        "/images/upload",
        post(upload_conversation_image).layer(DefaultBodyLimit::max(20 * 1024 * 1024)), // 20MB limit
    );

    let project_conversations =
        Router::new().route("/", get(list_conversations).post(create_conversation));

    Router::new()
        .nest(
            "/projects/{project_id}/conversations",
            project_conversations,
        )
        .nest("/conversations/{conversation_id}", conversation_actions)
        .nest("/conversations/{conversation_id}", conversation_images)
        .nest(
            "/conversations/{conversation_id}/queue",
            queue::router(deployment),
        )
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use axum::{
        Extension, Json,
        extract::{Path as AxumPath, State},
    };
    use db::models::{
        conversation_session::{ConversationSession, CreateConversationSession},
        project::{CreateProject, Project},
    };
    use local_deployment::LocalDeployment;
    use services::services::domain_events::{
        OrchestrationEventPublisherHandle, OrchestrationEventType,
        RecordingOrchestrationEventPublisher,
    };
    use uuid::Uuid;

    use super::*;

    async fn create_project(deployment: &DeploymentImpl, name: &str) -> Project {
        Project::create(
            &deployment.db().pool,
            &CreateProject {
                name: name.to_string(),
                repositories: vec![],
            },
            Uuid::new_v4(),
        )
        .await
        .unwrap()
    }

    async fn wait_for_event_types(
        publisher: &RecordingOrchestrationEventPublisher,
        minimum: usize,
    ) -> Vec<OrchestrationEventType> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            let events = publisher
                .published()
                .into_iter()
                .map(|(_, envelope)| envelope.event_type)
                .collect::<Vec<_>>();
            if events.len() >= minimum || tokio::time::Instant::now() >= deadline {
                return events;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn conversation_routes_emit_message_events_from_authoritative_handlers() {
        let _lock = crate::TEST_DB_LOCK.lock().unwrap();
        let publisher = RecordingOrchestrationEventPublisher::default();
        let publisher_handle: OrchestrationEventPublisherHandle = Arc::new(publisher.clone());
        let deployment = LocalDeployment::new_with_orchestration_event_publisher(publisher_handle)
            .await
            .unwrap();
        let project = create_project(&deployment, "conversation-route-events").await;

        let missing_worktree_path = std::env::temp_dir().join(format!(
            "vk-conversation-missing-worktree-{}",
            Uuid::new_v4()
        ));
        let create_result = create_conversation(
            State(deployment.clone()),
            AxumPath(project.id),
            Json(CreateConversationRequest {
                title: "Conversation".to_string(),
                initial_message: "hello from create".to_string(),
                executor_profile_id: None,
                worktree_path: Some(missing_worktree_path.to_string_lossy().to_string()),
                worktree_branch: None,
            }),
        )
        .await;
        assert!(create_result.is_err());

        let conversation = ConversationSession::create(
            &deployment.db().pool,
            CreateConversationSession {
                project_id: project.id,
                title: "Send message".to_string(),
                executor: Some("not-a-real-executor".to_string()),
                worktree_path: None,
                worktree_branch: None,
            },
        )
        .await
        .unwrap();

        let send_result = send_message(
            Extension(conversation),
            State(deployment.clone()),
            Json(SendMessageRequest {
                content: "hello from send".to_string(),
                variant: None,
            }),
        )
        .await;
        assert!(send_result.is_err());

        let event_types = wait_for_event_types(&publisher, 2).await;
        assert_eq!(
            event_types
                .iter()
                .filter(|event_type| {
                    **event_type == OrchestrationEventType::ConversationMessageAdded
                })
                .count(),
            2
        );
    }
}
