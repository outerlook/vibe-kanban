use axum::{
    Extension, Json, Router, extract::State, middleware::from_fn_with_state,
    response::Json as ResponseJson, routing::get,
};
use db::models::{scratch::DraftFollowUpData, session::Session};
use deployment::Deployment;
use serde::Deserialize;
use services::services::{
    container::ContainerService,
    domain_events::{
        DomainEvent, DomainEventEntityIds, FollowUpQueueKind, FollowUpScope,
        FollowUpTransitionState,
    },
    queued_message::QueueStatus,
};
use ts_rs::TS;
use utils::response::ApiResponse;

use crate::{DeploymentImpl, error::ApiError, middleware::load_session_middleware};

/// Request body for queueing a follow-up message
#[derive(Debug, Deserialize, TS)]
pub struct QueueMessageRequest {
    pub message: String,
    pub variant: Option<String>,
}

/// Queue a follow-up message to be executed when the current execution finishes
pub async fn queue_message(
    Extension(session): Extension<Session>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<QueueMessageRequest>,
) -> Result<ResponseJson<ApiResponse<QueueStatus>>, ApiError> {
    let data = DraftFollowUpData {
        message: payload.message,
        variant: payload.variant,
    };

    let queued = deployment
        .queued_message_service()
        .queue_message(session.id, data);

    deployment
        .track_if_analytics_allowed(
            "follow_up_queued",
            serde_json::json!({
                "session_id": session.id.to_string(),
                "workspace_id": session.workspace_id.to_string(),
            }),
        )
        .await;

    if let Some(dispatcher) = deployment.container().event_dispatch_callback() {
        dispatcher(DomainEvent::FollowUpTransition {
            state: FollowUpTransitionState::Queued,
            scope: FollowUpScope::TaskSession,
            queue_kind: Some(FollowUpQueueKind::AfterCurrentExecution),
            execution_process_id: None,
            entity_ids: DomainEventEntityIds {
                workspace_id: Some(session.workspace_id),
                session_id: Some(session.id),
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
    Extension(session): Extension<Session>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<QueueStatus>>, ApiError> {
    deployment
        .queued_message_service()
        .cancel_queued(session.id);

    deployment
        .track_if_analytics_allowed(
            "follow_up_queue_cancelled",
            serde_json::json!({
                "session_id": session.id.to_string(),
                "workspace_id": session.workspace_id.to_string(),
            }),
        )
        .await;

    if let Some(dispatcher) = deployment.container().event_dispatch_callback() {
        dispatcher(DomainEvent::FollowUpTransition {
            state: FollowUpTransitionState::Cancelled,
            scope: FollowUpScope::TaskSession,
            queue_kind: Some(FollowUpQueueKind::AfterCurrentExecution),
            execution_process_id: None,
            entity_ids: DomainEventEntityIds {
                workspace_id: Some(session.workspace_id),
                session_id: Some(session.id),
                ..DomainEventEntityIds::default()
            },
            occurred_at: chrono::Utc::now(),
        })
        .await;
    }

    Ok(ResponseJson(ApiResponse::success(QueueStatus::Empty)))
}

/// Get the current queue status for a session's workspace
pub async fn get_queue_status(
    Extension(session): Extension<Session>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<QueueStatus>>, ApiError> {
    let status = deployment.queued_message_service().get_status(session.id);

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
            load_session_middleware,
        ))
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use axum::{Extension, Json, extract::State};
    use db::models::{
        project::{CreateProject, Project},
        session::{CreateSession, Session},
        task::{CreateTask, Task, TaskStatus},
        workspace::{CreateWorkspace, Workspace},
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

    async fn create_task(deployment: &DeploymentImpl, project_id: Uuid, title: &str) -> Task {
        Task::create(
            &deployment.db().pool,
            &CreateTask {
                project_id,
                title: title.to_string(),
                description: None,
                status: Some(TaskStatus::Todo),
                parent_workspace_id: None,
                image_ids: None,
                shared_task_id: None,
                task_group_id: None,
            },
            Uuid::new_v4(),
        )
        .await
        .unwrap()
    }

    async fn create_session_for_task(
        deployment: &DeploymentImpl,
        task_id: Uuid,
        branch: &str,
    ) -> Session {
        let workspace = Workspace::create(
            &deployment.db().pool,
            &CreateWorkspace {
                branch: branch.to_string(),
                agent_working_dir: None,
            },
            Uuid::new_v4(),
            task_id,
        )
        .await
        .unwrap();

        Session::create(
            &deployment.db().pool,
            &CreateSession {
                executor: Some("CLAUDE_CODE".to_string()),
            },
            Uuid::new_v4(),
            workspace.id,
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
    async fn session_queue_routes_emit_queue_and_cancel_transitions() {
        let _lock = crate::TEST_DB_LOCK.lock().unwrap();
        let publisher = RecordingOrchestrationEventPublisher::default();
        let publisher_handle: OrchestrationEventPublisherHandle = Arc::new(publisher.clone());
        let deployment = LocalDeployment::new_with_orchestration_event_publisher(publisher_handle)
            .await
            .unwrap();
        let project = create_project(&deployment, "session-queue-events").await;
        let task = create_task(&deployment, project.id, "Queue me").await;
        let session = create_session_for_task(&deployment, task.id, "session-queue").await;

        let queued = queue_message(
            Extension(session.clone()),
            State(deployment.clone()),
            Json(QueueMessageRequest {
                message: "follow up later".to_string(),
                variant: Some("sonnet".to_string()),
            }),
        )
        .await
        .unwrap()
        .0
        .into_data()
        .unwrap();
        assert!(matches!(queued, QueueStatus::Queued { .. }));

        let cancelled = cancel_queued_message(Extension(session), State(deployment.clone()))
            .await
            .unwrap()
            .0
            .into_data()
            .unwrap();
        assert!(matches!(cancelled, QueueStatus::Empty));

        let event_types = wait_for_event_types(&publisher, 2).await;
        assert_eq!(
            event_types
                .iter()
                .filter(|event_type| **event_type == OrchestrationEventType::FollowUpTransition)
                .count(),
            2
        );
    }
}
