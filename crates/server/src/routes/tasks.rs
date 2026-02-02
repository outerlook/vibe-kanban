use std::path::PathBuf;

use anyhow;
use axum::{
    Extension, Json, Router,
    extract::{
        Query, State,
        ws::{WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    middleware::from_fn_with_state,
    response::{IntoResponse, Json as ResponseJson},
    routing::{delete, get, post, put},
};
use db::models::{
    image::TaskImage,
    project::{Project, ProjectError},
    repo::Repo,
    task::{CreateTask, Task, TaskOrderBy, TaskStatus, TaskWithAttemptStatus, UpdateTask},
    task_group::TaskGroup,
    workspace::{CreateWorkspace, Workspace},
    workspace_repo::{CreateWorkspaceRepo, WorkspaceRepo},
};
use deployment::Deployment;
use executors::profile::ExecutorProfileId;
use serde::{Deserialize, Serialize};
use services::services::{
    container::{ContainerService, StartWorkspaceResult},
    share::ShareError,
    workspace_manager::WorkspaceManager,
};
use sqlx::Error as SqlxError;
use ts_rs::TS;
use utils::{api::oauth::LoginStatus, response::ApiResponse};
use uuid::Uuid;

use crate::{
    DeploymentImpl, error::ApiError, middleware::load_task_middleware,
    routes::{task_attempts::WorkspaceRepoInput, ws_helpers::forward_stream_to_ws},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct ListTasksQuery {
    pub project_id: Uuid,
    pub query: Option<String>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
    pub status: Option<TaskStatus>,
    pub order_by: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedTasks {
    pub tasks: Vec<TaskWithAttemptStatus>,
    pub total: i64,
    pub has_more: bool,
}

/// Request for semantic task search
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SearchTasksRequest {
    pub project_id: Uuid,
    pub query: String,
    pub status: Option<TaskStatus>,
    pub limit: Option<i32>,
    /// Use hybrid search (vector + FTS). Defaults to true.
    pub hybrid: Option<bool>,
}

/// A task match with similarity score
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskMatchWithScore {
    #[serde(flatten)]
    #[ts(flatten)]
    pub task: TaskWithAttemptStatus,
    pub similarity_score: f64,
}

/// Response for semantic task search
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SearchTasksResponse {
    pub matches: Vec<TaskMatchWithScore>,
    pub count: usize,
    /// The search method used: "hybrid", "vector", or "keyword"
    pub search_method: String,
}

pub async fn get_tasks(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ListTasksQuery>,
) -> Result<ResponseJson<ApiResponse<PaginatedTasks>>, ApiError> {
    const DEFAULT_LIMIT: i32 = 50;
    const MAX_LIMIT: i32 = 200;
    const DEFAULT_OFFSET: i32 = 0;

    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(0, MAX_LIMIT) as i64;
    let offset = query.offset.unwrap_or(DEFAULT_OFFSET).max(0) as i64;

    let order_by = match query.order_by.as_deref() {
        None => TaskOrderBy::default(),
        Some("created_at_asc") => TaskOrderBy::CreatedAtAsc,
        Some("created_at_desc") => TaskOrderBy::CreatedAtDesc,
        Some("updated_at_asc") => TaskOrderBy::UpdatedAtAsc,
        Some("updated_at_desc") => TaskOrderBy::UpdatedAtDesc,
        Some(invalid) => {
            return Err(ApiError::BadRequest(format!(
                "Invalid order_by value '{}'. Valid values are: created_at_asc, created_at_desc, updated_at_asc, updated_at_desc",
                invalid
            )));
        }
    };

    let (tasks, total) = Task::find_paginated_by_project_id_with_attempt_status(
        &deployment.db().pool,
        query.project_id,
        query.query,
        query.status,
        order_by,
        limit,
        offset,
    )
    .await?;

    let has_more = offset + (tasks.len() as i64) < total;

    Ok(ResponseJson(ApiResponse::success(PaginatedTasks {
        tasks,
        total,
        has_more,
    })))
}

/// Search tasks using semantic search (hybrid vector + FTS or FTS-only fallback)
pub async fn search_tasks(
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<SearchTasksRequest>,
) -> Result<ResponseJson<ApiResponse<SearchTasksResponse>>, ApiError> {
    const DEFAULT_LIMIT: i32 = 10;
    const MAX_LIMIT: i32 = 50;

    // Validate query is not empty
    if request.query.trim().is_empty() {
        return Err(ApiError::BadRequest("Query cannot be empty".to_string()));
    }

    // Validate project exists
    let project = Project::find_by_id(&deployment.db().pool, request.project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {} not found", request.project_id)))?;

    let limit = request.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT) as i64;
    let use_hybrid = request.hybrid.unwrap_or(true);

    // Try hybrid search first if requested
    if use_hybrid {
        match deployment.embedding().embed_text(&request.query).await {
            Ok(query_embedding) => {
                let results = Task::search_hybrid(
                    &deployment.db().pool,
                    project.id,
                    &query_embedding,
                    &request.query,
                    request.status.clone(),
                    limit,
                )
                .await?;

                let matches: Vec<TaskMatchWithScore> = results
                    .into_iter()
                    .map(|(task, score)| TaskMatchWithScore {
                        task,
                        similarity_score: score,
                    })
                    .collect();

                let count = matches.len();
                return Ok(ResponseJson(ApiResponse::success(SearchTasksResponse {
                    matches,
                    count,
                    search_method: "hybrid".to_string(),
                })));
            }
            Err(e) => {
                tracing::warn!(
                    "Embedding generation failed, falling back to FTS-only: {}",
                    e
                );
                // Fall through to FTS-only search
            }
        }
    }

    // FTS-only fallback
    let results = Task::search_fts(
        &deployment.db().pool,
        project.id,
        &request.query,
        request.status.clone(),
        limit,
    )
    .await?;

    let matches: Vec<TaskMatchWithScore> = results
        .into_iter()
        .map(|(task, score)| TaskMatchWithScore {
            task,
            similarity_score: score,
        })
        .collect();

    let count = matches.len();
    Ok(ResponseJson(ApiResponse::success(SearchTasksResponse {
        matches,
        count,
        search_method: "keyword".to_string(),
    })))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskStreamQuery {
    pub project_id: Uuid,
    pub include_snapshot: Option<bool>,
}

pub async fn stream_tasks_ws(
    ws: WebSocketUpgrade,
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<TaskStreamQuery>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        let include_snapshot = query.include_snapshot.unwrap_or(true);
        if let Err(e) =
            handle_tasks_ws(socket, deployment, query.project_id, include_snapshot).await
        {
            tracing::warn!("tasks WS closed: {}", e);
        }
    })
}

async fn handle_tasks_ws(
    socket: WebSocket,
    deployment: DeploymentImpl,
    project_id: Uuid,
    include_snapshot: bool,
) -> anyhow::Result<()> {
    let stream = deployment
        .events()
        .stream_tasks_raw(
            project_id,
            include_snapshot,
            deployment.operation_status().clone(),
            deployment.hook_execution_store().clone(),
        )
        .await?;

    forward_stream_to_ws(socket, stream).await
}

pub async fn get_task(
    Extension(task): Extension<Task>,
    State(_deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Task>>, ApiError> {
    Ok(ResponseJson(ApiResponse::success(task)))
}

/// Validates that the provided task_group_id belongs to the specified project.
/// Returns an error if the group doesn't exist or belongs to a different project.
async fn validate_task_group_id(
    pool: &sqlx::SqlitePool,
    task_group_id: Uuid,
    project_id: Uuid,
) -> Result<(), ApiError> {
    let group = TaskGroup::find_by_id(pool, task_group_id)
        .await?
        .ok_or_else(|| ApiError::BadRequest(format!("Task group {} not found", task_group_id)))?;

    if group.project_id != project_id {
        return Err(ApiError::BadRequest(
            "Task group belongs to a different project".to_string(),
        ));
    }
    Ok(())
}

pub async fn create_task(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateTask>,
) -> Result<ResponseJson<ApiResponse<Task>>, ApiError> {
    let id = Uuid::new_v4();

    tracing::debug!(
        "Creating task '{}' in project {}",
        payload.title,
        payload.project_id
    );

    // Validate task_group_id if provided
    if let Some(task_group_id) = payload.task_group_id {
        validate_task_group_id(&deployment.db().pool, task_group_id, payload.project_id).await?;
    }

    let task = Task::create(&deployment.db().pool, &payload, id).await?;

    if let Some(image_ids) = &payload.image_ids {
        TaskImage::associate_many_dedup(&deployment.db().pool, task.id, image_ids).await?;
    }

    deployment
        .track_if_analytics_allowed(
            "task_created",
            serde_json::json!({
            "task_id": task.id.to_string(),
            "project_id": payload.project_id,
            "has_description": task.description.is_some(),
            "has_images": payload.image_ids.is_some(),
            }),
        )
        .await;

    Ok(ResponseJson(ApiResponse::success(task)))
}

#[derive(Debug, Deserialize, TS)]
pub struct CreateAndStartTaskRequest {
    pub task: CreateTask,
    pub executor_profile_id: ExecutorProfileId,
    pub repos: Vec<WorkspaceRepoInput>,
}

pub async fn create_task_and_start(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateAndStartTaskRequest>,
) -> Result<ResponseJson<ApiResponse<TaskWithAttemptStatus>>, ApiError> {
    if payload.repos.is_empty() {
        return Err(ApiError::BadRequest(
            "At least one repository is required".to_string(),
        ));
    }

    let pool = &deployment.db().pool;

    let task_id = Uuid::new_v4();
    let task = Task::create(pool, &payload.task, task_id).await?;

    if let Some(image_ids) = &payload.task.image_ids {
        TaskImage::associate_many_dedup(pool, task.id, image_ids).await?;
    }

    deployment
        .track_if_analytics_allowed(
            "task_created",
            serde_json::json!({
                "task_id": task.id.to_string(),
                "project_id": task.project_id,
                "has_description": task.description.is_some(),
                "has_images": payload.task.image_ids.is_some(),
            }),
        )
        .await;

    let project = Project::find_by_id(pool, task.project_id)
        .await?
        .ok_or(ProjectError::ProjectNotFound)?;

    let attempt_id = Uuid::new_v4();
    let git_branch_name = deployment
        .container()
        .git_branch_from_workspace(&attempt_id, &task.title)
        .await;

    let agent_working_dir = project
        .default_agent_working_dir
        .as_ref()
        .filter(|dir: &&String| !dir.is_empty())
        .cloned();

    let workspace = Workspace::create(
        pool,
        &CreateWorkspace {
            branch: git_branch_name,
            agent_working_dir,
        },
        attempt_id,
        task.id,
    )
    .await?;

    let workspace_repos: Vec<CreateWorkspaceRepo> = payload
        .repos
        .iter()
        .map(|r| CreateWorkspaceRepo {
            repo_id: r.repo_id,
            target_branch: r.target_branch.clone(),
        })
        .collect();
    WorkspaceRepo::create_many(&deployment.db().pool, workspace.id, &workspace_repos).await?;

    let is_attempt_running = match deployment
        .container()
        .start_workspace(&workspace, payload.executor_profile_id.clone())
        .await
    {
        Ok(StartWorkspaceResult::Started(_)) => true,
        Ok(StartWorkspaceResult::Queued(_)) => {
            tracing::info!("Task attempt queued for workspace {}", workspace.id);
            false
        }
        Err(err) => {
            tracing::error!("Failed to start task attempt: {}", err);
            false
        }
    };
    deployment
        .track_if_analytics_allowed(
            "task_attempt_started",
            serde_json::json!({
                "task_id": task.id.to_string(),
                "executor": &payload.executor_profile_id.executor,
                "variant": &payload.executor_profile_id.variant,
                "workspace_id": workspace.id.to_string(),
            }),
        )
        .await;

    let mut task = Task::find_by_id(pool, task.id)
        .await?
        .ok_or(ApiError::Database(SqlxError::RowNotFound))?;

    // Update task fields with current attempt state for the response
    // (materialized columns may not be updated yet)
    task.has_in_progress_attempt = is_attempt_running;
    task.last_attempt_failed = false;
    task.is_blocked = false;
    task.is_queued = !is_attempt_running;
    task.last_executor = payload.executor_profile_id.executor.to_string();

    tracing::info!("Started attempt for task {}", task.id);
    Ok(ResponseJson(ApiResponse::success(
        TaskWithAttemptStatus::from_task(task),
    )))
}

pub async fn update_task(
    Extension(existing_task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,

    Json(payload): Json<UpdateTask>,
) -> Result<ResponseJson<ApiResponse<Task>>, ApiError> {
    ensure_shared_task_auth(&existing_task, &deployment).await?;

    // Validate task_group_id if a new value is provided
    if let Some(task_group_id) = payload.task_group_id {
        validate_task_group_id(
            &deployment.db().pool,
            task_group_id,
            existing_task.project_id,
        )
        .await?;
    }

    // Use existing values if not provided in update
    let title = payload.title.unwrap_or(existing_task.title);
    let description = match payload.description {
        Some(s) if s.trim().is_empty() => None, // Empty string = clear description
        Some(s) => Some(s),                     // Non-empty string = update description
        None => existing_task.description,      // Field omitted = keep existing
    };
    let status = payload.status.unwrap_or(existing_task.status);
    let parent_workspace_id = payload
        .parent_workspace_id
        .or(existing_task.parent_workspace_id);
    let task_group_id = payload.task_group_id.or(existing_task.task_group_id);

    let task = Task::update(
        &deployment.db().pool,
        existing_task.id,
        existing_task.project_id,
        title,
        description,
        status,
        parent_workspace_id,
        task_group_id,
    )
    .await?;

    if let Some(image_ids) = &payload.image_ids {
        TaskImage::delete_by_task_id(&deployment.db().pool, task.id).await?;
        TaskImage::associate_many_dedup(&deployment.db().pool, task.id, image_ids).await?;
    }

    // If task has been shared, broadcast update
    if task.shared_task_id.is_some() {
        let Ok(publisher) = deployment.share_publisher() else {
            return Err(ShareError::MissingConfig("share publisher unavailable").into());
        };
        publisher.update_shared_task(&task).await?;
    }

    Ok(ResponseJson(ApiResponse::success(task)))
}

async fn ensure_shared_task_auth(
    existing_task: &Task,
    deployment: &local_deployment::LocalDeployment,
) -> Result<(), ApiError> {
    if existing_task.shared_task_id.is_some() {
        match deployment.get_login_status().await {
            LoginStatus::LoggedIn { .. } => return Ok(()),
            LoginStatus::LoggedOut => {
                return Err(ShareError::MissingAuth.into());
            }
        }
    }
    Ok(())
}

pub async fn delete_task(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
) -> Result<(StatusCode, ResponseJson<ApiResponse<()>>), ApiError> {
    ensure_shared_task_auth(&task, &deployment).await?;

    // Validate no running execution processes
    if deployment
        .container()
        .has_running_processes(task.id)
        .await?
    {
        return Err(ApiError::Conflict("Task has running execution processes. Please wait for them to complete or stop them first.".to_string()));
    }

    let pool = &deployment.db().pool;

    // Gather task attempts data needed for background cleanup
    let attempts = Workspace::fetch_all(pool, Some(task.id))
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch task attempts for task {}: {}", task.id, e);
            ApiError::Workspace(e)
        })?;

    let repositories = WorkspaceRepo::find_unique_repos_for_task(pool, task.id).await?;

    // Collect workspace directories that need cleanup
    let workspace_dirs: Vec<PathBuf> = attempts
        .iter()
        .filter_map(|attempt| attempt.container_ref.as_ref().map(PathBuf::from))
        .collect();

    if let Some(shared_task_id) = task.shared_task_id {
        let Ok(publisher) = deployment.share_publisher() else {
            return Err(ShareError::MissingConfig("share publisher unavailable").into());
        };
        publisher.delete_shared_task(shared_task_id).await?;
    }

    // Use a transaction to ensure atomicity: either all operations succeed or all are rolled back
    let mut tx = pool.begin().await?;

    // Nullify parent_workspace_id for all child tasks before deletion
    // This breaks parent-child relationships to avoid foreign key constraint violations
    let mut total_children_affected = 0u64;
    for attempt in &attempts {
        let children_affected =
            Task::nullify_children_by_workspace_id(&mut *tx, attempt.id).await?;
        total_children_affected += children_affected;
    }

    // Delete task from database (FK CASCADE will handle task_attempts and task_dependencies)
    // Note: is_blocked of dependent tasks is updated automatically via database trigger
    let rows_affected = Task::delete(&mut *tx, task.id).await?;

    if rows_affected == 0 {
        return Err(ApiError::Database(SqlxError::RowNotFound));
    }

    // Commit the transaction - if this fails, all changes are rolled back
    tx.commit().await?;

    if total_children_affected > 0 {
        tracing::info!(
            "Nullified {} child task references before deleting task {}",
            total_children_affected,
            task.id
        );
    }

    deployment
        .track_if_analytics_allowed(
            "task_deleted",
            serde_json::json!({
                "task_id": task.id.to_string(),
                "project_id": task.project_id.to_string(),
                "attempt_count": attempts.len(),
            }),
        )
        .await;

    let task_id = task.id;
    let pool = pool.clone();
    tokio::spawn(async move {
        tracing::info!(
            "Starting background cleanup for task {} ({} workspaces, {} repos)",
            task_id,
            workspace_dirs.len(),
            repositories.len()
        );

        for workspace_dir in &workspace_dirs {
            if let Err(e) = WorkspaceManager::cleanup_workspace(workspace_dir, &repositories).await
            {
                tracing::error!(
                    "Background workspace cleanup failed for task {} at {}: {}",
                    task_id,
                    workspace_dir.display(),
                    e
                );
            }
        }

        match Repo::delete_orphaned(&pool).await {
            Ok(count) if count > 0 => {
                tracing::info!("Deleted {} orphaned repo records", count);
            }
            Err(e) => {
                tracing::error!("Failed to delete orphaned repos: {}", e);
            }
            _ => {}
        }

        tracing::info!("Background cleanup completed for task {}", task_id);
    });

    // Return 202 Accepted to indicate deletion was scheduled
    Ok((StatusCode::ACCEPTED, ResponseJson(ApiResponse::success(()))))
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct ShareTaskResponse {
    pub shared_task_id: Uuid,
}

pub async fn share_task(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<ShareTaskResponse>>, ApiError> {
    let Ok(publisher) = deployment.share_publisher() else {
        return Err(ShareError::MissingConfig("share publisher unavailable").into());
    };
    let profile = deployment
        .auth_context()
        .cached_profile()
        .await
        .ok_or(ShareError::MissingAuth)?;
    let shared_task_id = publisher.share_task(task.id, profile.user_id).await?;

    let props = serde_json::json!({
        "task_id": task.id,
        "shared_task_id": shared_task_id,
    });
    deployment
        .track_if_analytics_allowed("start_sharing_task", props)
        .await;

    Ok(ResponseJson(ApiResponse::success(ShareTaskResponse {
        shared_task_id,
    })))
}

pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let task_actions_router = Router::new()
        .route("/", put(update_task))
        .route("/", delete(delete_task))
        .route("/share", post(share_task));

    let task_id_router = Router::new()
        .route("/", get(get_task))
        .merge(task_actions_router)
        .layer(from_fn_with_state(deployment.clone(), load_task_middleware));

    let inner = Router::new()
        .route("/", get(get_tasks).post(create_task))
        .route("/search", post(search_tasks))
        .route("/stream/ws", get(stream_tasks_ws))
        .route("/create-and-start", post(create_task_and_start))
        .nest("/{task_id}", task_id_router);

    // mount under /projects/:project_id/tasks
    Router::new().nest("/tasks", inner)
}
