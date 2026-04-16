use std::{collections::HashMap, path::PathBuf};

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
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use db::models::{
    project::{
        CreateProject, Project, ProjectError, ProjectWithTaskCounts, SearchResult, UpdateProject,
    },
    project_repo::{CreateProjectRepo, ProjectRepo, UpdateProjectRepo},
    repo::Repo,
    task_group::TaskGroup,
    workflow_association::{UpsertWorkflowAssociation, WorkflowAssociation},
    workspace::Workspace,
};
use deployment::Deployment;
use serde::{Deserialize, Serialize};
use services::services::{
    file_search_cache::SearchQuery,
    github::{GitHubService, GitHubServiceError, UnifiedPrComment},
    github_client::{GitHubClient, GitHubClientError, PullRequestSummary},
    pr_cache::{
        ProjectPrPage, ProjectPrPageCacheKey, ProjectPrPageResponse, ProjectPrSummary,
        ProjectRepoPrPage,
    },
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
    DeploymentImpl,
    error::ApiError,
    middleware::load_project_middleware,
    routes::{settings::get_github_token, ws_helpers::forward_stream_to_ws},
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

/// A project repository that can be resolved to a GitHub owner/repository pair.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectGitHubRepository {
    pub project_id: Uuid,
    pub project_name: String,
    pub repo_id: Uuid,
    pub repo_name: String,
    pub display_name: String,
    pub path: String,
    pub github_owner: String,
    pub github_repo_name: String,
    pub github_full_name: String,
}

const DEFAULT_PROJECT_PRS_LIMIT: usize = 25;
const MAX_PROJECT_PRS_LIMIT: usize = 100;

#[derive(Debug, Clone, Deserialize, Serialize, TS, Default)]
pub struct GetProjectPrPageQuery {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
    pub base_branch: Option<String>,
    pub search: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
struct ProjectPrCursor {
    pub updated_at: DateTime<Utc>,
    pub repo_id: Uuid,
    pub pr_number: u64,
}

#[derive(Debug, Clone)]
struct NormalizedProjectPrPageQuery {
    cache_key: ProjectPrPageCacheKey,
    cursor: Option<ProjectPrCursor>,
}

#[derive(Debug, Clone)]
struct ProjectPrRecord {
    repo_id: Uuid,
    repo_name: String,
    display_name: String,
    pr: PullRequestSummary,
}

impl GetProjectPrPageQuery {
    fn normalize(self, project_id: Uuid) -> Result<NormalizedProjectPrPageQuery, ApiError> {
        let limit = self
            .limit
            .unwrap_or(DEFAULT_PROJECT_PRS_LIMIT)
            .clamp(1, MAX_PROJECT_PRS_LIMIT);
        let base_branch = normalize_optional_filter(self.base_branch);
        let search = normalize_optional_filter(self.search).map(|term| term.to_lowercase());
        let cursor = self
            .cursor
            .as_deref()
            .map(decode_project_pr_cursor)
            .transpose()?;
        let canonical_cursor = cursor.as_ref().map(encode_project_pr_cursor).transpose()?;

        Ok(NormalizedProjectPrPageQuery {
            cache_key: ProjectPrPageCacheKey {
                project_id,
                cursor: canonical_cursor,
                limit,
                base_branch,
                search,
            },
            cursor,
        })
    }
}

fn normalize_optional_filter(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn encode_project_pr_cursor(cursor: &ProjectPrCursor) -> Result<String, ApiError> {
    let payload = serde_json::to_vec(cursor)
        .map_err(|e| ApiError::Internal(format!("Failed to encode PR cursor: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(payload))
}

fn decode_project_pr_cursor(cursor: &str) -> Result<ProjectPrCursor, ApiError> {
    let payload = URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| ApiError::BadRequest("Invalid PR cursor".to_string()))?;
    serde_json::from_slice(&payload)
        .map_err(|_| ApiError::BadRequest("Invalid PR cursor".to_string()))
}

fn compare_project_prs(left: &ProjectPrRecord, right: &ProjectPrRecord) -> std::cmp::Ordering {
    right
        .pr
        .updated_at
        .cmp(&left.pr.updated_at)
        .then_with(|| left.repo_id.cmp(&right.repo_id))
        .then_with(|| left.pr.number.cmp(&right.pr.number))
}

fn is_after_cursor(pr: &ProjectPrRecord, cursor: &ProjectPrCursor) -> bool {
    pr.pr.updated_at < cursor.updated_at
        || (pr.pr.updated_at == cursor.updated_at
            && (pr.repo_id > cursor.repo_id
                || (pr.repo_id == cursor.repo_id && pr.pr.number > cursor.pr_number)))
}

fn paginate_project_prs(
    mut prs: Vec<ProjectPrRecord>,
    cursor: Option<&ProjectPrCursor>,
    limit: usize,
) -> Result<ProjectPrPageResponse, ApiError> {
    prs.sort_by(compare_project_prs);

    let start_index = cursor
        .and_then(|cursor| prs.iter().position(|pr| is_after_cursor(pr, cursor)))
        .unwrap_or(0);

    let page_slice = prs
        .into_iter()
        .skip(start_index)
        .take(limit + 1)
        .collect::<Vec<_>>();
    let has_more = page_slice.len() > limit;
    let page_items = if has_more {
        page_slice[..limit].to_vec()
    } else {
        page_slice
    };
    let next_cursor = page_items
        .last()
        .filter(|_| has_more)
        .map(|last| {
            encode_project_pr_cursor(&ProjectPrCursor {
                updated_at: last.pr.updated_at,
                repo_id: last.repo_id,
                pr_number: last.pr.number,
            })
        })
        .transpose()?;

    Ok(ProjectPrPageResponse {
        repos: group_project_prs_by_repo(page_items),
        page: ProjectPrPage {
            limit,
            next_cursor,
            has_more,
        },
    })
}

fn group_project_prs_by_repo(prs: Vec<ProjectPrRecord>) -> Vec<ProjectRepoPrPage> {
    let mut grouped = Vec::new();
    let mut repo_indices = HashMap::new();

    for record in prs {
        let repo_index = if let Some(index) = repo_indices.get(&record.repo_id) {
            *index
        } else {
            let index = grouped.len();
            grouped.push(ProjectRepoPrPage {
                repo_id: record.repo_id,
                repo_name: record.repo_name.clone(),
                display_name: record.display_name.clone(),
                pull_requests: Vec::new(),
            });
            repo_indices.insert(record.repo_id, index);
            index
        };

        grouped[repo_index].pull_requests.push(ProjectPrSummary {
            pr: record.pr,
            unresolved_count: None,
        });
    }

    grouped
}

fn map_github_client_error(context: &str, error: GitHubClientError) -> ApiError {
    ApiError::Internal(format!("{context}: {error}"))
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
    let stream = deployment.events().stream_projects_raw().await?;
    forward_stream_to_ws(socket, stream).await
}

pub async fn get_project(
    Extension(project): Extension<Project>,
) -> Result<ResponseJson<ApiResponse<Project>>, ApiError> {
    Ok(ResponseJson(ApiResponse::success(project)))
}

pub async fn get_project_workflow_association(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Option<WorkflowAssociation>>>, ApiError> {
    let association =
        WorkflowAssociation::find_by_project_id(&deployment.db().pool, project.id).await?;
    Ok(ResponseJson(ApiResponse::success(association)))
}

pub async fn upsert_project_workflow_association(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<UpsertWorkflowAssociation>,
) -> Result<ResponseJson<ApiResponse<WorkflowAssociation>>, ApiError> {
    let association =
        WorkflowAssociation::upsert_for_project(&deployment.db().pool, project.id, &payload)
            .await?;
    Ok(ResponseJson(ApiResponse::success(association)))
}

pub async fn delete_project_workflow_association(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let deleted =
        WorkflowAssociation::delete_for_project(&deployment.db().pool, project.id).await?;
    if deleted == 0 {
        return Err(ApiError::NotFound(format!(
            "No workflow association found for project {}",
            project.id
        )));
    }

    Ok(ResponseJson(ApiResponse::success(())))
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

pub async fn get_project_github_repositories(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<ProjectGitHubRepository>>>, ApiError> {
    let repositories = deployment
        .project()
        .get_repositories(&deployment.db().pool, project.id)
        .await?;

    let git_service = deployment.git();
    let github_repositories = repositories
        .into_iter()
        .filter_map(|repo| match git_service.get_github_repo_info(&repo.path) {
            Ok(info) => Some(ProjectGitHubRepository {
                project_id: project.id,
                project_name: project.name.clone(),
                repo_id: repo.id,
                repo_name: repo.name,
                display_name: repo.display_name,
                path: repo.path.to_string_lossy().to_string(),
                github_owner: info.owner.clone(),
                github_repo_name: info.repo_name.clone(),
                github_full_name: format!("{}/{}", info.owner, info.repo_name),
            }),
            Err(error) => {
                tracing::warn!(
                    "Skipping repo {} ({}) in project {}: failed to resolve GitHub identity: {}",
                    repo.name,
                    repo.path.display(),
                    project.id,
                    error
                );
                None
            }
        })
        .collect();

    Ok(ResponseJson(ApiResponse::success(github_repositories)))
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

/// GET /api/projects/:id/prs - Get one filtered PR overview page for the project.
///
/// Each request returns a single page plus cursor metadata and is cached per page identity.
pub async fn get_project_prs(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<GetProjectPrPageQuery>,
) -> Result<ResponseJson<ApiResponse<ProjectPrPageResponse>>, ApiError> {
    let request = query.normalize(project.id)?;

    // Check cache first
    if let Some(cached) = deployment.pr_cache().get(&request.cache_key).await {
        tracing::debug!("Cache hit for project {} PR overview page", project.id);
        return Ok(ResponseJson(ApiResponse::success(cached)));
    }

    tracing::debug!(
        "Cache miss for project {} PR overview page, fetching from GitHub",
        project.id
    );

    // Fetch fresh data from GitHub
    let response = fetch_project_pr_page_from_github(&project, &deployment, &request).await?;

    // Store in cache
    deployment
        .pr_cache()
        .insert(request.cache_key, response.clone())
        .await;

    Ok(ResponseJson(ApiResponse::success(response)))
}

/// Fetch one PR overview page from the GitHub API after a cache miss.
async fn fetch_project_pr_page_from_github(
    project: &Project,
    deployment: &DeploymentImpl,
    request: &NormalizedProjectPrPageQuery,
) -> Result<ProjectPrPageResponse, ApiError> {
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
        return Ok(ProjectPrPageResponse {
            repos: vec![],
            page: ProjectPrPage {
                limit: request.cache_key.limit,
                next_cursor: None,
                has_more: false,
            },
        });
    }

    if let Some(base_branch) = request.cache_key.base_branch.as_deref()
        && !base_branches.iter().any(|branch| branch == base_branch)
    {
        return Ok(ProjectPrPageResponse {
            repos: vec![],
            page: ProjectPrPage {
                limit: request.cache_key.limit,
                next_cursor: None,
                has_more: false,
            },
        });
    }

    // Get project repositories
    let repositories = deployment
        .project()
        .get_repositories(pool, project.id)
        .await?;

    let git_service = deployment.git();
    let mut all_prs = Vec::new();

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

        let repo_prs = github_client
            .list_open_prs(&repo_info.owner, &repo_info.repo_name)
            .await
            .map_err(|e| {
                map_github_client_error(
                    &format!(
                        "Failed to fetch PRs for {}/{}",
                        repo_info.owner, repo_info.repo_name
                    ),
                    e,
                )
            })?;

        let filtered_repo_prs = repo_prs.into_iter().filter(|pr| {
            let matches_base_branch = request
                .cache_key
                .base_branch
                .as_deref()
                .map(|branch| pr.base_branch == branch)
                .unwrap_or_else(|| base_branches.iter().any(|branch| branch == &pr.base_branch));
            let matches_search = request
                .cache_key
                .search
                .as_deref()
                .map(|search| pr.title.to_lowercase().contains(search))
                .unwrap_or(true);

            matches_base_branch && matches_search
        });

        all_prs.extend(filtered_repo_prs.map(|pr| ProjectPrRecord {
            repo_id: repo.id,
            repo_name: repo.name.clone(),
            display_name: repo.display_name.clone(),
            pr,
        }));
    }

    paginate_project_prs(all_prs, request.cursor.as_ref(), request.cache_key.limit)
}

/// POST /api/projects/:id/prs/invalidate - Invalidate the PR cache for this project.
pub async fn invalidate_project_prs_cache(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    deployment.pr_cache().invalidate(project.id).await;
    tracing::info!("Invalidated PR cache for project {}", project.id);
    Ok(ResponseJson(ApiResponse::success(())))
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

/// GET /api/projects/:id/prs/unresolved-counts - Fetch unresolved comment counts for one loaded overview page.
/// This endpoint only resolves counts for the cached page identity returned by `/prs`.
pub async fn get_project_prs_unresolved_counts(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<GetProjectPrPageQuery>,
) -> Result<ResponseJson<ApiResponse<PrUnresolvedCountsResponse>>, ApiError> {
    let request = query.normalize(project.id)?;
    let cached_page = deployment
        .pr_cache()
        .get(&request.cache_key)
        .await
        .ok_or_else(|| {
            ApiError::BadRequest(
                "PR page must be loaded from /prs before requesting unresolved counts".to_string(),
            )
        })?;

    if cached_page.repos.is_empty() {
        return Ok(ResponseJson(ApiResponse::success(
            PrUnresolvedCountsResponse { counts: vec![] },
        )));
    }

    let pool = &deployment.db().pool;

    // Load GitHub token from settings
    let token = get_github_token(pool)
        .await?
        .ok_or_else(|| ApiError::BadRequest("GitHub token not configured".to_string()))?;

    let github_client = GitHubClient::new(token)
        .map_err(|e| ApiError::Internal(format!("Failed to create GitHub client: {}", e)))?;

    let repositories = deployment
        .project()
        .get_repositories(pool, project.id)
        .await?;
    let repositories_by_id = repositories
        .into_iter()
        .map(|repo| (repo.id, repo))
        .collect::<HashMap<_, _>>();

    let git_service = deployment.git();
    let mut all_counts = Vec::new();

    for repo_page in cached_page.repos {
        let repo = repositories_by_id.get(&repo_page.repo_id).ok_or_else(|| {
            ApiError::Internal(format!(
                "Cached PR page referenced unknown project repository {}",
                repo_page.repo_id
            ))
        })?;

        let repo_info = git_service.get_github_repo_info(&repo.path).map_err(|e| {
            ApiError::Internal(format!(
                "Failed to resolve GitHub repository info for {} ({}): {}",
                repo.name,
                repo.path.display(),
                e
            ))
        })?;

        let pr_numbers = repo_page
            .pull_requests
            .iter()
            .map(|pr| pr.pr.number)
            .collect::<Vec<_>>();

        if pr_numbers.is_empty() {
            continue;
        }

        let unresolved_counts = github_client
            .get_unresolved_thread_counts_batch(&repo_info.owner, &repo_info.repo_name, &pr_numbers)
            .await
            .map_err(|e| {
                map_github_client_error(
                    &format!(
                        "Failed to fetch unresolved thread counts for {}/{}",
                        repo_info.owner, repo_info.repo_name
                    ),
                    e,
                )
            })?;

        for (pr_number, count) in unresolved_counts {
            all_counts.push(PrUnresolvedCount {
                repo_id: repo_page.repo_id,
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
    Ok(ResponseJson(ApiResponse::success(
        MergeQueueCountResponse { count },
    )))
}

/// Response for GET /api/projects/:id/prs/:repoId/:prNumber/threads
#[derive(Debug, Clone, Serialize, TS)]
pub struct PrThreadsResponse {
    pub threads: Vec<UnifiedPrComment>,
}

/// Error type for GET /api/projects/:id/prs/:repoId/:prNumber/threads
#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(tag = "type", rename_all = "snake_case")]
pub enum GetPrThreadsError {
    GithubNotConfigured,
    RepoNotFound,
    PrNotFound,
    GithubAuthFailed,
}

/// GET /api/projects/:id/prs/:repoId/:prNumber/threads - Get PR review threads
///
/// Fetches both general and inline review comments for a specific PR.
pub async fn get_pr_threads(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
    Path((_project_id, repo_id, pr_number)): Path<(Uuid, Uuid, i64)>,
) -> Result<ResponseJson<ApiResponse<PrThreadsResponse, GetPrThreadsError>>, ApiError> {
    let pool = &deployment.db().pool;

    // Look up the repo by ID within this project
    let project_repo =
        match ProjectRepo::find_by_project_and_repo(pool, project.id, repo_id).await? {
            Some(pr) => pr,
            None => {
                return Ok(ResponseJson(ApiResponse::error_with_data(
                    GetPrThreadsError::RepoNotFound,
                )));
            }
        };

    let repo = match Repo::find_by_id(pool, project_repo.repo_id).await? {
        Some(r) => r,
        None => {
            return Ok(ResponseJson(ApiResponse::error_with_data(
                GetPrThreadsError::RepoNotFound,
            )));
        }
    };

    // Get GitHub repo info from the local repo path
    let repo_info = deployment.git().get_github_repo_info(&repo.path)?;

    // Create GitHub service (uses gh CLI)
    let github_service = match GitHubService::new() {
        Ok(svc) => svc,
        Err(_) => {
            return Ok(ResponseJson(ApiResponse::error_with_data(
                GetPrThreadsError::GithubNotConfigured,
            )));
        }
    };

    // Fetch comments from GitHub
    match github_service.get_pr_comments(&repo_info, pr_number).await {
        Ok(comments) => Ok(ResponseJson(ApiResponse::success(PrThreadsResponse {
            threads: comments,
        }))),
        Err(e) => {
            tracing::error!(
                "Failed to fetch PR threads for project {}, repo {}, PR #{}: {}",
                project.id,
                repo_id,
                pr_number,
                e
            );
            match &e {
                GitHubServiceError::GhCliNotInstalled(_) => Ok(ResponseJson(
                    ApiResponse::error_with_data(GetPrThreadsError::GithubNotConfigured),
                )),
                GitHubServiceError::AuthFailed(_) => Ok(ResponseJson(
                    ApiResponse::error_with_data(GetPrThreadsError::GithubAuthFailed),
                )),
                GitHubServiceError::RepoNotFoundOrNoAccess(_) => Ok(ResponseJson(
                    ApiResponse::error_with_data(GetPrThreadsError::RepoNotFound),
                )),
                _ => Err(ApiError::GitHubService(e)),
            }
        }
    }
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
        return Ok(ResponseJson(ApiResponse::success(
            ProjectWorktreesResponse { worktrees: vec![] },
        )));
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

    Ok(ResponseJson(ApiResponse::success(
        ProjectWorktreesResponse {
            worktrees: all_worktrees,
        },
    )))
}

pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let project_id_router = Router::new()
        .route(
            "/",
            get(get_project).put(update_project).delete(delete_project),
        )
        .route("/remote/members", get(get_project_remote_members))
        .route(
            "/workflow-association",
            get(get_project_workflow_association)
                .put(upsert_project_workflow_association)
                .delete(delete_project_workflow_association),
        )
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
        .route("/github-repositories", get(get_project_github_repositories))
        .route("/prs", get(get_project_prs))
        .route("/prs/invalidate", post(invalidate_project_prs_cache))
        .route(
            "/prs/unresolved-counts",
            get(get_project_prs_unresolved_counts),
        )
        .route("/prs/{repo_id}/{pr_number}/threads", get(get_pr_threads))
        .route("/merge-queue-count", get(get_merge_queue_count))
        .route("/workspaces", get(get_project_workspaces))
        .route("/worktrees", get(get_project_worktrees))
        .merge(crate::routes::tasks::project_router(deployment))
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

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use chrono::TimeZone;
    use db::models::{
        project::{CreateProject, Project},
        project_repo::ProjectRepo,
    };
    use git2::Repository;
    use local_deployment::LocalDeployment;
    use serde_json::json;
    use tempfile::TempDir;
    use tower::ServiceExt;

    use super::*;

    fn reset_test_database() {
        let db_path = utils::assets::asset_dir().join("db.sqlite");
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn project_workflow_association_routes_round_trip() {
        let _lock = crate::TEST_DB_LOCK.lock().unwrap();
        reset_test_database();
        let deployment = LocalDeployment::new().await.unwrap();
        let project = Project::create(
            &deployment.db().pool,
            &CreateProject {
                name: "workflow-project".to_string(),
                repositories: vec![],
            },
            Uuid::new_v4(),
        )
        .await
        .unwrap();

        let app = super::router(&deployment).with_state(deployment.clone());

        let update_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/projects/{}/workflow-association", project.id))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "workflow_id": "wf-project",
                            "label": "Repository default",
                            "url": "https://workflows.example/repository-default"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(update_response.status(), StatusCode::OK);

        let get_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/projects/{}/workflow-association", project.id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_response.status(), StatusCode::OK);
        let body = to_bytes(get_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let api_response: ApiResponse<Option<WorkflowAssociation>> =
            serde_json::from_slice(&body).unwrap();
        let data = api_response.into_data().unwrap().unwrap();
        assert_eq!(data.workflow_id, "wf-project");

        let delete_response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/projects/{}/workflow-association", project.id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(delete_response.status(), StatusCode::OK);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn project_github_repositories_route_returns_resolved_owner_repo_pairs() {
        let _lock = crate::TEST_DB_LOCK.lock().unwrap();
        reset_test_database();
        let deployment = LocalDeployment::new().await.unwrap();
        let project = Project::create(
            &deployment.db().pool,
            &CreateProject {
                name: "github-project".to_string(),
                repositories: vec![],
            },
            Uuid::new_v4(),
        )
        .await
        .unwrap();

        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path().join("github-linked-repo");
        std::fs::create_dir_all(&repo_path).unwrap();

        let repository = Repository::init(&repo_path).unwrap();
        repository
            .remote("origin", "https://github.com/acme/widgets.git")
            .unwrap();

        ProjectRepo::add_repo_to_project(
            &deployment.db().pool,
            project.id,
            repo_path.to_str().unwrap(),
            "GitHub Linked Repo",
        )
        .await
        .unwrap();

        let app = super::router(&deployment).with_state(deployment.clone());
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/projects/{}/github-repositories", project.id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let api_response: ApiResponse<Vec<ProjectGitHubRepository>> =
            serde_json::from_slice(&body).unwrap();
        let repositories = api_response.into_data().unwrap();

        assert_eq!(repositories.len(), 1);
        assert_eq!(repositories[0].project_id, project.id);
        assert_eq!(repositories[0].project_name, "github-project");
        assert_eq!(repositories[0].display_name, "GitHub Linked Repo");
        assert_eq!(repositories[0].github_owner, "acme");
        assert_eq!(repositories[0].github_repo_name, "widgets");
        assert_eq!(repositories[0].github_full_name, "acme/widgets");
    }

    fn sample_pr_record(
        repo_id: Uuid,
        repo_name: &str,
        display_name: &str,
        pr_number: u64,
        updated_at: DateTime<Utc>,
        title: &str,
    ) -> ProjectPrRecord {
        ProjectPrRecord {
            repo_id,
            repo_name: repo_name.to_string(),
            display_name: display_name.to_string(),
            pr: PullRequestSummary {
                number: pr_number,
                title: title.to_string(),
                url: format!("https://example.com/pr/{pr_number}"),
                author: "octocat".to_string(),
                head_branch: format!("feature-{pr_number}"),
                base_branch: "main".to_string(),
                created_at: updated_at,
                updated_at,
            },
        }
    }

    #[test]
    fn normalizes_project_pr_query_for_cache_identity() {
        let project_id = Uuid::new_v4();
        let normalized = GetProjectPrPageQuery {
            cursor: None,
            limit: Some(500),
            base_branch: Some(" main ".to_string()),
            search: Some(" Fix Login ".to_string()),
        }
        .normalize(project_id)
        .unwrap();

        assert_eq!(normalized.cache_key.project_id, project_id);
        assert_eq!(normalized.cache_key.limit, MAX_PROJECT_PRS_LIMIT);
        assert_eq!(normalized.cache_key.base_branch.as_deref(), Some("main"));
        assert_eq!(normalized.cache_key.search.as_deref(), Some("fix login"));
        assert!(normalized.cursor.is_none());
    }

    #[test]
    fn rejects_invalid_project_pr_cursor() {
        let error = GetProjectPrPageQuery {
            cursor: Some("not-a-cursor".to_string()),
            limit: None,
            base_branch: None,
            search: None,
        }
        .normalize(Uuid::new_v4())
        .unwrap_err();

        assert!(matches!(error, ApiError::BadRequest(message) if message == "Invalid PR cursor"));
    }

    #[test]
    fn paginates_pull_requests_with_stable_cursor() {
        let first_repo = Uuid::new_v4();
        let second_repo = Uuid::new_v4();
        let newer = Utc.with_ymd_and_hms(2026, 1, 4, 12, 0, 0).unwrap();
        let middle = Utc.with_ymd_and_hms(2026, 1, 3, 12, 0, 0).unwrap();
        let older = Utc.with_ymd_and_hms(2026, 1, 2, 12, 0, 0).unwrap();

        let prs = vec![
            sample_pr_record(first_repo, "repo-a", "Repo A", 11, middle, "Middle"),
            sample_pr_record(second_repo, "repo-b", "Repo B", 12, older, "Older"),
            sample_pr_record(first_repo, "repo-a", "Repo A", 10, newer, "Newest"),
        ];

        let first_page = paginate_project_prs(prs.clone(), None, 2).unwrap();
        assert!(first_page.page.has_more);
        assert_eq!(first_page.repos.len(), 1);
        assert_eq!(first_page.repos[0].pull_requests.len(), 2);
        assert_eq!(first_page.repos[0].pull_requests[0].pr.number, 10);
        assert_eq!(first_page.repos[0].pull_requests[1].pr.number, 11);

        let cursor =
            decode_project_pr_cursor(first_page.page.next_cursor.as_deref().unwrap()).unwrap();
        let second_page = paginate_project_prs(prs, Some(&cursor), 2).unwrap();

        assert!(!second_page.page.has_more);
        assert_eq!(second_page.repos.len(), 1);
        assert_eq!(second_page.repos[0].repo_id, second_repo);
        assert_eq!(second_page.repos[0].pull_requests[0].pr.number, 12);
    }

    #[test]
    fn serializes_paginated_pr_response_shape() {
        let repo_id = Uuid::new_v4();
        let response = ProjectPrPageResponse {
            repos: vec![ProjectRepoPrPage {
                repo_id,
                repo_name: "repo-a".to_string(),
                display_name: "Repo A".to_string(),
                pull_requests: vec![ProjectPrSummary {
                    pr: PullRequestSummary {
                        number: 42,
                        title: "Fix login".to_string(),
                        url: "https://example.com/pr/42".to_string(),
                        author: "octocat".to_string(),
                        head_branch: "feature/fix-login".to_string(),
                        base_branch: "main".to_string(),
                        created_at: Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap(),
                        updated_at: Utc.with_ymd_and_hms(2026, 1, 2, 12, 0, 0).unwrap(),
                    },
                    unresolved_count: None,
                }],
            }],
            page: ProjectPrPage {
                limit: 25,
                next_cursor: Some("cursor".to_string()),
                has_more: true,
            },
        };

        let json = serde_json::to_value(response).unwrap();
        assert_eq!(
            json["page"],
            json!({ "limit": 25, "next_cursor": "cursor", "has_more": true })
        );
        assert_eq!(json["repos"][0]["repo_id"], json!(repo_id));
        assert_eq!(json["repos"][0]["pull_requests"][0]["number"], json!(42));
    }
}
