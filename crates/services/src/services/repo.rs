use std::path::{Path, PathBuf};

use db::models::repo::Repo as RepoModel;
use sqlx::SqlitePool;
use thiserror::Error;
use utils::path::expand_tilde;
use uuid::Uuid;

use super::{
    config::Config,
    git::{GitCli, GitCliError, GitService, GitServiceError},
};

#[derive(Debug, Error)]
pub enum RepoError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Path does not exist: {0}")]
    PathNotFound(PathBuf),
    #[error("Path is not a directory: {0}")]
    PathNotDirectory(PathBuf),
    #[error("Path is not a git repository: {0}")]
    NotGitRepository(PathBuf),
    #[error("Repository not found")]
    NotFound,
    #[error("Directory already exists: {0}")]
    DirectoryAlreadyExists(PathBuf),
    #[error("Git error: {0}")]
    Git(#[from] GitServiceError),
    #[error("Git CLI error: {0}")]
    GitCli(#[from] GitCliError),
    #[error("Invalid folder name: {0}")]
    InvalidFolderName(String),
    #[error("Invalid repository URL: {0}")]
    InvalidUrl(String),
    #[error("No clone directory configured and no destination specified")]
    NoCloneDirectory,
}

pub type Result<T> = std::result::Result<T, RepoError>;

/// Normalize a GitHub repository URL to a canonical HTTPS format.
///
/// Supports three input formats:
/// - HTTPS: `https://github.com/org/repo` or `https://github.com/org/repo.git`
/// - SSH: `git@github.com:org/repo` or `git@github.com:org/repo.git`
/// - Shorthand: `org/repo`
///
/// Returns the normalized URL in the form `https://github.com/org/repo.git`
pub fn normalize_github_url(url: &str) -> Result<String> {
    let url = url.trim();

    // Handle SSH format: git@github.com:org/repo[.git]
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        let path = rest.strip_suffix(".git").unwrap_or(rest);
        return validate_org_repo_format(path).map(|_| format!("https://github.com/{path}.git"));
    }

    // Handle HTTPS format: https://github.com/org/repo[.git]
    if let Some(rest) = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
    {
        let path = rest.strip_suffix(".git").unwrap_or(rest);
        return validate_org_repo_format(path).map(|_| format!("https://github.com/{path}.git"));
    }

    // Handle shorthand format: org/repo
    if url.contains('/') && !url.contains(':') && !url.starts_with("http") {
        return validate_org_repo_format(url).map(|_| format!("https://github.com/{url}.git"));
    }

    Err(RepoError::InvalidUrl(format!(
        "Unrecognized URL format: {url}. Expected HTTPS (https://github.com/org/repo), SSH (git@github.com:org/repo), or shorthand (org/repo)"
    )))
}

/// Validate that a path is in the form `org/repo` (exactly two non-empty segments)
fn validate_org_repo_format(path: &str) -> Result<()> {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() != 2 || parts.iter().any(|p| p.is_empty()) {
        return Err(RepoError::InvalidUrl(format!(
            "Invalid org/repo format: {path}. Expected exactly 'org/repo'"
        )));
    }
    Ok(())
}

/// Extract the repository name from a normalized GitHub URL.
/// E.g., `https://github.com/org/my-repo.git` -> `my-repo`
fn extract_repo_name(url: &str) -> Option<String> {
    url.trim_end_matches(".git")
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

#[derive(Clone, Default)]
pub struct RepoService;

impl RepoService {
    pub fn new() -> Self {
        Self
    }

    pub fn validate_git_repo_path(&self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Err(RepoError::PathNotFound(path.to_path_buf()));
        }

        if !path.is_dir() {
            return Err(RepoError::PathNotDirectory(path.to_path_buf()));
        }

        if !path.join(".git").exists() {
            return Err(RepoError::NotGitRepository(path.to_path_buf()));
        }

        Ok(())
    }

    pub fn normalize_path(&self, path: &str) -> std::io::Result<PathBuf> {
        std::path::absolute(expand_tilde(path))
    }

    pub async fn register(
        &self,
        pool: &SqlitePool,
        path: &str,
        display_name: Option<&str>,
    ) -> Result<RepoModel> {
        let normalized_path = self.normalize_path(path)?;
        self.validate_git_repo_path(&normalized_path)?;

        let name = normalized_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unnamed".to_string());

        let display_name = display_name.unwrap_or(&name);

        let repo = RepoModel::find_or_create(pool, &normalized_path, display_name).await?;
        Ok(repo)
    }

    pub async fn find_by_id(&self, pool: &SqlitePool, repo_id: Uuid) -> Result<Option<RepoModel>> {
        let repo = RepoModel::find_by_id(pool, repo_id).await?;
        Ok(repo)
    }

    pub async fn get_by_id(&self, pool: &SqlitePool, repo_id: Uuid) -> Result<RepoModel> {
        self.find_by_id(pool, repo_id)
            .await?
            .ok_or(RepoError::NotFound)
    }

    pub async fn init_repo(
        &self,
        pool: &SqlitePool,
        git: &GitService,
        parent_path: &str,
        folder_name: &str,
    ) -> Result<RepoModel> {
        if folder_name.is_empty()
            || folder_name.contains('/')
            || folder_name.contains('\\')
            || folder_name == "."
            || folder_name == ".."
        {
            return Err(RepoError::InvalidFolderName(folder_name.to_string()));
        }

        let normalized_parent = self.normalize_path(parent_path)?;
        if !normalized_parent.exists() {
            return Err(RepoError::PathNotFound(normalized_parent));
        }
        if !normalized_parent.is_dir() {
            return Err(RepoError::PathNotDirectory(normalized_parent));
        }

        let repo_path = normalized_parent.join(folder_name);
        if repo_path.exists() {
            return Err(RepoError::DirectoryAlreadyExists(repo_path));
        }

        git.initialize_repo_with_main_branch(&repo_path)?;

        let repo = RepoModel::find_or_create(pool, &repo_path, folder_name).await?;
        Ok(repo)
    }

    /// Clone a repository from a URL and register it.
    ///
    /// # Arguments
    /// * `pool` - Database connection pool
    /// * `url` - Repository URL (HTTPS, SSH, or org/repo shorthand)
    /// * `destination` - Optional destination directory. If None, uses config's default_clone_directory
    /// * `config` - Application config for default_clone_directory
    ///
    /// # Returns
    /// The registered repository model
    pub async fn clone_repository(
        &self,
        pool: &SqlitePool,
        url: &str,
        destination: Option<&str>,
        config: &Config,
    ) -> Result<RepoModel> {
        let normalized_url = normalize_github_url(url)?;
        let repo_name = extract_repo_name(&normalized_url).ok_or_else(|| {
            RepoError::InvalidUrl("Could not extract repository name".to_string())
        })?;

        // Determine destination directory
        let dest_path = match destination {
            Some(dest) => self.normalize_path(dest)?,
            None => {
                let clone_dir = config
                    .default_clone_directory
                    .as_ref()
                    .ok_or(RepoError::NoCloneDirectory)?;
                let normalized_clone_dir = self.normalize_path(clone_dir)?;
                normalized_clone_dir.join(&repo_name)
            }
        };

        // Check if destination already exists
        if dest_path.exists() {
            return Err(RepoError::DirectoryAlreadyExists(dest_path));
        }

        // Ensure parent directory exists
        if let Some(parent) = dest_path.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent)?;
        }

        // Clone the repository
        let git_cli = GitCli::new();
        git_cli.clone(&normalized_url, &dest_path)?;

        // Register the cloned repository
        let repo = RepoModel::find_or_create(pool, &dest_path, &repo_name).await?;
        Ok(repo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_github_url_https() {
        // Without .git suffix
        assert_eq!(
            normalize_github_url("https://github.com/org/repo").unwrap(),
            "https://github.com/org/repo.git"
        );
        // With .git suffix
        assert_eq!(
            normalize_github_url("https://github.com/org/repo.git").unwrap(),
            "https://github.com/org/repo.git"
        );
        // HTTP (should also work)
        assert_eq!(
            normalize_github_url("http://github.com/org/repo").unwrap(),
            "https://github.com/org/repo.git"
        );
    }

    #[test]
    fn test_normalize_github_url_ssh() {
        // Without .git suffix
        assert_eq!(
            normalize_github_url("git@github.com:org/repo").unwrap(),
            "https://github.com/org/repo.git"
        );
        // With .git suffix
        assert_eq!(
            normalize_github_url("git@github.com:org/repo.git").unwrap(),
            "https://github.com/org/repo.git"
        );
    }

    #[test]
    fn test_normalize_github_url_shorthand() {
        assert_eq!(
            normalize_github_url("org/repo").unwrap(),
            "https://github.com/org/repo.git"
        );
        // With whitespace
        assert_eq!(
            normalize_github_url("  org/repo  ").unwrap(),
            "https://github.com/org/repo.git"
        );
    }

    #[test]
    fn test_normalize_github_url_invalid() {
        // No slash
        assert!(normalize_github_url("invalid").is_err());
        // Too many segments
        assert!(normalize_github_url("org/repo/extra").is_err());
        // Empty segments
        assert!(normalize_github_url("/repo").is_err());
        assert!(normalize_github_url("org/").is_err());
        // Random URL
        assert!(normalize_github_url("https://gitlab.com/org/repo").is_err());
    }

    #[test]
    fn test_extract_repo_name() {
        assert_eq!(
            extract_repo_name("https://github.com/org/my-repo.git"),
            Some("my-repo".to_string())
        );
        assert_eq!(
            extract_repo_name("https://github.com/org/repo"),
            Some("repo".to_string())
        );
        assert_eq!(extract_repo_name(""), None);
    }
}
