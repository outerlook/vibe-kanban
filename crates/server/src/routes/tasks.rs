use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

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
    workflow_association::{
        UpsertWorkflowAssociation, WorkflowAssociation, WorkflowAssociationResolution,
    },
    workspace::{CreateWorkspace, Workspace},
    workspace_repo::{CreateWorkspaceRepo, WorkspaceRepo},
};
use deployment::Deployment;
use executors::profile::ExecutorProfileId;
use serde::{Deserialize, Serialize};
use services::services::{
    container::{ContainerService, StartWorkspaceResult},
    domain_events::{DomainEvent, TaskGroupTransitionAction, TaskLifecycleAction},
    orchestration::{OrchestrationService, OrchestrationTaskContextDto},
    share::ShareError,
    workspace_manager::WorkspaceManager,
};
use sqlx::Error as SqlxError;
use ts_rs::TS;
use utils::{api::oauth::LoginStatus, response::ApiResponse};
use uuid::Uuid;

use crate::{
    DeploymentImpl,
    error::ApiError,
    middleware::load_task_middleware,
    routes::{task_attempts::WorkspaceRepoInput, ws_helpers::forward_stream_to_ws},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct ListTasksQuery {
    pub project_id: Uuid,
    pub query: Option<String>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
    pub status: Option<TaskStatus>,
    pub task_group_id: Option<Uuid>,
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
    pub task_group_id: Option<Uuid>,
    pub limit: Option<i32>,
    /// Use hybrid search (vector + FTS). Defaults to true.
    pub hybrid: Option<bool>,
}

/// A task match with similarity score
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskMatchWithScore {
    // Keep the search-match payload explicit so this route's camelCase contract
    // does not inherit snake_case field names from the flattened task model.
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub parent_workspace_id: Option<Uuid>,
    pub shared_task_id: Option<Uuid>,
    pub task_group_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub is_blocked: bool,
    pub has_in_progress_attempt: bool,
    pub last_attempt_failed: bool,
    pub is_queued: bool,
    pub last_executor: String,
    pub needs_attention: Option<bool>,
    pub similarity_score: f64,
}

impl TaskMatchWithScore {
    fn from_task(task: TaskWithAttemptStatus, similarity_score: f64) -> Self {
        let task = task.task;

        Self {
            id: task.id,
            project_id: task.project_id,
            title: task.title,
            description: task.description,
            status: task.status,
            parent_workspace_id: task.parent_workspace_id,
            shared_task_id: task.shared_task_id,
            task_group_id: task.task_group_id,
            created_at: task.created_at,
            updated_at: task.updated_at,
            is_blocked: task.is_blocked,
            has_in_progress_attempt: task.has_in_progress_attempt,
            last_attempt_failed: task.last_attempt_failed,
            is_queued: task.is_queued,
            last_executor: task.last_executor,
            needs_attention: task.needs_attention,
            similarity_score,
        }
    }
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

#[derive(Debug, Deserialize, TS)]
pub struct BulkDeleteTasksRequest {
    pub project_id: Uuid,
    pub task_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DeletedTaskSummary {
    pub id: Uuid,
    pub project_id: Uuid,
    pub status: TaskStatus,
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct BulkDeleteTasksResponse {
    pub deleted_tasks: Vec<DeletedTaskSummary>,
}

#[derive(Debug, Deserialize, TS)]
pub struct BulkUpdateTaskStatusRequest {
    pub project_id: Uuid,
    pub task_ids: Vec<Uuid>,
    pub status: TaskStatus,
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct BulkUpdateTaskStatusResponse {
    pub tasks: Vec<Task>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectBulkDeleteTasksRequest {
    pub task_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectBulkUpdateTaskStatusRequest {
    pub task_ids: Vec<Uuid>,
    pub status: TaskStatus,
}

#[derive(Debug, Clone)]
struct ResolvedTaskUpdate {
    title: String,
    description: Option<String>,
    status: TaskStatus,
    parent_workspace_id: Option<Uuid>,
    task_group_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
struct TaskDeletionPlan {
    task: Task,
    attempts: Vec<Workspace>,
    repositories: Vec<Repo>,
    workspace_dirs: Vec<PathBuf>,
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
        query.task_group_id,
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
                    request.task_group_id,
                    limit,
                )
                .await?;

                let matches: Vec<TaskMatchWithScore> = results
                    .into_iter()
                    .map(|(task, score)| TaskMatchWithScore::from_task(task, score))
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
        request.task_group_id,
        limit,
    )
    .await?;

    let matches: Vec<TaskMatchWithScore> = results
        .into_iter()
        .map(|(task, score)| TaskMatchWithScore::from_task(task, score))
        .collect();

    let count = matches.len();
    Ok(ResponseJson(ApiResponse::success(SearchTasksResponse {
        matches,
        count,
        search_method: "keyword".to_string(),
    })))
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::Value;

    use super::*;

    fn sample_task() -> TaskWithAttemptStatus {
        TaskWithAttemptStatus::from_task(Task {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            title: "Fix semantic search contract".to_string(),
            description: Some("Align route and MCP payload naming".to_string()),
            status: TaskStatus::InProgress,
            parent_workspace_id: Some(Uuid::new_v4()),
            shared_task_id: Some(Uuid::new_v4()),
            task_group_id: Some(Uuid::new_v4()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            is_blocked: true,
            has_in_progress_attempt: true,
            last_attempt_failed: false,
            is_queued: false,
            last_executor: "claude-code".to_string(),
            needs_attention: Some(true),
        })
    }

    #[test]
    fn search_task_matches_serialize_flattened_fields_in_camel_case() {
        let response = SearchTasksResponse {
            matches: vec![TaskMatchWithScore::from_task(sample_task(), 0.91)],
            count: 1,
            search_method: "hybrid".to_string(),
        };

        let value = serde_json::to_value(response).unwrap();
        let first_match = &value["matches"][0];

        assert_eq!(value["searchMethod"], Value::String("hybrid".to_string()));
        assert!(value.get("search_method").is_none());

        for key in [
            "projectId",
            "parentWorkspaceId",
            "sharedTaskId",
            "taskGroupId",
            "createdAt",
            "updatedAt",
            "isBlocked",
            "hasInProgressAttempt",
            "lastAttemptFailed",
            "isQueued",
            "lastExecutor",
            "needsAttention",
            "similarityScore",
        ] {
            assert!(first_match.get(key).is_some(), "missing key {key}");
        }

        for key in [
            "project_id",
            "parent_workspace_id",
            "shared_task_id",
            "task_group_id",
            "created_at",
            "updated_at",
            "is_blocked",
            "has_in_progress_attempt",
            "last_attempt_failed",
            "is_queued",
            "last_executor",
            "needs_attention",
            "similarity_score",
        ] {
            assert!(first_match.get(key).is_none(), "unexpected key {key}");
        }
    }
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

pub async fn get_task_workflow_associations(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<WorkflowAssociationResolution>>, ApiError> {
    let resolution = WorkflowAssociation::resolve_for_task(&deployment.db().pool, &task).await?;
    Ok(ResponseJson(ApiResponse::success(resolution)))
}

pub async fn upsert_task_workflow_association(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<UpsertWorkflowAssociation>,
) -> Result<ResponseJson<ApiResponse<WorkflowAssociation>>, ApiError> {
    let association =
        WorkflowAssociation::upsert_for_task(&deployment.db().pool, task.id, &payload).await?;
    Ok(ResponseJson(ApiResponse::success(association)))
}

pub async fn delete_task_workflow_association(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let deleted = WorkflowAssociation::delete_for_task(&deployment.db().pool, task.id).await?;
    if deleted == 0 {
        return Err(ApiError::NotFound(format!(
            "No workflow association found for task {}",
            task.id
        )));
    }

    Ok(ResponseJson(ApiResponse::success(())))
}

pub async fn get_task_orchestration_context(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<OrchestrationTaskContextDto>>, ApiError> {
    let context = OrchestrationService::build_task_context(
        &deployment.db().pool,
        deployment.approvals(),
        deployment.merge_queue_store(),
        task,
    )
    .await?;

    Ok(ResponseJson(ApiResponse::success(context)))
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

    deployment
        .container()
        .dispatch_event(DomainEvent::TaskLifecycle {
            action: TaskLifecycleAction::Created,
            task: task.clone(),
            previous_task_group_id: None,
            occurred_at: task.created_at,
        })
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

    deployment
        .container()
        .dispatch_event(DomainEvent::TaskLifecycle {
            action: TaskLifecycleAction::Created,
            task: task.clone(),
            previous_task_group_id: None,
            occurred_at: task.created_at,
        })
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

    deployment
        .container()
        .dispatch_event(DomainEvent::WorkspaceCreated {
            workspace: workspace.clone(),
        })
        .await;

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

    let image_ids = payload.image_ids.clone();
    let resolved_update = resolve_task_update(&deployment, &existing_task, &payload).await?;
    let previous_status = existing_task.status.clone();
    let task_updated = existing_task.title != resolved_update.title
        || existing_task.description != resolved_update.description
        || existing_task.parent_workspace_id != resolved_update.parent_workspace_id
        || existing_task.task_group_id != resolved_update.task_group_id
        || image_ids.is_some();
    let task = persist_task_update(&deployment.db().pool, &existing_task, &resolved_update).await?;

    if let Some(image_ids) = &image_ids {
        TaskImage::delete_by_task_id(&deployment.db().pool, task.id).await?;
        TaskImage::associate_many_dedup(&deployment.db().pool, task.id, image_ids).await?;
    }

    finalize_task_update(&deployment, &task, previous_status).await?;

    if existing_task.task_group_id != task.task_group_id {
        deployment
            .container()
            .dispatch_event(DomainEvent::TaskGroupTransition {
                action: TaskGroupTransitionAction::AssignmentChanged,
                project_id: task.project_id,
                task_group_id: task.task_group_id,
                previous_task_group_id: existing_task.task_group_id,
                task_ids: vec![task.id],
                occurred_at: task.updated_at,
            })
            .await;
    }

    if task_updated {
        deployment
            .container()
            .dispatch_event(DomainEvent::TaskLifecycle {
                action: TaskLifecycleAction::Updated,
                task: task.clone(),
                previous_task_group_id: existing_task.task_group_id,
                occurred_at: task.updated_at,
            })
            .await;
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

async fn ensure_shared_tasks_auth(
    tasks: &[Task],
    deployment: &local_deployment::LocalDeployment,
) -> Result<(), ApiError> {
    if tasks.iter().all(|task| task.shared_task_id.is_none()) {
        return Ok(());
    }

    match deployment.get_login_status().await {
        LoginStatus::LoggedIn { .. } => Ok(()),
        LoginStatus::LoggedOut => Err(ShareError::MissingAuth.into()),
    }
}

async fn resolve_task_update(
    deployment: &DeploymentImpl,
    existing_task: &Task,
    payload: &UpdateTask,
) -> Result<ResolvedTaskUpdate, ApiError> {
    if let Some(task_group_id) = payload.task_group_id {
        validate_task_group_id(
            &deployment.db().pool,
            task_group_id,
            existing_task.project_id,
        )
        .await?;
    }

    Ok(ResolvedTaskUpdate {
        title: payload
            .title
            .clone()
            .unwrap_or_else(|| existing_task.title.clone()),
        description: match &payload.description {
            Some(s) if s.trim().is_empty() => None,
            Some(s) => Some(s.clone()),
            None => existing_task.description.clone(),
        },
        status: payload
            .status
            .clone()
            .unwrap_or_else(|| existing_task.status.clone()),
        parent_workspace_id: payload
            .parent_workspace_id
            .or(existing_task.parent_workspace_id),
        task_group_id: payload.task_group_id.or(existing_task.task_group_id),
    })
}

async fn persist_task_update<'e, E>(
    executor: E,
    existing_task: &Task,
    resolved_update: &ResolvedTaskUpdate,
) -> Result<Task, ApiError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    Ok(Task::update_with_executor(
        executor,
        existing_task.id,
        existing_task.project_id,
        resolved_update.title.clone(),
        resolved_update.description.clone(),
        resolved_update.status.clone(),
        resolved_update.parent_workspace_id,
        resolved_update.task_group_id,
    )
    .await?)
}

async fn finalize_task_update(
    deployment: &DeploymentImpl,
    task: &Task,
    previous_status: TaskStatus,
) -> Result<(), ApiError> {
    if task.status != previous_status {
        deployment
            .container()
            .dispatch_event(DomainEvent::TaskStatusChanged {
                task: task.clone(),
                previous_status,
            })
            .await;
    }

    if task.shared_task_id.is_some() {
        let Ok(publisher) = deployment.share_publisher() else {
            return Err(ShareError::MissingConfig("share publisher unavailable").into());
        };
        publisher.update_shared_task(task).await?;
    }

    Ok(())
}

fn validate_bulk_task_ids(task_ids: &[Uuid]) -> Result<Vec<Uuid>, ApiError> {
    if task_ids.is_empty() {
        return Err(ApiError::BadRequest(
            "taskIds must contain at least one task".to_string(),
        ));
    }

    let mut seen = HashSet::new();
    let mut unique_ids = Vec::with_capacity(task_ids.len());

    for task_id in task_ids {
        if !seen.insert(*task_id) {
            return Err(ApiError::BadRequest("taskIds must be unique".to_string()));
        }
        unique_ids.push(*task_id);
    }

    Ok(unique_ids)
}

async fn load_project_tasks(
    pool: &sqlx::SqlitePool,
    project_id: Uuid,
    task_ids: &[Uuid],
) -> Result<Vec<Task>, ApiError> {
    let tasks = Task::find_by_ids(pool, task_ids).await?;
    let tasks_by_id: HashMap<Uuid, Task> = tasks.into_iter().map(|task| (task.id, task)).collect();

    let mut resolved_tasks = Vec::with_capacity(task_ids.len());
    let mut invalid_task_ids = Vec::new();

    for task_id in task_ids {
        match tasks_by_id.get(task_id) {
            Some(task) if task.project_id == project_id => resolved_tasks.push(task.clone()),
            _ => invalid_task_ids.push(task_id.to_string()),
        }
    }

    if !invalid_task_ids.is_empty() {
        return Err(ApiError::BadRequest(format!(
            "Tasks do not belong to project {}: {}",
            project_id,
            invalid_task_ids.join(", ")
        )));
    }

    Ok(resolved_tasks)
}

async fn build_task_deletion_plan(
    deployment: &DeploymentImpl,
    task: Task,
) -> Result<TaskDeletionPlan, ApiError> {
    if deployment
        .container()
        .has_running_processes(task.id)
        .await?
    {
        return Err(ApiError::Conflict(
            "Task has running execution processes. Please wait for them to complete or stop them first."
                .to_string(),
        ));
    }

    let pool = &deployment.db().pool;
    let attempts = Workspace::fetch_all(pool, Some(task.id))
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch task attempts for task {}: {}", task.id, e);
            ApiError::Workspace(e)
        })?;
    let repositories = WorkspaceRepo::find_unique_repos_for_task(pool, task.id).await?;
    let workspace_dirs = attempts
        .iter()
        .filter_map(|attempt| attempt.container_ref.as_ref().map(PathBuf::from))
        .collect();

    Ok(TaskDeletionPlan {
        task,
        attempts,
        repositories,
        workspace_dirs,
    })
}

async fn publish_task_deletions(
    deployment: &DeploymentImpl,
    deletion_plans: &[TaskDeletionPlan],
) -> Result<(), ApiError> {
    if deletion_plans
        .iter()
        .all(|plan| plan.task.shared_task_id.is_none())
    {
        return Ok(());
    }

    let Ok(publisher) = deployment.share_publisher() else {
        return Err(ShareError::MissingConfig("share publisher unavailable").into());
    };

    for plan in deletion_plans {
        if let Some(shared_task_id) = plan.task.shared_task_id {
            publisher.delete_shared_task(shared_task_id).await?;
        }
    }

    Ok(())
}

async fn execute_task_deletions(
    pool: &sqlx::SqlitePool,
    deletion_plans: &[TaskDeletionPlan],
) -> Result<Vec<DeletedTaskSummary>, ApiError> {
    let mut tx = pool.begin().await?;

    for plan in deletion_plans {
        let mut total_children_affected = 0u64;
        for attempt in &plan.attempts {
            let children_affected =
                Task::nullify_children_by_workspace_id(&mut *tx, attempt.id).await?;
            total_children_affected += children_affected;
        }

        let rows_affected = Task::delete(&mut *tx, plan.task.id).await?;
        if rows_affected == 0 {
            return Err(ApiError::Database(SqlxError::RowNotFound));
        }

        if total_children_affected > 0 {
            tracing::info!(
                "Nullified {} child task references before deleting task {}",
                total_children_affected,
                plan.task.id
            );
        }
    }

    tx.commit().await?;

    Ok(deletion_plans
        .iter()
        .map(|plan| DeletedTaskSummary {
            id: plan.task.id,
            project_id: plan.task.project_id,
            status: plan.task.status.clone(),
        })
        .collect())
}

async fn track_deleted_tasks(deployment: &DeploymentImpl, deletion_plans: &[TaskDeletionPlan]) {
    for plan in deletion_plans {
        deployment
            .track_if_analytics_allowed(
                "task_deleted",
                serde_json::json!({
                    "task_id": plan.task.id.to_string(),
                    "project_id": plan.task.project_id.to_string(),
                    "attempt_count": plan.attempts.len(),
                }),
            )
            .await;
    }
}

fn spawn_task_cleanup(pool: sqlx::SqlitePool, deletion_plans: Vec<TaskDeletionPlan>) {
    tokio::spawn(async move {
        let total_workspaces: usize = deletion_plans
            .iter()
            .map(|plan| plan.workspace_dirs.len())
            .sum();
        let total_repos: usize = deletion_plans
            .iter()
            .map(|plan| plan.repositories.len())
            .sum();

        tracing::info!(
            "Starting background cleanup for {} deleted tasks ({} workspaces, {} repos)",
            deletion_plans.len(),
            total_workspaces,
            total_repos
        );

        for plan in &deletion_plans {
            for workspace_dir in &plan.workspace_dirs {
                if let Err(e) =
                    WorkspaceManager::cleanup_workspace(workspace_dir, &plan.repositories).await
                {
                    tracing::error!(
                        "Background workspace cleanup failed for task {} at {}: {}",
                        plan.task.id,
                        workspace_dir.display(),
                        e
                    );
                }
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

        tracing::info!("Background cleanup completed for deleted tasks");
    });
}

pub async fn delete_task(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
) -> Result<(StatusCode, ResponseJson<ApiResponse<()>>), ApiError> {
    ensure_shared_task_auth(&task, &deployment).await?;

    let deletion_plan = build_task_deletion_plan(&deployment, task).await?;
    publish_task_deletions(&deployment, std::slice::from_ref(&deletion_plan)).await?;
    execute_task_deletions(&deployment.db().pool, std::slice::from_ref(&deletion_plan)).await?;
    deployment
        .container()
        .dispatch_event(DomainEvent::TaskLifecycle {
            action: TaskLifecycleAction::Deleted,
            task: deletion_plan.task.clone(),
            previous_task_group_id: deletion_plan.task.task_group_id,
            occurred_at: chrono::Utc::now(),
        })
        .await;
    track_deleted_tasks(&deployment, std::slice::from_ref(&deletion_plan)).await;
    spawn_task_cleanup(deployment.db().pool.clone(), vec![deletion_plan]);

    // Return 202 Accepted to indicate deletion was scheduled
    Ok((StatusCode::ACCEPTED, ResponseJson(ApiResponse::success(()))))
}

pub async fn bulk_update_task_status(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<BulkUpdateTaskStatusRequest>,
) -> Result<ResponseJson<ApiResponse<BulkUpdateTaskStatusResponse>>, ApiError> {
    let task_ids = validate_bulk_task_ids(&payload.task_ids)?;
    let existing_tasks =
        load_project_tasks(&deployment.db().pool, payload.project_id, &task_ids).await?;
    ensure_shared_tasks_auth(&existing_tasks, &deployment).await?;

    let previous_statuses: HashMap<Uuid, TaskStatus> = existing_tasks
        .iter()
        .map(|task| (task.id, task.status.clone()))
        .collect();

    let mut tx = deployment.db().pool.begin().await?;
    let mut updated_tasks = Vec::with_capacity(existing_tasks.len());

    for task in &existing_tasks {
        let resolved_update = ResolvedTaskUpdate {
            title: task.title.clone(),
            description: task.description.clone(),
            status: payload.status.clone(),
            parent_workspace_id: task.parent_workspace_id,
            task_group_id: task.task_group_id,
        };

        updated_tasks.push(persist_task_update(&mut *tx, task, &resolved_update).await?);
    }

    tx.commit().await?;

    for task in &updated_tasks {
        let previous_status = previous_statuses
            .get(&task.id)
            .cloned()
            .ok_or(ApiError::Database(SqlxError::RowNotFound))?;
        finalize_task_update(&deployment, task, previous_status).await?;
    }

    Ok(ResponseJson(ApiResponse::success(
        BulkUpdateTaskStatusResponse {
            tasks: updated_tasks,
        },
    )))
}

pub async fn bulk_delete_tasks(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<BulkDeleteTasksRequest>,
) -> Result<
    (
        StatusCode,
        ResponseJson<ApiResponse<BulkDeleteTasksResponse>>,
    ),
    ApiError,
> {
    let task_ids = validate_bulk_task_ids(&payload.task_ids)?;
    let tasks = load_project_tasks(&deployment.db().pool, payload.project_id, &task_ids).await?;
    ensure_shared_tasks_auth(&tasks, &deployment).await?;

    let mut deletion_plans = Vec::with_capacity(tasks.len());
    for task in tasks {
        deletion_plans.push(build_task_deletion_plan(&deployment, task).await?);
    }

    publish_task_deletions(&deployment, &deletion_plans).await?;
    let deleted_tasks = execute_task_deletions(&deployment.db().pool, &deletion_plans).await?;
    for plan in &deletion_plans {
        deployment
            .container()
            .dispatch_event(DomainEvent::TaskLifecycle {
                action: TaskLifecycleAction::Deleted,
                task: plan.task.clone(),
                previous_task_group_id: plan.task.task_group_id,
                occurred_at: chrono::Utc::now(),
            })
            .await;
    }
    track_deleted_tasks(&deployment, &deletion_plans).await;
    spawn_task_cleanup(deployment.db().pool.clone(), deletion_plans);

    Ok((
        StatusCode::ACCEPTED,
        ResponseJson(ApiResponse::success(BulkDeleteTasksResponse {
            deleted_tasks,
        })),
    ))
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
        .route("/share", post(share_task))
        .route(
            "/workflow-associations",
            get(get_task_workflow_associations),
        )
        .route(
            "/workflow-association",
            put(upsert_task_workflow_association).delete(delete_task_workflow_association),
        );

    let task_id_router = Router::new()
        .route("/", get(get_task))
        .route(
            "/orchestration-context",
            get(get_task_orchestration_context),
        )
        .merge(task_actions_router)
        .layer(from_fn_with_state(deployment.clone(), load_task_middleware));

    let inner = Router::new()
        .route("/", get(get_tasks).post(create_task))
        .route("/search", post(search_tasks))
        .route("/stream/ws", get(stream_tasks_ws))
        .route("/create-and-start", post(create_task_and_start))
        .route("/bulk-delete", post(bulk_delete_tasks))
        .route("/bulk-update-status", post(bulk_update_task_status))
        .nest("/{task_id}", task_id_router);

    // mount under /projects/:project_id/tasks
    Router::new().nest("/tasks", inner)
}

async fn project_bulk_update_task_status(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<ProjectBulkUpdateTaskStatusRequest>,
) -> Result<ResponseJson<ApiResponse<BulkUpdateTaskStatusResponse>>, ApiError> {
    bulk_update_task_status(
        State(deployment),
        Json(BulkUpdateTaskStatusRequest {
            project_id: project.id,
            task_ids: payload.task_ids,
            status: payload.status,
        }),
    )
    .await
}

async fn project_bulk_delete_tasks(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<ProjectBulkDeleteTasksRequest>,
) -> Result<
    (
        StatusCode,
        ResponseJson<ApiResponse<BulkDeleteTasksResponse>>,
    ),
    ApiError,
> {
    bulk_delete_tasks(
        State(deployment),
        Json(BulkDeleteTasksRequest {
            project_id: project.id,
            task_ids: payload.task_ids,
        }),
    )
    .await
}

pub fn project_router(_deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    Router::new().nest(
        "/tasks",
        Router::new()
            .route("/bulk/status", post(project_bulk_update_task_status))
            .route("/bulk/delete", post(project_bulk_delete_tasks)),
    )
}

#[cfg(test)]
mod lifecycle_tests {

    use std::{sync::Arc, time::Duration};

    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
        middleware::from_fn_with_state,
    };
    use db::models::{
        project::CreateProject,
        session::{CreateSession, Session},
        task::{CreateTask, TaskStatus},
        workflow_association::WorkflowAssociationResolution,
        workspace::{CreateWorkspace, Workspace},
    };
    use local_deployment::LocalDeployment;
    use services::services::domain_events::{
        OrchestrationEventPublisherHandle, OrchestrationEventType,
        RecordingOrchestrationEventPublisher,
    };
    use tower::ServiceExt;

    use super::*;
    use crate::middleware::load_project_middleware;

    fn reset_test_database() {
        let db_path = utils::assets::asset_dir().join("db.sqlite");
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn task_workflow_association_routes_resolve_precedence() {
        let _lock = crate::TEST_DB_LOCK.lock().unwrap();
        reset_test_database();
        let deployment = LocalDeployment::new().await.unwrap();
        let project = create_project(&deployment, "workflow-task").await;
        let group = TaskGroup::create(
            &deployment.db().pool,
            project.id,
            "Workflow group".to_string(),
            None,
            None,
        )
        .await
        .unwrap();
        let task = Task::create(
            &deployment.db().pool,
            &CreateTask {
                project_id: project.id,
                title: "Resolve association".to_string(),
                description: None,
                status: Some(TaskStatus::Todo),
                parent_workspace_id: None,
                image_ids: None,
                shared_task_id: None,
                task_group_id: Some(group.id),
            },
            Uuid::new_v4(),
        )
        .await
        .unwrap();

        let app = super::router(&deployment).with_state(deployment.clone());

        let request = |method: &str, path: String, body: serde_json::Value| {
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap()
        };

        app.clone()
            .oneshot(request(
                "PUT",
                format!("/tasks/{}/workflow-association", task.id),
                serde_json::json!({
                    "workflow_id": "wf-task",
                    "label": "Task workflow",
                    "url": "https://workflows.example/task-override"
                }),
            ))
            .await
            .unwrap();

        let get_response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/tasks/{}/workflow-associations", task.id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_response.status(), StatusCode::OK);
        let body = to_bytes(get_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let api_response: ApiResponse<WorkflowAssociationResolution> =
            serde_json::from_slice(&body).unwrap();
        let data = api_response.into_data().unwrap();
        assert_eq!(data.effective.unwrap().workflow_id, "wf-task");
    }

    fn bulk_tasks_test_router(deployment: DeploymentImpl) -> Router {
        let project_routes = Router::new().nest(
            "/{id}",
            project_router(&deployment).layer(from_fn_with_state(
                deployment.clone(),
                load_project_middleware,
            )),
        );

        Router::new()
            .nest("/projects", project_routes)
            .with_state(deployment)
    }

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

    async fn create_task(
        deployment: &DeploymentImpl,
        project_id: Uuid,
        title: &str,
        status: TaskStatus,
        parent_workspace_id: Option<Uuid>,
    ) -> Task {
        Task::create(
            &deployment.db().pool,
            &CreateTask {
                project_id,
                title: title.to_string(),
                description: None,
                status: Some(status),
                parent_workspace_id,
                image_ids: None,
                shared_task_id: None,
                task_group_id: None,
            },
            Uuid::new_v4(),
        )
        .await
        .unwrap()
    }

    async fn create_workspace_for_task(
        deployment: &DeploymentImpl,
        task_id: Uuid,
        branch: &str,
    ) -> Workspace {
        Workspace::create(
            &deployment.db().pool,
            &CreateWorkspace {
                branch: branch.to_string(),
                agent_working_dir: None,
            },
            Uuid::new_v4(),
            task_id,
        )
        .await
        .unwrap()
    }

    async fn create_running_process_for_task(deployment: &DeploymentImpl, task_id: Uuid) {
        let workspace = create_workspace_for_task(deployment, task_id, "bulk-delete-test").await;
        let session = Session::create(
            &deployment.db().pool,
            &CreateSession {
                executor: Some("test-executor".to_string()),
            },
            Uuid::new_v4(),
            workspace.id,
        )
        .await
        .unwrap();

        sqlx::query(
            r#"INSERT INTO execution_processes (
                    id, session_id, conversation_session_id, run_reason, executor_action,
                    status, exit_code, dropped, input_tokens, output_tokens,
                    started_at, completed_at, created_at, updated_at
               ) VALUES (?, ?, NULL, 'codingagent', '{}', 'running', NULL, FALSE, NULL, NULL,
                         datetime('now'), NULL, datetime('now'), datetime('now'))"#,
        )
        .bind(Uuid::new_v4())
        .bind(session.id)
        .execute(&deployment.db().pool)
        .await
        .unwrap();
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
                .map(|(_, envelope)| envelope.event_type())
                .collect::<Vec<_>>();
            if events.len() >= minimum || tokio::time::Instant::now() >= deadline {
                return events;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn task_routes_emit_task_lifecycle_events() {
        let _lock = crate::TEST_DB_LOCK.lock().unwrap();
        let publisher = RecordingOrchestrationEventPublisher::default();
        let publisher_handle: OrchestrationEventPublisherHandle = Arc::new(publisher.clone());
        let deployment = LocalDeployment::new_with_orchestration_event_publisher(publisher_handle)
            .await
            .unwrap();
        let project = create_project(&deployment, "task-lifecycle-events").await;

        let created = super::create_task(
            State(deployment.clone()),
            Json(CreateTask {
                project_id: project.id,
                title: "task-a".to_string(),
                description: Some("first".to_string()),
                status: Some(TaskStatus::Todo),
                parent_workspace_id: None,
                image_ids: None,
                shared_task_id: None,
                task_group_id: None,
            }),
        )
        .await
        .unwrap()
        .0
        .into_data()
        .unwrap();

        let updated = super::update_task(
            Extension(created.clone()),
            State(deployment.clone()),
            Json(UpdateTask {
                title: Some("task-a-updated".to_string()),
                description: Some("updated".to_string()),
                status: Some(TaskStatus::InReview),
                parent_workspace_id: None,
                task_group_id: None,
                image_ids: None,
            }),
        )
        .await
        .unwrap()
        .0
        .into_data()
        .unwrap();

        let _ = super::delete_task(Extension(updated), State(deployment.clone()))
            .await
            .unwrap();

        let event_types = wait_for_event_types(&publisher, 4).await;
        assert!(event_types.contains(&OrchestrationEventType::TaskCreated));
        assert!(event_types.contains(&OrchestrationEventType::TaskUpdated));
        assert!(event_types.contains(&OrchestrationEventType::TaskStatusChanged));
        assert!(event_types.contains(&OrchestrationEventType::TaskDeleted));
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn bulk_status_rejects_cross_project_tasks_without_mutating() {
        let _lock = crate::TEST_DB_LOCK.lock().unwrap();
        reset_test_database();
        let deployment = LocalDeployment::new().await.unwrap();
        let project = create_project(&deployment, "bulk-status-main").await;
        let other_project = create_project(&deployment, "bulk-status-other").await;
        let first_task =
            create_task(&deployment, project.id, "first", TaskStatus::Todo, None).await;
        let foreign_task = create_task(
            &deployment,
            other_project.id,
            "foreign",
            TaskStatus::Todo,
            None,
        )
        .await;
        let app = bulk_tasks_test_router(deployment.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/projects/{}/tasks/bulk/status", project.id))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "taskIds": [first_task.id, foreign_task.id],
                            "status": "inreview"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            Task::find_by_id(&deployment.db().pool, first_task.id)
                .await
                .unwrap()
                .unwrap()
                .status,
            TaskStatus::Todo
        );
        assert_eq!(
            Task::find_by_id(&deployment.db().pool, foreign_task.id)
                .await
                .unwrap()
                .unwrap()
                .status,
            TaskStatus::Todo
        );
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn bulk_status_dispatches_status_change_side_effects() {
        let _lock = crate::TEST_DB_LOCK.lock().unwrap();
        reset_test_database();
        let deployment = LocalDeployment::new().await.unwrap();
        let project = create_project(&deployment, "bulk-status-events").await;
        let first_task =
            create_task(&deployment, project.id, "first", TaskStatus::Todo, None).await;
        let second_task =
            create_task(&deployment, project.id, "second", TaskStatus::Todo, None).await;
        let app = bulk_tasks_test_router(deployment.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/projects/{}/tasks/bulk/status", project.id))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "taskIds": [first_task.id, second_task.id],
                            "status": "inreview"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let api_response: ApiResponse<BulkUpdateTaskStatusResponse> =
            serde_json::from_slice(&body).unwrap();
        let data = api_response.into_data().unwrap();
        assert_eq!(data.tasks.len(), 2);
        assert!(
            data.tasks
                .iter()
                .all(|task| task.status == TaskStatus::InReview)
        );
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn bulk_delete_rejects_if_any_task_has_running_processes() {
        let _lock = crate::TEST_DB_LOCK.lock().unwrap();
        reset_test_database();
        let deployment = LocalDeployment::new().await.unwrap();
        let project = create_project(&deployment, "bulk-delete-guard").await;
        let deletable_task =
            create_task(&deployment, project.id, "deletable", TaskStatus::Todo, None).await;
        let blocked_task =
            create_task(&deployment, project.id, "blocked", TaskStatus::Todo, None).await;
        create_running_process_for_task(&deployment, blocked_task.id).await;
        let app = bulk_tasks_test_router(deployment.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/projects/{}/tasks/bulk/delete", project.id))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "taskIds": [deletable_task.id, blocked_task.id]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert!(
            Task::find_by_id(&deployment.db().pool, deletable_task.id)
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            Task::find_by_id(&deployment.db().pool, blocked_task.id)
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn bulk_delete_clears_child_parent_links_and_returns_deleted_ids() {
        let _lock = crate::TEST_DB_LOCK.lock().unwrap();
        reset_test_database();
        let deployment = LocalDeployment::new().await.unwrap();
        let project = create_project(&deployment, "bulk-delete-side-effects").await;
        let parent_task =
            create_task(&deployment, project.id, "parent", TaskStatus::Todo, None).await;
        let sibling_task =
            create_task(&deployment, project.id, "sibling", TaskStatus::Todo, None).await;
        let parent_workspace =
            create_workspace_for_task(&deployment, parent_task.id, "parent-branch").await;
        let child_task = create_task(
            &deployment,
            project.id,
            "child",
            TaskStatus::Todo,
            Some(parent_workspace.id),
        )
        .await;
        let app = bulk_tasks_test_router(deployment.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/projects/{}/tasks/bulk/delete", project.id))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "taskIds": [parent_task.id, sibling_task.id]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let api_response: ApiResponse<BulkDeleteTasksResponse> =
            serde_json::from_slice(&body).unwrap();
        let data = api_response.into_data().unwrap();
        assert_eq!(data.deleted_tasks.len(), 2);
        assert!(
            data.deleted_tasks
                .iter()
                .any(|task| task.id == parent_task.id)
        );
        assert!(
            data.deleted_tasks
                .iter()
                .any(|task| task.id == sibling_task.id)
        );

        assert!(
            Task::find_by_id(&deployment.db().pool, parent_task.id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            Task::find_by_id(&deployment.db().pool, sibling_task.id)
                .await
                .unwrap()
                .is_none()
        );

        let refreshed_child = Task::find_by_id(&deployment.db().pool, child_task.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(refreshed_child.parent_workspace_id, None);
    }
}
