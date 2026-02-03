use axum::{Json, Router, extract::{Path, State}, routing::get};
use db::models::review_attention::ReviewAttention;
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

pub fn router(_deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let inner = Router::new().route("/task/{task_id}", get(get_review_attention_by_task));

    Router::new().nest("/review-attention", inner)
}
