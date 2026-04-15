use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use db::models::{
    review_attention::{CreateReviewAttention, ReviewAttention},
    task::Task,
};
use deployment::Deployment;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

/// GET /api/review-attention/task/:task_id - Returns the latest review attention for a task
pub async fn get_review_attention_by_task(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Option<ReviewAttention>>>, ApiError> {
    let review_attention =
        ReviewAttention::find_latest_by_task_id(&deployment.db().pool, task_id).await?;
    Ok(Json(ApiResponse::success(review_attention)))
}

/// POST /api/review-attention - Persist externally collected review attention state.
pub async fn create_review_attention(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateReviewAttention>,
) -> Result<Json<ApiResponse<ReviewAttention>>, ApiError> {
    let review_attention =
        ReviewAttention::create(&deployment.db().pool, &payload, Uuid::new_v4()).await?;
    Task::update_needs_attention(
        &deployment.db().pool,
        payload.task_id,
        Some(payload.needs_attention),
    )
    .await?;

    Ok(Json(ApiResponse::success(review_attention)))
}

pub fn router(_deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let inner = Router::new()
        .route("/", post(create_review_attention))
        .route("/task/{task_id}", get(get_review_attention_by_task));

    Router::new().nest("/review-attention", inner)
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use db::models::{
        execution_process::{CreateExecutionProcess, ExecutionProcess, ExecutionProcessRunReason},
        project::{CreateProject, Project},
        session::{CreateSession, Session},
        task::{CreateTask, Task, TaskStatus},
        workspace::{CreateWorkspace, Workspace},
    };
    use executors::{
        actions::{
            ExecutorAction, ExecutorActionType, coding_agent_initial::CodingAgentInitialRequest,
        },
        executors::BaseCodingAgent,
        profile::ExecutorProfileId,
    };
    use local_deployment::LocalDeployment;
    use tower::ServiceExt;
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
                description: Some(format!("{} description", title)),
                status: Some(TaskStatus::InReview),
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

    async fn create_workspace(deployment: &DeploymentImpl, task_id: Uuid) -> Workspace {
        Workspace::create(
            &deployment.db().pool,
            &CreateWorkspace {
                branch: "feature/review-attention".to_string(),
                agent_working_dir: Some("src".to_string()),
            },
            Uuid::new_v4(),
            task_id,
        )
        .await
        .unwrap()
    }

    async fn create_session(deployment: &DeploymentImpl, workspace_id: Uuid) -> Session {
        Session::create(
            &deployment.db().pool,
            &CreateSession {
                executor: Some("CLAUDE_CODE".to_string()),
            },
            Uuid::new_v4(),
            workspace_id,
        )
        .await
        .unwrap()
    }

    async fn create_execution(deployment: &DeploymentImpl, session_id: Uuid) -> ExecutionProcess {
        ExecutionProcess::create(
            &deployment.db().pool,
            &CreateExecutionProcess {
                session_id,
                executor_action: ExecutorAction::new(
                    ExecutorActionType::CodingAgentInitialRequest(CodingAgentInitialRequest {
                        prompt: "review attention".to_string(),
                        executor_profile_id: ExecutorProfileId::new(BaseCodingAgent::ClaudeCode),
                        working_dir: None,
                    }),
                    None,
                ),
                run_reason: ExecutionProcessRunReason::CodingAgent,
            },
            Uuid::new_v4(),
            &[],
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn create_review_attention_updates_task_state() {
        let _lock = crate::TEST_DB_LOCK.lock().unwrap();
        let deployment = LocalDeployment::new().await.unwrap();
        let project = create_project(&deployment, "review-attention-route").await;
        let task = create_task(&deployment, project.id, "Review me").await;
        let workspace = create_workspace(&deployment, task.id).await;
        let session = create_session(&deployment, workspace.id).await;
        let execution = create_execution(&deployment, session.id).await;
        let app = router(&deployment).with_state(deployment.clone());

        let payload = serde_json::json!({
            "execution_process_id": execution.id,
            "task_id": task.id,
            "workspace_id": workspace.id,
            "needs_attention": true,
            "reasoning": "Manual review required before merge",
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/review-attention")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let api_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let data = &api_response["data"];

        assert_eq!(data["execution_process_id"], execution.id.to_string());
        assert_eq!(data["needs_attention"], serde_json::json!(true));
        assert_eq!(
            data["reasoning"],
            serde_json::json!("Manual review required before merge")
        );

        let updated_task = Task::find_by_id(&deployment.db().pool, task.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated_task.needs_attention, Some(true));
    }
}
