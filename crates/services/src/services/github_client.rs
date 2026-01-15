use std::sync::Arc;

use chrono::{DateTime, Utc};
use octocrab::{Octocrab, params};
use serde::Serialize;
use thiserror::Error;
use ts_rs::TS;

#[derive(Debug, Error)]
pub enum GitHubClientError {
    #[error("Failed to build GitHub client: {0}")]
    BuildError(String),
    #[error("API error: {0}")]
    ApiError(String),
}

/// Summary of a pull request for display purposes.
#[derive(Debug, Clone, Serialize, TS)]
pub struct PullRequestSummary {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub author: String,
    pub head_branch: String,
    pub base_branch: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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

    /// List open pull requests filtered by base branch.
    ///
    /// Fetches all pages of results for repos with many PRs.
    pub async fn list_open_prs_by_base(
        &self,
        owner: &str,
        repo: &str,
        base_branch: &str,
    ) -> Result<Vec<PullRequestSummary>, GitHubClientError> {
        let mut all_prs = Vec::new();
        let mut page_num = 1u32;

        loop {
            let page = self
                .inner
                .pulls(owner, repo)
                .list()
                .state(params::State::Open)
                .base(base_branch)
                .per_page(100)
                .page(page_num)
                .send()
                .await
                .map_err(|e| GitHubClientError::ApiError(e.to_string()))?;

            let items = page.items;
            if items.is_empty() {
                break;
            }

            for pr in items {
                let summary = PullRequestSummary {
                    number: pr.number,
                    title: pr.title.unwrap_or_default(),
                    url: pr
                        .html_url
                        .map(|u| u.to_string())
                        .unwrap_or_default(),
                    author: pr
                        .user
                        .map(|u| u.login)
                        .unwrap_or_else(|| "unknown".to_string()),
                    head_branch: pr.head.ref_field,
                    base_branch: pr.base.ref_field,
                    created_at: pr.created_at.unwrap_or_default(),
                    updated_at: pr.updated_at.unwrap_or_default(),
                };
                all_prs.push(summary);
            }

            // Check if there are more pages
            if page.next.is_none() {
                break;
            }
            page_num += 1;
        }

        Ok(all_prs)
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

    /// Integration test for list_open_prs_by_base.
    /// Requires GITHUB_TOKEN environment variable to be set.
    /// Run with: cargo test -p services -- --ignored test_list_open_prs
    #[tokio::test]
    #[ignore = "requires GITHUB_TOKEN env var"]
    async fn test_list_open_prs_by_base_integration() {
        let token = std::env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN must be set");
        let client = GitHubClient::new(token).expect("client creation failed");

        // Test against rust-lang/rust which usually has many open PRs
        let prs = client
            .list_open_prs_by_base("rust-lang", "rust", "master")
            .await
            .expect("API call failed");

        // Verify we got some results and they have expected fields populated
        for pr in &prs {
            assert!(pr.number > 0);
            assert!(!pr.title.is_empty());
            assert!(pr.url.contains("github.com"));
            assert_eq!(pr.base_branch, "master");
        }
    }

    /// Test that list_open_prs returns empty vec for repo with no PRs to base branch.
    #[tokio::test]
    #[ignore = "requires GITHUB_TOKEN env var"]
    async fn test_list_open_prs_empty_result() {
        let token = std::env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN must be set");
        let client = GitHubClient::new(token).expect("client creation failed");

        // Use a branch that likely has no PRs
        let prs = client
            .list_open_prs_by_base("rust-lang", "rust", "nonexistent-branch-xyz123")
            .await
            .expect("API call failed");

        assert!(prs.is_empty());
    }

    /// Test error handling for invalid owner/repo.
    #[tokio::test]
    #[ignore = "requires GITHUB_TOKEN env var"]
    async fn test_list_open_prs_invalid_repo() {
        let token = std::env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN must be set");
        let client = GitHubClient::new(token).expect("client creation failed");

        let result = client
            .list_open_prs_by_base("nonexistent-owner-xyz", "nonexistent-repo-xyz", "main")
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, GitHubClientError::ApiError(_)));
    }
}
