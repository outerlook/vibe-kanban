use axum::{
    Router,
    extract::{Path, State},
    response::Json as ResponseJson,
    routing::{get, post},
};
use db::models::repo::Repo;
use deployment::Deployment;
use serde::Deserialize;
use services::services::git::GitBranch;
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct RegisterRepoRequest {
    pub path: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct InitRepoRequest {
    pub parent_path: String,
    pub folder_name: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CloneRepoRequest {
    pub url: String,
    pub destination: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateBranchRequest {
    pub name: String,
    pub base_branch: Option<String>,
}

pub async fn register_repo(
    State(deployment): State<DeploymentImpl>,
    ResponseJson(payload): ResponseJson<RegisterRepoRequest>,
) -> Result<ResponseJson<ApiResponse<Repo>>, ApiError> {
    let repo = deployment
        .repo()
        .register(
            &deployment.db().pool,
            &payload.path,
            payload.display_name.as_deref(),
        )
        .await?;

    Ok(ResponseJson(ApiResponse::success(repo)))
}

pub async fn init_repo(
    State(deployment): State<DeploymentImpl>,
    ResponseJson(payload): ResponseJson<InitRepoRequest>,
) -> Result<ResponseJson<ApiResponse<Repo>>, ApiError> {
    let repo = deployment
        .repo()
        .init_repo(
            &deployment.db().pool,
            deployment.git(),
            &payload.parent_path,
            &payload.folder_name,
        )
        .await?;

    Ok(ResponseJson(ApiResponse::success(repo)))
}

pub async fn clone_repo(
    State(deployment): State<DeploymentImpl>,
    ResponseJson(payload): ResponseJson<CloneRepoRequest>,
) -> Result<ResponseJson<ApiResponse<Repo>>, ApiError> {
    let config = deployment.config().read().await;
    let repo = deployment
        .repo()
        .clone_repository(
            &deployment.db().pool,
            &payload.url,
            payload.destination.as_deref(),
            &config,
        )
        .await?;

    Ok(ResponseJson(ApiResponse::success(repo)))
}

pub async fn get_repo_branches(
    State(deployment): State<DeploymentImpl>,
    Path(repo_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<Vec<GitBranch>>>, ApiError> {
    let repo = deployment
        .repo()
        .get_by_id(&deployment.db().pool, repo_id)
        .await?;

    let branches = deployment.git().get_all_branches(&repo.path)?;
    Ok(ResponseJson(ApiResponse::success(branches)))
}

pub async fn create_branch(
    State(deployment): State<DeploymentImpl>,
    Path(repo_id): Path<Uuid>,
    ResponseJson(payload): ResponseJson<CreateBranchRequest>,
) -> Result<ResponseJson<ApiResponse<GitBranch>>, ApiError> {
    let repo = deployment
        .repo()
        .get_by_id(&deployment.db().pool, repo_id)
        .await?;

    let git = deployment.git();

    // Validate branch name
    if !git.is_branch_name_valid(&payload.name) {
        return Err(ApiError::BadRequest(format!(
            "Invalid branch name: {}",
            payload.name
        )));
    }

    // Check if branch already exists
    if git.check_branch_exists(&repo.path, &payload.name)? {
        return Err(ApiError::Conflict(format!(
            "Branch already exists: {}",
            payload.name
        )));
    }

    // Create the branch
    git.create_branch(&repo.path, &payload.name, payload.base_branch.as_deref())?;

    // Get the created branch from the list
    let branches = git.get_all_branches(&repo.path)?;
    let created_branch = branches
        .into_iter()
        .find(|b| b.name == payload.name && !b.is_remote)
        .ok_or_else(|| {
            ApiError::BadRequest("Branch was created but could not be found".to_string())
        })?;

    Ok(ResponseJson(ApiResponse::success(created_branch)))
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/repos", post(register_repo))
        .route("/repos/init", post(init_repo))
        .route("/repos/clone", post(clone_repo))
        .route(
            "/repos/{repo_id}/branches",
            get(get_repo_branches).post(create_branch),
        )
}
