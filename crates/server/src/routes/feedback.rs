use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use chrono::{DateTime, Utc};
use db::models::agent_feedback::AgentFeedback;
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
        let feedback = f.feedback_json.as_ref().and_then(|json_str| {
            serde_json::from_str(json_str).ok()
        });

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

/// GET /api/feedback/task/:task_id - Returns all feedback for a task
pub async fn get_feedback_by_task(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<FeedbackResponse>>>, ApiError> {
    let feedback_list = AgentFeedback::find_by_task_id(&deployment.db().pool, task_id).await?;
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
        .route("/task/{task_id}", get(get_feedback_by_task))
        .route("/recent", get(get_recent_feedback));

    Router::new().nest("/feedback", inner)
}
