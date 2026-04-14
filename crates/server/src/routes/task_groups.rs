use axum::{
    Extension, Json, Router,
    extract::State,
    http::StatusCode,
    middleware::from_fn_with_state,
    response::Json as ResponseJson,
    routing::{get, post},
};
use db::models::{
    task_group::{MergeError, TaskGroup, TaskGroupWithStats, UpdateTaskGroup},
    workspace::Workspace,
};
use deployment::Deployment;
use serde::{Deserialize, Serialize};
use services::services::{
    container::ContainerService,
    domain_events::{DomainEvent, TaskGroupTransitionAction},
};
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use super::projects::MergeQueueCountResponse;
use crate::{DeploymentImpl, error::ApiError, middleware::load_task_group_middleware};

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
    )
    .await?;

    deployment
        .container()
        .dispatch_event(DomainEvent::TaskGroupTransition {
            action: TaskGroupTransitionAction::Created,
            project_id: task_group.project_id,
            task_group_id: Some(task_group.id),
            previous_task_group_id: None,
            task_ids: Vec::new(),
            occurred_at: task_group.created_at,
        })
        .await;

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

    deployment
        .container()
        .dispatch_event(DomainEvent::TaskGroupTransition {
            action: TaskGroupTransitionAction::Updated,
            project_id: task_group.project_id,
            task_group_id: Some(task_group.id),
            previous_task_group_id: None,
            task_ids: Vec::new(),
            occurred_at: task_group.updated_at,
        })
        .await;

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

    deployment
        .container()
        .dispatch_event(DomainEvent::TaskGroupTransition {
            action: TaskGroupTransitionAction::Deleted,
            project_id: task_group.project_id,
            task_group_id: Some(task_group.id),
            previous_task_group_id: None,
            task_ids: Vec::new(),
            occurred_at: chrono::Utc::now(),
        })
        .await;

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

    deployment
        .container()
        .dispatch_event(DomainEvent::TaskGroupTransition {
            action: TaskGroupTransitionAction::AssignmentChanged,
            project_id: task_group.project_id,
            task_group_id: Some(task_group.id),
            previous_task_group_id: None,
            task_ids: payload.task_ids.clone(),
            occurred_at: chrono::Utc::now(),
        })
        .await;

    Ok(ResponseJson(ApiResponse::success(
        BulkAssignTasksResponse { updated_count },
    )))
}

pub async fn merge_task_group(
    Extension(source_group): Extension<TaskGroup>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<MergeTaskGroupRequest>,
) -> Result<ResponseJson<ApiResponse<TaskGroup>>, ApiError> {
    let moved_task_ids = db::models::task::Task::find_by_project_id_with_attempt_status(
        &deployment.db().pool,
        source_group.project_id,
    )
    .await?
    .into_iter()
    .map(|task| task.task)
    .filter(|task| task.task_group_id == Some(source_group.id))
    .map(|task| task.id)
    .collect::<Vec<_>>();

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

    deployment
        .container()
        .dispatch_event(DomainEvent::TaskGroupTransition {
            action: TaskGroupTransitionAction::Merged,
            project_id: target.project_id,
            task_group_id: Some(target.id),
            previous_task_group_id: Some(source_group.id),
            task_ids: moved_task_ids,
            occurred_at: chrono::Utc::now(),
        })
        .await;

    Ok(ResponseJson(ApiResponse::success(target)))
}

/// GET /api/task-groups/:id/merge-queue-count - Get the number of entries in the merge queue for a task group
pub async fn get_merge_queue_count(
    Extension(task_group): Extension<TaskGroup>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<MergeQueueCountResponse>>, ApiError> {
    let pool = &deployment.db().pool;
    let workspace_ids = Workspace::fetch_ids_by_task_group(pool, task_group.id).await?;
    let count = deployment
        .merge_queue_store()
        .count_by_workspace_ids(&workspace_ids);
    Ok(ResponseJson(ApiResponse::success(
        MergeQueueCountResponse { count },
    )))
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

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use axum::{Extension, Json, extract::State};
    use db::models::{
        project::{CreateProject, Project},
        task::{CreateTask, Task},
    };
    use deployment::Deployment;
    use local_deployment::LocalDeployment;
    use services::services::domain_events::{
        OrchestrationEventPublisherHandle, OrchestrationEventType,
        RecordingOrchestrationEventPublisher,
    };

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
    async fn task_group_routes_emit_transition_and_completion_events() {
        let _lock = crate::TEST_DB_LOCK.lock().unwrap();
        let publisher = RecordingOrchestrationEventPublisher::default();
        let publisher_handle: OrchestrationEventPublisherHandle = Arc::new(publisher.clone());
        let deployment =
            LocalDeployment::new_with_orchestration_event_publisher(publisher_handle)
                .await
                .unwrap();
        let project = create_project(&deployment, "task-group-events").await;

        let group = create_task_group(
            State(deployment.clone()),
            Json(CreateTaskGroupRequest {
                project_id: project.id,
                name: "Group A".to_string(),
                description: None,
                base_branch: None,
            }),
        )
        .await
        .unwrap()
        .0
        .into_data()
        .unwrap();

        let task = Task::create(
            &deployment.db().pool,
            &CreateTask {
                project_id: project.id,
                title: "Done task".to_string(),
                description: None,
                status: Some(db::models::task::TaskStatus::Done),
                parent_workspace_id: None,
                image_ids: None,
                shared_task_id: None,
                task_group_id: None,
            },
            Uuid::new_v4(),
        )
        .await
        .unwrap();

        let _ = bulk_assign_tasks(
            Extension(group.clone()),
            State(deployment.clone()),
            Json(BulkAssignTasksRequest {
                task_ids: vec![task.id],
            }),
        )
        .await
        .unwrap();

        let assignment_event_types = wait_for_event_types(&publisher, 3).await;
        assert!(assignment_event_types.contains(&OrchestrationEventType::TaskGroupTransition));
        assert!(assignment_event_types.contains(&OrchestrationEventType::TaskGroupCompleted));

        let group = update_task_group(
            Extension(group.clone()),
            State(deployment.clone()),
            Json(UpdateTaskGroupRequest {
                name: Some("Group A+".to_string()),
                description: None,
                base_branch: None,
            }),
        )
        .await
        .unwrap()
        .0
        .into_data()
        .unwrap();

        let _ = delete_task_group(Extension(group), State(deployment.clone()))
            .await
            .unwrap();

        let event_types = wait_for_event_types(&publisher, 4).await;
        assert!(event_types.contains(&OrchestrationEventType::TaskGroupTransition));
    }
}
