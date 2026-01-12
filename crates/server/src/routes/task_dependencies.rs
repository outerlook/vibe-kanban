use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    middleware::from_fn_with_state,
    response::Json as ResponseJson,
    routing::{delete, get},
};
use db::models::{
    task::Task,
    task_dependency::{TaskDependency, TaskDependencyError},
};
use deployment::Deployment;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError, middleware::load_task_middleware};

const DEFAULT_MAX_DEPTH: u32 = 5;

#[derive(Debug, Deserialize)]
pub struct AddDependencyRequest {
    pub depends_on_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyDirection {
    BlockedBy,
    Blocking,
}

#[derive(Debug, Deserialize)]
pub struct DependencyListQuery {
    pub direction: Option<DependencyDirection>,
}

#[derive(Debug, Deserialize)]
pub struct DependencyTreeQuery {
    pub max_depth: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct TaskDependencyTreeNode {
    pub task: Task,
    pub dependencies: Vec<TaskDependencyTreeNode>,
}

#[derive(Debug, Deserialize)]
pub struct DependencyPath {
    pub task_id: Uuid,
    pub dep_id: Uuid,
}

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub async fn get_dependencies(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<DependencyListQuery>,
) -> Result<ResponseJson<ApiResponse<Vec<Task>>>, ApiError> {
    let dependencies = match query.direction.unwrap_or(DependencyDirection::BlockedBy) {
        DependencyDirection::BlockedBy => {
            TaskDependency::find_blocked_by(&deployment.db().pool, task.id).await?
        }
        DependencyDirection::Blocking => {
            TaskDependency::find_blocking(&deployment.db().pool, task.id).await?
        }
    };
    Ok(ResponseJson(ApiResponse::success(dependencies)))
}

pub async fn add_dependency(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<AddDependencyRequest>,
) -> Result<ResponseJson<ApiResponse<TaskDependency>>, ApiError> {
    if task.id == payload.depends_on_id {
        return Err(ApiError::BadRequest(
            "Task cannot depend on itself".to_string(),
        ));
    }

    let pool = &deployment.db().pool;

    let dependency = TaskDependency::create(pool, task.id, payload.depends_on_id)
        .await
        .map_err(map_dependency_error)?;

    // Auto-inherit group: if the depends_on task has a group and current task doesn't,
    // inherit the group from depends_on
    if task.task_group_id.is_none()
        && let Some(depends_on_task) = Task::find_by_id(pool, payload.depends_on_id).await?
        && let Some(group_id) = depends_on_task.task_group_id
    {
        Task::inherit_group_if_none(pool, task.id, group_id).await?;
    }

    Ok(ResponseJson(ApiResponse::success(dependency)))
}

pub async fn remove_dependency(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
    Path(params): Path<DependencyPath>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let pool = &deployment.db().pool;
    TaskDependency::delete(pool, task.id, params.dep_id)
        .await
        .map_err(map_dependency_error)?;
    Ok(ResponseJson(ApiResponse::success(())))
}

pub async fn get_dependency_tree(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<DependencyTreeQuery>,
) -> Result<ResponseJson<ApiResponse<TaskDependencyTreeNode>>, ApiError> {
    let max_depth = query.max_depth.unwrap_or(DEFAULT_MAX_DEPTH) as usize;
    let mut path = HashSet::new();

    let tree =
        build_dependency_tree(&deployment.db().pool, task, max_depth, &mut path).await?;

    Ok(ResponseJson(ApiResponse::success(tree)))
}

fn build_dependency_tree<'a>(
    pool: &'a SqlitePool,
    task: Task,
    max_depth: usize,
    path: &'a mut HashSet<Uuid>,
) -> BoxFuture<'a, Result<TaskDependencyTreeNode, ApiError>> {
    Box::pin(async move {
        if !path.insert(task.id) {
            return Err(ApiError::BadRequest(
                "Cycle detected in dependency graph".to_string(),
            ));
        }

        let dependencies = if max_depth == 0 {
            Vec::new()
        } else {
            let dependency_tasks = TaskDependency::find_blocked_by(pool, task.id).await?;
            let mut nodes = Vec::with_capacity(dependency_tasks.len());
            for dependency_task in dependency_tasks {
                let node =
                    build_dependency_tree(pool, dependency_task, max_depth - 1, path).await?;
                nodes.push(node);
            }
            nodes
        };

        path.remove(&task.id);

        Ok(TaskDependencyTreeNode { task, dependencies })
    })
}

fn map_dependency_error(error: TaskDependencyError) -> ApiError {
    match error {
        TaskDependencyError::TaskNotFound => {
            ApiError::NotFound("Task not found".to_string())
        }
        TaskDependencyError::DifferentProjects => ApiError::BadRequest(
            "Tasks must belong to the same project".to_string(),
        ),
        TaskDependencyError::CycleDetected => ApiError::BadRequest(
            "Adding this dependency would create a cycle".to_string(),
        ),
        TaskDependencyError::Database(err) => {
            if let Some(db_err) = err.as_database_error()
                && db_err.is_unique_violation()
            {
                return ApiError::Conflict("Dependency already exists".to_string());
            }
            ApiError::Database(err)
        }
    }
}

pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let task_dependencies = Router::new()
        .route("/dependencies", get(get_dependencies).post(add_dependency))
        .route("/dependencies/{dep_id}", delete(remove_dependency))
        .route("/dependency-tree", get(get_dependency_tree))
        .layer(from_fn_with_state(deployment.clone(), load_task_middleware));

    Router::new().nest("/tasks/{task_id}", task_dependencies)
}
