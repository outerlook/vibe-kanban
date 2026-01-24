use axum::{
    Extension, Json, Router,
    extract::State,
    http::StatusCode,
    middleware::from_fn_with_state,
    response::Json as ResponseJson,
    routing::{get, post},
};
use db::models::{
    merge_queue::MergeQueue,
    task_group::{MergeError, TaskGroup, TaskGroupWithStats, UpdateTaskGroup},
};
use deployment::Deployment;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError, middleware::load_task_group_middleware};

use super::projects::MergeQueueCountResponse;

#[derive(Debug, Deserialize)]
pub struct ListTaskGroupsQuery {
    pub project_id: Uuid,
}

#[derive(Debug, Deserialize, TS)]
pub struct CreateTaskGroupRequest {
    pub project_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub base_branch: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
pub struct UpdateTaskGroupRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub base_branch: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, TS)]
pub struct BulkAssignTasksRequest {
    pub task_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize, TS)]
pub struct BulkAssignTasksResponse {
    pub updated_count: u64,
}

#[derive(Debug, Deserialize, TS)]
pub struct MergeTaskGroupRequest {
    pub target_group_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct GetTaskGroupStatsQuery {
    pub project_id: Uuid,
}

pub async fn get_task_group_stats(
    State(deployment): State<DeploymentImpl>,
    axum::extract::Query(query): axum::extract::Query<GetTaskGroupStatsQuery>,
) -> Result<ResponseJson<ApiResponse<Vec<TaskGroupWithStats>>>, ApiError> {
    let stats = TaskGroup::get_stats_for_project(&deployment.db().pool, query.project_id).await?;
    Ok(ResponseJson(ApiResponse::success(stats)))
}

pub async fn list_task_groups(
    State(deployment): State<DeploymentImpl>,
    axum::extract::Query(query): axum::extract::Query<ListTaskGroupsQuery>,
) -> Result<ResponseJson<ApiResponse<Vec<TaskGroup>>>, ApiError> {
    let groups = TaskGroup::find_by_project_id(&deployment.db().pool, query.project_id).await?;
    Ok(ResponseJson(ApiResponse::success(groups)))
}

pub async fn get_task_group(
    Extension(task_group): Extension<TaskGroup>,
) -> Result<ResponseJson<ApiResponse<TaskGroup>>, ApiError> {
    Ok(ResponseJson(ApiResponse::success(task_group)))
}

pub async fn create_task_group(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateTaskGroupRequest>,
) -> Result<ResponseJson<ApiResponse<TaskGroup>>, ApiError> {
    let task_group = TaskGroup::create(
        &deployment.db().pool,
        payload.project_id,
        payload.name,
        payload.description,
        payload.base_branch,
        payload.description,
    )
    .await?;

    Ok(ResponseJson(ApiResponse::success(task_group)))
}

pub async fn update_task_group(
    Extension(existing): Extension<TaskGroup>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<UpdateTaskGroupRequest>,
) -> Result<ResponseJson<ApiResponse<TaskGroup>>, ApiError> {
    let update = UpdateTaskGroup {
        name: payload.name,
        description: payload.description,
        base_branch: payload.base_branch,
    };

    let task_group = TaskGroup::update(&deployment.db().pool, existing.id, &update)
        .await?
        .ok_or_else(|| ApiError::NotFound("Task group not found".to_string()))?;

    Ok(ResponseJson(ApiResponse::success(task_group)))
}

pub async fn delete_task_group(
    Extension(task_group): Extension<TaskGroup>,
    State(deployment): State<DeploymentImpl>,
) -> Result<(StatusCode, ResponseJson<ApiResponse<()>>), ApiError> {
    let rows_affected = TaskGroup::delete(&deployment.db().pool, task_group.id).await?;

    if rows_affected == 0 {
        return Err(ApiError::NotFound("Task group not found".to_string()));
    }

    Ok((StatusCode::OK, ResponseJson(ApiResponse::success(()))))
}

pub async fn bulk_assign_tasks(
    Extension(task_group): Extension<TaskGroup>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<BulkAssignTasksRequest>,
) -> Result<ResponseJson<ApiResponse<BulkAssignTasksResponse>>, ApiError> {
    if payload.task_ids.is_empty() {
        return Ok(ResponseJson(ApiResponse::success(
            BulkAssignTasksResponse { updated_count: 0 },
        )));
    }

    let updated_count = TaskGroup::bulk_assign_tasks(
        &deployment.db().pool,
        task_group.id,
        task_group.project_id,
        &payload.task_ids,
    )
    .await?;

    Ok(ResponseJson(ApiResponse::success(
        BulkAssignTasksResponse { updated_count },
    )))
}

pub async fn merge_task_group(
    Extension(source_group): Extension<TaskGroup>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<MergeTaskGroupRequest>,
) -> Result<ResponseJson<ApiResponse<TaskGroup>>, ApiError> {
    let target = TaskGroup::merge_into(
        &deployment.db().pool,
        source_group.id,
        payload.target_group_id,
    )
    .await
    .map_err(|err| match err {
        MergeError::SameGroup => {
            ApiError::BadRequest("Cannot merge a group into itself".to_string())
        }
        MergeError::SourceNotFound => ApiError::NotFound("Source task group not found".to_string()),
        MergeError::TargetNotFound => ApiError::NotFound("Target task group not found".to_string()),
        MergeError::DifferentProjects => {
            ApiError::BadRequest("Groups belong to different projects".to_string())
        }
        MergeError::Database(db_err) => ApiError::Database(db_err),
    })?;

    Ok(ResponseJson(ApiResponse::success(target)))
}

/// GET /api/task-groups/:id/merge-queue-count - Get the number of entries in the merge queue for a task group
pub async fn get_merge_queue_count(
    Extension(task_group): Extension<TaskGroup>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<MergeQueueCountResponse>>, ApiError> {
    let pool = &deployment.db().pool;
    let count = MergeQueue::count_by_task_group(pool, task_group.id).await?;
    Ok(ResponseJson(ApiResponse::success(MergeQueueCountResponse {
        count,
    })))
}

pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let task_group_actions = Router::new()
        .route(
            "/",
            get(get_task_group)
                .put(update_task_group)
                .delete(delete_task_group),
        )
        .route("/assign", post(bulk_assign_tasks))
        .route("/merge", post(merge_task_group))
        .route("/merge-queue-count", get(get_merge_queue_count))
        .layer(from_fn_with_state(
            deployment.clone(),
            load_task_group_middleware,
        ));

    let inner = Router::new()
        .route("/", get(list_task_groups).post(create_task_group))
        .route("/stats", get(get_task_group_stats))
        .nest("/{group_id}", task_group_actions);

    Router::new().nest("/task-groups", inner)
}
