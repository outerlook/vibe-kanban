use std::path::PathBuf;

use anyhow;
use axum::{
    Extension, Json, Router,
    extract::{
        Path, Query, State,
        ws::{WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    middleware::from_fn_with_state,
    response::{IntoResponse, Json as ResponseJson},
    routing::{get, post},
};
use db::models::{
    project::{
        CreateProject, Project, ProjectError, ProjectWithTaskCounts, SearchResult, UpdateProject,
    },
    project_repo::{CreateProjectRepo, ProjectRepo, UpdateProjectRepo},
    repo::Repo,
    task_group::TaskGroup,
    workspace::Workspace,
};
use deployment::Deployment;
use futures_util::{SinkExt, StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use services::services::{
    file_search_cache::SearchQuery,
    github_client::{GitHubClient, PullRequestSummary},
    project::ProjectServiceError,
    remote_client::CreateRemoteProjectPayload,
};
use ts_rs::TS;
use utils::{
    api::projects::{RemoteProject, RemoteProjectMembersResponse},
    response::ApiResponse,
};
use uuid::Uuid;

use crate::{
    DeploymentImpl, error::ApiError, middleware::load_project_middleware,
    routes::settings::get_github_token,
};

#[derive(Deserialize, TS)]
pub struct LinkToExistingRequest {
    pub remote_project_id: Uuid,
}

#[derive(Deserialize, TS)]
pub struct CreateRemoteProjectRequest {
    pub organization_id: Uuid,
    pub name: String,
}

/// A pull request with its optional unresolved review thread count.
/// The count may be null if it's being loaded progressively.
#[derive(Debug, Clone, Serialize, TS)]
pub struct PrWithComments {
    #[serde(flatten)]
    pub pr: PullRequestSummary,
    pub unresolved_count: Option<usize>,
}

/// PRs grouped by repository.
#[derive(Debug, Clone, Serialize, TS)]
pub struct RepoPrs {
    pub repo_id: Uuid,
    pub repo_name: String,
    pub display_name: String,
    pub pull_requests: Vec<PrWithComments>,
}

/// Response for GET /api/projects/:id/prs
#[derive(Debug, Clone, Serialize, TS)]
pub struct ProjectPrsResponse {
    pub repos: Vec<RepoPrs>,
}

/// A task group summary for matching against worktrees
#[derive(Debug, Clone, Serialize, TS)]
pub struct MatchingTaskGroup {
    pub id: Uuid,
    pub name: String,
}

/// Worktree info with matching task groups
#[derive(Debug, Clone, Serialize, TS)]
pub struct WorktreeInfo {
    pub path: String,
    pub branch: Option<String>,
    pub is_main: bool,
    pub matching_groups: Vec<MatchingTaskGroup>,
}

/// Response for GET /api/projects/:id/worktrees
#[derive(Debug, Clone, Serialize, TS)]
pub struct ProjectWorktreesResponse {
    pub worktrees: Vec<WorktreeInfo>,
}

pub async fn get_projects(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<ProjectWithTaskCounts>>>, ApiError> {
    let projects = Project::find_all(&deployment.db().pool).await?;
    Ok(ResponseJson(ApiResponse::success(projects)))
}

pub async fn stream_projects_ws(
    ws: WebSocketUpgrade,
    State(deployment): State<DeploymentImpl>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_projects_ws(socket, deployment).await {
            tracing::warn!("projects WS closed: {}", e);
        }
    })
}

async fn handle_projects_ws(socket: WebSocket, deployment: DeploymentImpl) -> anyhow::Result<()> {
    let mut stream = deployment
        .events()
        .stream_projects_raw()
        .await?
        .map_ok(|msg| msg.to_ws_message_unchecked());

    // Split socket into sender and receiver
    let (mut sender, mut receiver) = socket.split();

    // Drain (and ignore) any client->server messages so pings/pongs work
    tokio::spawn(async move { while let Some(Ok(_)) = receiver.next().await {} });

    // Forward server messages
    while let Some(item) = stream.next().await {
        match item {
            Ok(msg) => {
                if sender.send(msg).await.is_err() {
                    break; // client disconnected
                }
            }
            Err(e) => {
                tracing::error!("stream error: {}", e);
                break;
            }
        }
    }

    Ok(())
}

pub async fn get_project(
    Extension(project): Extension<Project>,
) -> Result<ResponseJson<ApiResponse<Project>>, ApiError> {
    Ok(ResponseJson(ApiResponse::success(project)))
}

pub async fn link_project_to_existing_remote(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<LinkToExistingRequest>,
) -> Result<ResponseJson<ApiResponse<Project>>, ApiError> {
    let client = deployment.remote_client()?;

    let remote_project = client.get_project(payload.remote_project_id).await?;

    let updated_project = apply_remote_project_link(&deployment, project, remote_project).await?;

    Ok(ResponseJson(ApiResponse::success(updated_project)))
}

pub async fn create_and_link_remote_project(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateRemoteProjectRequest>,
) -> Result<ResponseJson<ApiResponse<Project>>, ApiError> {
    let repo_name = payload.name.trim().to_string();
    if repo_name.trim().is_empty() {
        return Err(ApiError::Conflict(
            "Remote project name cannot be empty.".to_string(),
        ));
    }

    let client = deployment.remote_client()?;

    let remote_project = client
        .create_project(&CreateRemoteProjectPayload {
            organization_id: payload.organization_id,
            name: repo_name,
            metadata: None,
        })
        .await?;

    let updated_project = apply_remote_project_link(&deployment, project, remote_project).await?;

    Ok(ResponseJson(ApiResponse::success(updated_project)))
}

pub async fn unlink_project(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Project>>, ApiError> {
    let updated_project = deployment
        .project()
        .unlink_from_remote(&deployment.db().pool, &project)
        .await?;

    Ok(ResponseJson(ApiResponse::success(updated_project)))
}

pub async fn get_remote_project_by_id(
    State(deployment): State<DeploymentImpl>,
    Path(remote_project_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<RemoteProject>>, ApiError> {
    let client = deployment.remote_client()?;

    let remote_project = client.get_project(remote_project_id).await?;

    Ok(ResponseJson(ApiResponse::success(remote_project)))
}

pub async fn get_project_remote_members(
    State(deployment): State<DeploymentImpl>,
    Extension(project): Extension<Project>,
) -> Result<ResponseJson<ApiResponse<RemoteProjectMembersResponse>>, ApiError> {
    let remote_project_id = project.remote_project_id.ok_or_else(|| {
        ApiError::Conflict("Project is not linked to a remote project".to_string())
    })?;

    let client = deployment.remote_client()?;

    let remote_project = client.get_project(remote_project_id).await?;
    let members = client
        .list_members(remote_project.organization_id)
        .await?
        .members;

    Ok(ResponseJson(ApiResponse::success(
        RemoteProjectMembersResponse {
            organization_id: remote_project.organization_id,
            members,
        },
    )))
}

async fn apply_remote_project_link(
    deployment: &DeploymentImpl,
    project: Project,
    remote_project: RemoteProject,
) -> Result<Project, ApiError> {
    if project.remote_project_id.is_some() {
        return Err(ApiError::Conflict(
            "Project is already linked to a remote project. Unlink it first.".to_string(),
        ));
    }

    let updated_project = deployment
        .project()
        .link_to_remote(&deployment.db().pool, project.id, remote_project)
        .await?;

    deployment
        .track_if_analytics_allowed(
            "project_linked_to_remote",
            serde_json::json!({
                "project_id": project.id.to_string(),
            }),
        )
        .await;

    Ok(updated_project)
}

pub async fn create_project(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateProject>,
) -> Result<ResponseJson<ApiResponse<Project>>, ApiError> {
    tracing::debug!("Creating project '{}'", payload.name);
    let repo_count = payload.repositories.len();

    match deployment
        .project()
        .create_project(&deployment.db().pool, deployment.repo(), payload)
        .await
    {
        Ok(project) => {
            // Track project creation event
            deployment
                .track_if_analytics_allowed(
                    "project_created",
                    serde_json::json!({
                        "project_id": project.id.to_string(),
                        "repository_count": repo_count,
                        "trigger": "manual",
                    }),
                )
                .await;

            Ok(ResponseJson(ApiResponse::success(project)))
        }
        Err(ProjectServiceError::DuplicateGitRepoPath) => Ok(ResponseJson(ApiResponse::error(
            "Duplicate repository path provided",
        ))),
        Err(ProjectServiceError::DuplicateRepositoryName) => Ok(ResponseJson(ApiResponse::error(
            "Duplicate repository name provided",
        ))),
        Err(ProjectServiceError::PathNotFound(_)) => Ok(ResponseJson(ApiResponse::error(
            "The specified path does not exist",
        ))),
        Err(ProjectServiceError::PathNotDirectory(_)) => Ok(ResponseJson(ApiResponse::error(
            "The specified path is not a directory",
        ))),
        Err(ProjectServiceError::NotGitRepository(_)) => Ok(ResponseJson(ApiResponse::error(
            "The specified directory is not a git repository",
        ))),
        Err(e) => Err(ProjectError::CreateFailed(e.to_string()).into()),
    }
}

pub async fn update_project(
    Extension(existing_project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<UpdateProject>,
) -> Result<ResponseJson<ApiResponse<Project>>, StatusCode> {
    match deployment
        .project()
        .update_project(&deployment.db().pool, &existing_project, payload)
        .await
    {
        Ok(project) => Ok(ResponseJson(ApiResponse::success(project))),
        Err(e) => {
            tracing::error!("Failed to update project: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn delete_project(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    match deployment
        .project()
        .delete_project(&deployment.db().pool, project.id)
        .await
    {
        Ok(rows_affected) => {
            if rows_affected == 0 {
                Err(StatusCode::NOT_FOUND)
            } else {
                deployment
                    .track_if_analytics_allowed(
                        "project_deleted",
                        serde_json::json!({
                            "project_id": project.id.to_string(),
                        }),
                    )
                    .await;

                Ok(ResponseJson(ApiResponse::success(())))
            }
        }
        Err(e) => {
            tracing::error!("Failed to delete project: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(serde::Deserialize)]
pub struct OpenEditorRequest {
    editor_type: Option<String>,
    git_repo_path: Option<PathBuf>,
}

#[derive(Debug, serde::Serialize, ts_rs::TS)]
pub struct OpenEditorResponse {
    pub url: Option<String>,
}

pub async fn open_project_in_editor(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<Option<OpenEditorRequest>>,
) -> Result<ResponseJson<ApiResponse<OpenEditorResponse>>, ApiError> {
    let path = if let Some(ref req) = payload
        && let Some(ref specified_path) = req.git_repo_path
    {
        specified_path.clone()
    } else {
        let repositories = deployment
            .project()
            .get_repositories(&deployment.db().pool, project.id)
            .await?;

        repositories
            .first()
            .map(|r| r.path.clone())
            .ok_or_else(|| ApiError::BadRequest("Project has no repositories".to_string()))?
    };

    let editor_config = {
        let config = deployment.config().read().await;
        let editor_type_str = payload.as_ref().and_then(|req| req.editor_type.as_deref());
        config.editor.with_override(editor_type_str)?
    };

    match editor_config.open_file(&path).await {
        Ok(url) => {
            tracing::info!(
                "Opened editor for project {} at path: {}{}",
                project.id,
                path.to_string_lossy(),
                if url.is_some() { " (remote mode)" } else { "" }
            );

            deployment
                .track_if_analytics_allowed(
                    "project_editor_opened",
                    serde_json::json!({
                        "project_id": project.id.to_string(),
                        "editor_type": payload.as_ref().and_then(|req| req.editor_type.as_ref()),
                        "remote_mode": url.is_some(),
                    }),
                )
                .await;

            Ok(ResponseJson(ApiResponse::success(OpenEditorResponse {
                url,
            })))
        }
        Err(e) => {
            tracing::error!("Failed to open editor for project {}: {:?}", project.id, e);
            Err(ApiError::EditorOpen(e))
        }
    }
}

pub async fn search_project_files(
    State(deployment): State<DeploymentImpl>,
    Extension(project): Extension<Project>,
    Query(search_query): Query<SearchQuery>,
) -> Result<ResponseJson<ApiResponse<Vec<SearchResult>>>, StatusCode> {
    if search_query.q.trim().is_empty() {
        return Ok(ResponseJson(ApiResponse::error(
            "Query parameter 'q' is required and cannot be empty",
        )));
    }

    let repositories = match deployment
        .project()
        .get_repositories(&deployment.db().pool, project.id)
        .await
    {
        Ok(repos) => repos,
        Err(e) => {
            tracing::error!("Failed to get repositories: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    match deployment
        .project()
        .search_files(
            deployment.file_search_cache().as_ref(),
            &repositories,
            &search_query,
        )
        .await
    {
        Ok(results) => Ok(ResponseJson(ApiResponse::success(results))),
        Err(e) => {
            tracing::error!("Failed to search files: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_project_repositories(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<Repo>>>, ApiError> {
    let repositories = deployment
        .project()
        .get_repositories(&deployment.db().pool, project.id)
        .await?;
    Ok(ResponseJson(ApiResponse::success(repositories)))
}

pub async fn add_project_repository(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateProjectRepo>,
) -> Result<ResponseJson<ApiResponse<Repo>>, ApiError> {
    tracing::debug!(
        "Adding repository '{}' to project {} (path: {})",
        payload.display_name,
        project.id,
        payload.git_repo_path
    );

    match deployment
        .project()
        .add_repository(
            &deployment.db().pool,
            deployment.repo(),
            project.id,
            &payload,
        )
        .await
    {
        Ok(repository) => {
            deployment
                .track_if_analytics_allowed(
                    "project_repository_added",
                    serde_json::json!({
                        "project_id": project.id.to_string(),
                        "repository_id": repository.id.to_string(),
                    }),
                )
                .await;

            Ok(ResponseJson(ApiResponse::success(repository)))
        }
        Err(ProjectServiceError::PathNotFound(_)) => {
            tracing::warn!(
                "Failed to add repository to project {}: path does not exist",
                project.id
            );
            Ok(ResponseJson(ApiResponse::error(
                "The specified path does not exist",
            )))
        }
        Err(ProjectServiceError::PathNotDirectory(_)) => {
            tracing::warn!(
                "Failed to add repository to project {}: path is not a directory",
                project.id
            );
            Ok(ResponseJson(ApiResponse::error(
                "The specified path is not a directory",
            )))
        }
        Err(ProjectServiceError::NotGitRepository(_)) => {
            tracing::warn!(
                "Failed to add repository to project {}: not a git repository",
                project.id
            );
            Ok(ResponseJson(ApiResponse::error(
                "The specified directory is not a git repository",
            )))
        }
        Err(ProjectServiceError::DuplicateRepositoryName) => {
            tracing::warn!(
                "Failed to add repository to project {}: duplicate repository name",
                project.id
            );
            Ok(ResponseJson(ApiResponse::error(
                "A repository with this name already exists in the project",
            )))
        }
        Err(ProjectServiceError::DuplicateGitRepoPath) => {
            tracing::warn!(
                "Failed to add repository to project {}: duplicate repository path",
                project.id
            );
            Ok(ResponseJson(ApiResponse::error(
                "A repository with this path already exists in the project",
            )))
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn delete_project_repository(
    State(deployment): State<DeploymentImpl>,
    Path((project_id, repo_id)): Path<(Uuid, Uuid)>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    tracing::debug!(
        "Removing repository {} from project {}",
        repo_id,
        project_id
    );

    match deployment
        .project()
        .delete_repository(&deployment.db().pool, project_id, repo_id)
        .await
    {
        Ok(()) => {
            deployment
                .track_if_analytics_allowed(
                    "project_repository_removed",
                    serde_json::json!({
                        "project_id": project_id.to_string(),
                        "repository_id": repo_id.to_string(),
                    }),
                )
                .await;

            Ok(ResponseJson(ApiResponse::success(())))
        }
        Err(ProjectServiceError::RepositoryNotFound) => {
            tracing::warn!(
                "Failed to remove repository {} from project {}: not found",
                repo_id,
                project_id
            );
            Ok(ResponseJson(ApiResponse::error("Repository not found")))
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn get_project_repository(
    State(deployment): State<DeploymentImpl>,
    Path((project_id, repo_id)): Path<(Uuid, Uuid)>,
) -> Result<ResponseJson<ApiResponse<ProjectRepo>>, ApiError> {
    match ProjectRepo::find_by_project_and_repo(&deployment.db().pool, project_id, repo_id).await {
        Ok(Some(project_repo)) => Ok(ResponseJson(ApiResponse::success(project_repo))),
        Ok(None) => Err(ApiError::BadRequest(
            "Repository not found in project".to_string(),
        )),
        Err(e) => Err(e.into()),
    }
}

pub async fn update_project_repository(
    State(deployment): State<DeploymentImpl>,
    Path((project_id, repo_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateProjectRepo>,
) -> Result<ResponseJson<ApiResponse<ProjectRepo>>, ApiError> {
    match ProjectRepo::update(&deployment.db().pool, project_id, repo_id, &payload).await {
        Ok(project_repo) => Ok(ResponseJson(ApiResponse::success(project_repo))),
        Err(db::models::project_repo::ProjectRepoError::NotFound) => Err(ApiError::BadRequest(
            "Repository not found in project".to_string(),
        )),
        Err(e) => Err(e.into()),
    }
}

/// GET /api/projects/:id/prs - Get open PRs across all repos, filtered by task group base branches.
pub async fn get_project_prs(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<ProjectPrsResponse>>, ApiError> {
    let pool = &deployment.db().pool;

    // Load GitHub token from settings
    let token = get_github_token(pool)
        .await?
        .ok_or_else(|| ApiError::BadRequest("GitHub token not configured".to_string()))?;

    let github_client = GitHubClient::new(token)
        .map_err(|e| ApiError::Internal(format!("Failed to create GitHub client: {}", e)))?;

    // Get unique base branches from task groups
    let base_branches = TaskGroup::get_unique_base_branches(pool, project.id).await?;

    // If no base branches, return empty response
    if base_branches.is_empty() {
        return Ok(ResponseJson(ApiResponse::success(ProjectPrsResponse {
            repos: vec![],
        })));
    }

    // Get project repositories
    let repositories = deployment
        .project()
        .get_repositories(pool, project.id)
        .await?;

    let git_service = deployment.git();
    let mut repo_prs_list = Vec::new();

    for repo in repositories {
        // Get GitHub repo info from remote URL
        let repo_info = match git_service.get_github_repo_info(&repo.path) {
            Ok(info) => info,
            Err(e) => {
                tracing::warn!(
                    "Skipping repo {} ({}): failed to get GitHub info: {}",
                    repo.name,
                    repo.path.display(),
                    e
                );
                continue;
            }
        };

        // Fetch PRs for all head branches in parallel
        let pr_futures = base_branches.iter().map(|head_branch| {
            let head_ref = format!("{}:{}", repo_info.owner, head_branch);
            let owner = repo_info.owner.clone();
            let repo_name = repo_info.repo_name.clone();
            let client = &github_client;
            async move {
                let result = client
                    .list_open_prs_by_head(&owner, &repo_name, &head_ref)
                    .await;
                (head_branch.clone(), result)
            }
        });

        let pr_results = futures_util::future::join_all(pr_futures).await;

        let mut all_prs = Vec::new();
        for (head_branch, result) in pr_results {
            match result {
                Ok(prs) => {
                    all_prs.extend(prs);
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to fetch PRs for {}/{} (head: {}): {}",
                        repo_info.owner,
                        repo_info.repo_name,
                        head_branch,
                        e
                    );
                }
            }
        }

        // Skip repos with no PRs
        if all_prs.is_empty() {
            continue;
        }

        // Return PRs with null unresolved_count for progressive loading
        // (counts are fetched separately via /prs/unresolved-counts)
        let prs_with_comments: Vec<PrWithComments> = all_prs
            .into_iter()
            .map(|pr| PrWithComments {
                pr,
                unresolved_count: None,
            })
            .collect();

        repo_prs_list.push(RepoPrs {
            repo_id: repo.id,
            repo_name: repo.name.clone(),
            display_name: repo.display_name.clone(),
            pull_requests: prs_with_comments,
        });
    }

    Ok(ResponseJson(ApiResponse::success(ProjectPrsResponse {
        repos: repo_prs_list,
    })))
}

/// Unresolved count for a single PR, keyed by repo and PR number.
#[derive(Debug, Clone, Serialize, TS)]
pub struct PrUnresolvedCount {
    pub repo_id: Uuid,
    pub pr_number: u64,
    pub unresolved_count: usize,
}

/// Response for GET /api/projects/:id/prs/unresolved-counts
#[derive(Debug, Clone, Serialize, TS)]
pub struct PrUnresolvedCountsResponse {
    pub counts: Vec<PrUnresolvedCount>,
}

/// GET /api/projects/:id/prs/unresolved-counts - Fetch unresolved comment counts for all PRs.
/// This endpoint is designed to be called after /prs to progressively load the counts.
pub async fn get_project_prs_unresolved_counts(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<PrUnresolvedCountsResponse>>, ApiError> {
    let pool = &deployment.db().pool;

    // Load GitHub token from settings
    let token = get_github_token(pool)
        .await?
        .ok_or_else(|| ApiError::BadRequest("GitHub token not configured".to_string()))?;

    let github_client = GitHubClient::new(token)
        .map_err(|e| ApiError::Internal(format!("Failed to create GitHub client: {}", e)))?;

    // Get unique base branches from task groups
    let base_branches = TaskGroup::get_unique_base_branches(pool, project.id).await?;

    // If no base branches, return empty response
    if base_branches.is_empty() {
        return Ok(ResponseJson(ApiResponse::success(
            PrUnresolvedCountsResponse { counts: vec![] },
        )));
    }

    // Get project repositories
    let repositories = deployment
        .project()
        .get_repositories(pool, project.id)
        .await?;

    let git_service = deployment.git();
    let mut all_counts = Vec::new();

    for repo in repositories {
        // Get GitHub repo info from remote URL
        let repo_info = match git_service.get_github_repo_info(&repo.path) {
            Ok(info) => info,
            Err(e) => {
                tracing::warn!(
                    "Skipping repo {} ({}): failed to get GitHub info: {}",
                    repo.name,
                    repo.path.display(),
                    e
                );
                continue;
            }
        };

        // Fetch PRs for all head branches in parallel to get PR numbers
        let pr_futures = base_branches.iter().map(|head_branch| {
            let head_ref = format!("{}:{}", repo_info.owner, head_branch);
            let owner = repo_info.owner.clone();
            let repo_name = repo_info.repo_name.clone();
            let client = &github_client;
            async move {
                client
                    .list_open_prs_by_head(&owner, &repo_name, &head_ref)
                    .await
            }
        });

        let pr_results = futures_util::future::join_all(pr_futures).await;

        let mut pr_numbers: Vec<u64> = Vec::new();
        for result in pr_results {
            if let Ok(prs) = result {
                pr_numbers.extend(prs.iter().map(|pr| pr.number));
            }
        }

        if pr_numbers.is_empty() {
            continue;
        }

        // Batch fetch unresolved counts for all PRs in this repo
        let unresolved_counts = github_client
            .get_unresolved_thread_counts_batch(&repo_info.owner, &repo_info.repo_name, &pr_numbers)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(
                    "Failed to batch fetch unresolved threads for {}/{}: {}",
                    repo_info.owner,
                    repo_info.repo_name,
                    e
                );
                // Return 0 for all PRs on failure
                pr_numbers.iter().map(|&num| (num, 0)).collect()
            });

        // Add counts for this repo
        for (pr_number, count) in unresolved_counts {
            all_counts.push(PrUnresolvedCount {
                repo_id: repo.id,
                pr_number,
                unresolved_count: count,
            });
        }
    }

    Ok(ResponseJson(ApiResponse::success(
        PrUnresolvedCountsResponse { counts: all_counts },
    )))
}

/// Response for GET /api/projects/:id/merge-queue-count
#[derive(Debug, Clone, Serialize, TS)]
pub struct MergeQueueCountResponse {
    pub count: i64,
}

/// GET /api/projects/:id/merge-queue-count - Get the number of entries in the merge queue
pub async fn get_merge_queue_count(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<MergeQueueCountResponse>>, ApiError> {
    let count = deployment.merge_queue_store().count_by_project(project.id);
    Ok(ResponseJson(ApiResponse::success(MergeQueueCountResponse {
        count,
    })))
}

/// GET /api/projects/:id/workspaces - Get all workspaces for a project's tasks
pub async fn get_project_workspaces(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<Workspace>>>, ApiError> {
    let pool = &deployment.db().pool;
    let workspaces = Workspace::fetch_by_project_id(pool, project.id).await?;
    Ok(ResponseJson(ApiResponse::success(workspaces)))
}

/// GET /api/projects/:id/worktrees - Discover all worktrees for a project's repositories
///
/// Returns a list of worktrees with their branches and matching task groups.
/// Task groups match when their base_branch equals the worktree's branch.
pub async fn get_project_worktrees(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<ProjectWorktreesResponse>>, ApiError> {
    let pool = &deployment.db().pool;

    // Get project repositories
    let repositories = deployment
        .project()
        .get_repositories(pool, project.id)
        .await?;

    if repositories.is_empty() {
        return Ok(ResponseJson(ApiResponse::success(ProjectWorktreesResponse {
            worktrees: vec![],
        })));
    }

    // Get all task groups for this project
    let task_groups = TaskGroup::find_by_project_id(pool, project.id).await?;

    let git_service = deployment.git();
    let mut all_worktrees = Vec::new();

    // Use the first repository to discover worktrees (they share the same git structure)
    if let Some(repo) = repositories.first() {
        match git_service.discover_worktrees(&repo.path) {
            Ok(entries) => {
                for entry in entries {
                    // Find task groups that match this worktree's branch
                    let matching_groups: Vec<MatchingTaskGroup> = entry
                        .branch
                        .as_ref()
                        .map(|branch| {
                            task_groups
                                .iter()
                                .filter(|group| {
                                    group
                                        .base_branch
                                        .as_ref()
                                        .is_some_and(|base| base == branch)
                                })
                                .map(|group| MatchingTaskGroup {
                                    id: group.id,
                                    name: group.name.clone(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();

                    all_worktrees.push(WorktreeInfo {
                        path: entry.path.to_string_lossy().to_string(),
                        branch: entry.branch,
                        is_main: entry.is_main,
                        matching_groups,
                    });
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to discover worktrees for project {}: {}",
                    project.name,
                    e
                );
            }
        }
    }

    Ok(ResponseJson(ApiResponse::success(ProjectWorktreesResponse {
        worktrees: all_worktrees,
    })))
}

pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let project_id_router = Router::new()
        .route(
            "/",
            get(get_project).put(update_project).delete(delete_project),
        )
        .route("/remote/members", get(get_project_remote_members))
        .route("/search", get(search_project_files))
        .route("/open-editor", post(open_project_in_editor))
        .route(
            "/link",
            post(link_project_to_existing_remote).delete(unlink_project),
        )
        .route("/link/create", post(create_and_link_remote_project))
        .route(
            "/repositories",
            get(get_project_repositories).post(add_project_repository),
        )
        .route("/prs", get(get_project_prs))
        .route(
            "/prs/unresolved-counts",
            get(get_project_prs_unresolved_counts),
        )
        .route("/merge-queue-count", get(get_merge_queue_count))
        .route("/workspaces", get(get_project_workspaces))
        .route("/worktrees", get(get_project_worktrees))
        .layer(from_fn_with_state(
            deployment.clone(),
            load_project_middleware,
        ));

    let projects_router = Router::new()
        .route("/", get(get_projects).post(create_project))
        .route(
            "/{project_id}/repositories/{repo_id}",
            get(get_project_repository)
                .put(update_project_repository)
                .delete(delete_project_repository),
        )
        .route("/stream/ws", get(stream_projects_ws))
        .nest("/{id}", project_id_router);

    Router::new().nest("/projects", projects_router).route(
        "/remote-projects/{remote_project_id}",
        get(get_remote_project_by_id),
    )
}
