use axum::{
    Extension, Json, Router, extract::State, middleware::from_fn_with_state,
    response::Json as ResponseJson, routing::get,
};
use db::models::{conversation_session::ConversationSession, scratch::DraftFollowUpData};
use deployment::Deployment;
use serde::Deserialize;
use services::services::{container::ContainerService, queued_message::QueueStatus};
use services::services::domain_events::{
    DomainEvent, DomainEventEntityIds, FollowUpQueueKind, FollowUpScope,
    FollowUpTransitionState,
};
use ts_rs::TS;
use utils::response::ApiResponse;

use crate::{DeploymentImpl, error::ApiError, middleware::load_conversation_middleware};

/// Request body for queueing a follow-up message
#[derive(Debug, Deserialize, TS)]
pub struct QueueMessageRequest {
    pub message: String,
    pub variant: Option<String>,
}

/// Queue a follow-up message to be executed when the current execution finishes
pub async fn queue_message(
    Extension(conversation): Extension<ConversationSession>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<QueueMessageRequest>,
) -> Result<ResponseJson<ApiResponse<QueueStatus>>, ApiError> {
    let data = DraftFollowUpData {
        message: payload.message,
        variant: payload.variant,
    };

    let queued = deployment
        .queued_message_service()
        .queue_message(conversation.id, data);

    deployment
        .track_if_analytics_allowed(
            "conversation_message_queued",
            serde_json::json!({
                "conversation_id": conversation.id.to_string(),
                "project_id": conversation.project_id.to_string(),
            }),
        )
        .await;

    if let Some(dispatcher) = deployment.container().event_dispatch_callback() {
        dispatcher(DomainEvent::FollowUpTransition {
            state: FollowUpTransitionState::Queued,
            scope: FollowUpScope::Conversation,
            queue_kind: Some(FollowUpQueueKind::AfterCurrentExecution),
            execution_process_id: None,
            entity_ids: DomainEventEntityIds {
                session_id: Some(conversation.id),
                ..DomainEventEntityIds::default()
            },
            occurred_at: queued.queued_at,
        })
        .await;
    }

    Ok(ResponseJson(ApiResponse::success(QueueStatus::Queued {
        message: queued,
    })))
}

/// Cancel a queued follow-up message
pub async fn cancel_queued_message(
    Extension(conversation): Extension<ConversationSession>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<QueueStatus>>, ApiError> {
    deployment
        .queued_message_service()
        .cancel_queued(conversation.id);

    deployment
        .track_if_analytics_allowed(
            "conversation_queue_cancelled",
            serde_json::json!({
                "conversation_id": conversation.id.to_string(),
                "project_id": conversation.project_id.to_string(),
            }),
        )
        .await;

    if let Some(dispatcher) = deployment.container().event_dispatch_callback() {
        dispatcher(DomainEvent::FollowUpTransition {
            state: FollowUpTransitionState::Cancelled,
            scope: FollowUpScope::Conversation,
            queue_kind: Some(FollowUpQueueKind::AfterCurrentExecution),
            execution_process_id: None,
            entity_ids: DomainEventEntityIds {
                session_id: Some(conversation.id),
                ..DomainEventEntityIds::default()
            },
            occurred_at: chrono::Utc::now(),
        })
        .await;
    }

    Ok(ResponseJson(ApiResponse::success(QueueStatus::Empty)))
}

/// Get the current queue status for a conversation
pub async fn get_queue_status(
    Extension(conversation): Extension<ConversationSession>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<QueueStatus>>, ApiError> {
    let status = deployment
        .queued_message_service()
        .get_status(conversation.id);

    Ok(ResponseJson(ApiResponse::success(status)))
}

pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    Router::new()
        .route(
            "/",
            get(get_queue_status)
                .post(queue_message)
                .delete(cancel_queued_message),
        )
        .layer(from_fn_with_state(
            deployment.clone(),
            load_conversation_middleware,
        ))
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use axum::{Extension, Json, extract::State};
    use db::models::project::{CreateProject, Project};
    use deployment::Deployment;
    use local_deployment::LocalDeployment;
    use services::services::{
        conversation::ConversationService,
        domain_events::{
            OrchestrationEventPublisherHandle, OrchestrationEventType,
            RecordingOrchestrationEventPublisher,
        },
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
    async fn queue_routes_emit_follow_up_transitions() {
        let _lock = crate::TEST_DB_LOCK.lock().unwrap();
        let publisher = RecordingOrchestrationEventPublisher::default();
        let publisher_handle: OrchestrationEventPublisherHandle = Arc::new(publisher.clone());
        let deployment =
            LocalDeployment::new_with_orchestration_event_publisher(publisher_handle)
                .await
                .unwrap();
        let project = create_project(&deployment, "conversation-queue-events").await;
        let (conversation, _) = ConversationService::create_conversation_with_events(
            &deployment.db().pool,
            project.id,
            "Chat".to_string(),
            "hello".to_string(),
            Some("CLAUDE_CODE".to_string()),
            None,
            None,
            deployment.container().event_dispatch_callback(),
        )
        .await
        .unwrap();

        let _ = queue_message(
            Extension(conversation.clone()),
            State(deployment.clone()),
            Json(QueueMessageRequest {
                message: "follow up".to_string(),
                variant: None,
            }),
        )
        .await
        .unwrap();

        let _ = cancel_queued_message(Extension(conversation), State(deployment.clone()))
            .await
            .unwrap();

        let event_types = wait_for_event_types(&publisher, 3).await;
        assert!(event_types.contains(&OrchestrationEventType::ConversationMessageAdded));
        assert!(event_types.contains(&OrchestrationEventType::FollowUpTransition));
    }
}
