use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use db::models::agent_feedback::{AgentFeedback, CreateAgentFeedback};
use deployment::Deployment;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

/// Response DTO that parses feedback_json into a JSON object
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct FeedbackResponse {
    pub id: Uuid,
    pub task_id: Uuid,
    pub workspace_id: Uuid,
    pub execution_process_id: Uuid,
    pub feedback: Option<serde_json::Value>,
    pub collected_at: DateTime<Utc>,
}

impl From<AgentFeedback> for FeedbackResponse {
    fn from(f: AgentFeedback) -> Self {
        let feedback = f
            .feedback_json
            .as_ref()
            .and_then(|json_str| serde_json::from_str(json_str).ok());

        FeedbackResponse {
            id: f.id,
            task_id: f.task_id,
            workspace_id: f.workspace_id,
            execution_process_id: f.execution_process_id,
            feedback,
            collected_at: f.collected_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RecentFeedbackQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    10
}

/// POST /api/feedback - Persist externally collected feedback for an execution.
pub async fn create_feedback(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateAgentFeedback>,
) -> Result<Json<ApiResponse<FeedbackResponse>>, ApiError> {
    let feedback =
        AgentFeedback::create(&deployment.db().pool, &payload, Uuid::new_v4()).await?;
    Ok(Json(ApiResponse::success(feedback.into())))
}

/// GET /api/feedback/task/:task_id - Returns all feedback for a task
pub async fn get_feedback_by_task(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<FeedbackResponse>>>, ApiError> {
    let feedback_list = AgentFeedback::find_by_task_id(&deployment.db().pool, task_id).await?;
    let response: Vec<FeedbackResponse> = feedback_list.into_iter().map(Into::into).collect();
    Ok(Json(ApiResponse::success(response)))
}

/// GET /api/feedback/workspace/:workspace_id - Returns all feedback for a workspace
pub async fn get_feedback_by_workspace(
    State(deployment): State<DeploymentImpl>,
    Path(workspace_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<FeedbackResponse>>>, ApiError> {
    let feedback_list =
        AgentFeedback::find_by_workspace_id(&deployment.db().pool, workspace_id).await?;
    let response: Vec<FeedbackResponse> = feedback_list.into_iter().map(Into::into).collect();
    Ok(Json(ApiResponse::success(response)))
}

/// GET /api/feedback/recent?limit=N - Returns N most recent feedback entries
pub async fn get_recent_feedback(
    State(deployment): State<DeploymentImpl>,
    Query(params): Query<RecentFeedbackQuery>,
) -> Result<Json<ApiResponse<Vec<FeedbackResponse>>>, ApiError> {
    let feedback_list = AgentFeedback::find_recent(&deployment.db().pool, params.limit).await?;
    let response: Vec<FeedbackResponse> = feedback_list.into_iter().map(Into::into).collect();
    Ok(Json(ApiResponse::success(response)))
}

pub fn router(_deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let inner = Router::new()
        .route("/", post(create_feedback))
        .route("/task/{task_id}", get(get_feedback_by_task))
        .route("/workspace/{workspace_id}", get(get_feedback_by_workspace))
        .route("/recent", get(get_recent_feedback));

    Router::new().nest("/feedback", inner)
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use db::models::{
        execution_process::{
            CreateExecutionProcess, ExecutionProcess, ExecutionProcessRunReason,
        },
        project::{CreateProject, Project},
        session::{CreateSession, Session},
        task::{CreateTask, Task, TaskStatus},
        workspace::{CreateWorkspace, Workspace},
    };
    use executors::{
        actions::{
            ExecutorAction, ExecutorActionType,
            coding_agent_initial::CodingAgentInitialRequest,
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
                status: Some(TaskStatus::Done),
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
                branch: "feature/feedback".to_string(),
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

    async fn create_execution(
        deployment: &DeploymentImpl,
        session_id: Uuid,
    ) -> ExecutionProcess {
        ExecutionProcess::create(
            &deployment.db().pool,
            &CreateExecutionProcess {
                session_id,
                executor_action: ExecutorAction::new(
                    ExecutorActionType::CodingAgentInitialRequest(
                        CodingAgentInitialRequest {
                            prompt: "collect feedback".to_string(),
                            executor_profile_id: ExecutorProfileId::new(
                                BaseCodingAgent::ClaudeCode,
                            ),
                            working_dir: None,
                        },
                    ),
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
    async fn create_feedback_persists_external_payload() {
        let _lock = crate::TEST_DB_LOCK.lock().unwrap();
        let deployment = LocalDeployment::new().await.unwrap();
        let project = create_project(&deployment, "feedback-route").await;
        let task = create_task(&deployment, project.id, "Collect feedback").await;
        let workspace = create_workspace(&deployment, task.id).await;
        let session = create_session(&deployment, workspace.id).await;
        let execution = create_execution(&deployment, session.id).await;
        let app = router(&deployment).with_state(deployment.clone());

        let payload = serde_json::json!({
            "execution_process_id": execution.id,
            "task_id": task.id,
            "workspace_id": workspace.id,
            "feedback_json": r#"{"summary":"Ship it"}"#,
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/feedback")
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
        assert_eq!(data["task_id"], task.id.to_string());
        assert_eq!(data["workspace_id"], workspace.id.to_string());
        assert_eq!(data["feedback"], serde_json::json!({ "summary": "Ship it" }));

        let persisted = AgentFeedback::find_by_execution_process_id(
            &deployment.db().pool,
            execution.id,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(persisted.task_id, task.id);
        assert_eq!(
            persisted.feedback_json,
            Some(r#"{"summary":"Ship it"}"#.to_string())
        );
    }
}
