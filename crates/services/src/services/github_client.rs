use std::sync::Arc;

use octocrab::Octocrab;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitHubClientError {
    #[error("Failed to build GitHub client: {0}")]
    BuildError(String),
}

/// GitHub API client wrapper using octocrab.
///
/// This provides direct GitHub API access as an alternative to the `gh` CLI-based
/// `GitHubService`. Use this for more reliable programmatic access to GitHub APIs.
#[derive(Clone)]
pub struct GitHubClient {
    inner: Arc<Octocrab>,
}

impl GitHubClient {
    /// Create a new GitHub client with a personal access token.
    pub fn new(token: String) -> Result<Self, GitHubClientError> {
        let octocrab = Octocrab::builder()
            .personal_token(token)
            .build()
            .map_err(|e| GitHubClientError::BuildError(e.to_string()))?;

        Ok(Self {
            inner: Arc::new(octocrab),
        })
    }

    /// Get a reference to the underlying octocrab client for advanced usage.
    pub fn inner(&self) -> &Octocrab {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_github_client_construction() {
        // Test that client construction works with a dummy token
        let result = GitHubClient::new("test_token".to_string());
        assert!(result.is_ok());

        let client = result.unwrap();
        // Verify we can access the inner client
        let _ = client.inner();
    }

    #[test]
    fn test_github_client_clone() {
        let client = GitHubClient::new("test_token".to_string()).unwrap();
        let cloned = client.clone();

        // Both should point to the same underlying Arc
        assert!(Arc::ptr_eq(&client.inner, &cloned.inner));
    }
}
