use axum::{
    Extension, Router,
    extract::State,
    middleware::from_fn_with_state,
    response::Json as ResponseJson,
    routing::get,
};
use db::models::{gantt::GanttTask, project::Project};
use deployment::Deployment;
use utils::response::ApiResponse;

use crate::{DeploymentImpl, error::ApiError, middleware::load_project_middleware};

pub async fn get_gantt_data(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<GanttTask>>>, ApiError> {
    let tasks = GanttTask::find_by_project_id(&deployment.db().pool, project.id).await?;
    Ok(ResponseJson(ApiResponse::success(tasks)))
}

pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let project_gantt = Router::new()
        .route("/gantt", get(get_gantt_data))
        .layer(from_fn_with_state(deployment.clone(), load_project_middleware));

    Router::new().nest("/projects/{project_id}", project_gantt)
}
