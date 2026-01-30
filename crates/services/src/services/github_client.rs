use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Utc};
use octocrab::{Octocrab, params};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
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
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
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

// GraphQL types for PR review threads query
#[derive(Debug, Serialize)]
struct ReviewThreadsQuery {
    query: &'static str,
    variables: ReviewThreadsVariables,
}

#[derive(Debug, Serialize)]
struct ReviewThreadsVariables {
    owner: String,
    repo: String,
    pr: i64,
}

#[derive(Debug, Deserialize)]
struct GraphQLResponse {
    data: Option<GraphQLData>,
    errors: Option<Vec<GraphQLError>>,
}

#[derive(Debug, Deserialize)]
struct GraphQLError {
    message: String,
}

#[derive(Debug, Deserialize)]
struct GraphQLData {
    repository: Option<RepositoryData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryData {
    pull_request: Option<PullRequestData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PullRequestData {
    review_threads: ReviewThreadsConnection,
}

#[derive(Debug, Deserialize)]
struct ReviewThreadsConnection {
    nodes: Vec<ReviewThread>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewThread {
    is_resolved: bool,
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
                    url: pr.html_url.map(|u| u.to_string()).unwrap_or_default(),
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

    /// List open pull requests filtered by head branch.
    ///
    /// Fetches all pages of results for repos with many PRs.
    /// The head_ref parameter should be in the format "owner:branch" (e.g., "usherlabs:FIET-540").
    pub async fn list_open_prs_by_head(
        &self,
        owner: &str,
        repo: &str,
        head_ref: &str,
    ) -> Result<Vec<PullRequestSummary>, GitHubClientError> {
        let mut all_prs = Vec::new();
        let mut page_num = 1u32;

        loop {
            let page = self
                .inner
                .pulls(owner, repo)
                .list()
                .state(params::State::Open)
                .head(head_ref)
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
                    url: pr.html_url.map(|u| u.to_string()).unwrap_or_default(),
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

    /// Get the count of unresolved review threads for a pull request.
    ///
    /// Uses GitHub's GraphQL API to fetch review threads with their resolved status.
    /// Returns 0 for PRs with no review threads.
    pub async fn get_unresolved_thread_count(
        &self,
        owner: &str,
        repo: &str,
        pr_number: u64,
    ) -> Result<usize, GitHubClientError> {
        const QUERY: &str = r#"
            query($owner: String!, $repo: String!, $pr: Int!) {
                repository(owner: $owner, name: $repo) {
                    pullRequest(number: $pr) {
                        reviewThreads(first: 100) {
                            nodes {
                                isResolved
                            }
                        }
                    }
                }
            }
        "#;

        let query = ReviewThreadsQuery {
            query: QUERY,
            variables: ReviewThreadsVariables {
                owner: owner.to_string(),
                repo: repo.to_string(),
                pr: pr_number as i64,
            },
        };

        let response: GraphQLResponse = self
            .inner
            .graphql(&query)
            .await
            .map_err(|e| GitHubClientError::ApiError(e.to_string()))?;

        if let Some(errors) = response.errors
            && !errors.is_empty()
        {
            let error_messages: Vec<_> = errors.iter().map(|e| e.message.as_str()).collect();
            return Err(GitHubClientError::ApiError(error_messages.join(", ")));
        }

        let count = response
            .data
            .and_then(|d| d.repository)
            .and_then(|r| r.pull_request)
            .map(|pr| {
                pr.review_threads
                    .nodes
                    .iter()
                    .filter(|t| !t.is_resolved)
                    .count()
            })
            .unwrap_or(0);

        Ok(count)
    }

    /// Get the count of unresolved review threads for multiple pull requests in a single query.
    ///
    /// Uses GitHub's GraphQL API with aliases to fetch review threads for multiple PRs at once.
    /// Returns a HashMap mapping PR number to unresolved count.
    /// PRs that fail to fetch will have a count of 0.
    pub async fn get_unresolved_thread_counts_batch(
        &self,
        owner: &str,
        repo: &str,
        pr_numbers: &[u64],
    ) -> Result<HashMap<u64, usize>, GitHubClientError> {
        if pr_numbers.is_empty() {
            return Ok(HashMap::new());
        }

        // Build dynamic GraphQL query with aliases for each PR
        let pr_fragments: Vec<String> = pr_numbers
            .iter()
            .map(|num| {
                format!(
                    r#"pr{num}: pullRequest(number: {num}) {{
                        reviewThreads(first: 100) {{
                            nodes {{
                                isResolved
                            }}
                        }}
                    }}"#,
                    num = num
                )
            })
            .collect();

        let query = format!(
            r#"query($owner: String!, $repo: String!) {{
                repository(owner: $owner, name: $repo) {{
                    {fragments}
                }}
            }}"#,
            fragments = pr_fragments.join("\n")
        );

        #[derive(Debug, Serialize)]
        struct BatchQuery {
            query: String,
            variables: BatchVariables,
        }

        #[derive(Debug, Serialize)]
        struct BatchVariables {
            owner: String,
            repo: String,
        }

        let request = BatchQuery {
            query,
            variables: BatchVariables {
                owner: owner.to_string(),
                repo: repo.to_string(),
            },
        };

        let response: JsonValue = self
            .inner
            .graphql(&request)
            .await
            .map_err(|e| GitHubClientError::ApiError(e.to_string()))?;

        // Check for top-level errors
        if let Some(errors) = response.get("errors") {
            if let Some(arr) = errors.as_array() {
                if !arr.is_empty() {
                    let error_messages: Vec<&str> = arr
                        .iter()
                        .filter_map(|e| e.get("message").and_then(|m| m.as_str()))
                        .collect();
                    return Err(GitHubClientError::ApiError(error_messages.join(", ")));
                }
            }
        }

        let mut counts = HashMap::new();

        // Parse each PR's response from the repository data
        if let Some(repo_data) = response
            .get("data")
            .and_then(|d| d.get("repository"))
            .and_then(|r| r.as_object())
        {
            for pr_number in pr_numbers {
                let alias = format!("pr{}", pr_number);
                let count = repo_data
                    .get(&alias)
                    .and_then(|pr| pr.get("reviewThreads"))
                    .and_then(|rt| rt.get("nodes"))
                    .and_then(|nodes| nodes.as_array())
                    .map(|threads| {
                        threads
                            .iter()
                            .filter(|t| {
                                t.get("isResolved")
                                    .and_then(|v| v.as_bool())
                                    .map(|resolved| !resolved)
                                    .unwrap_or(false)
                            })
                            .count()
                    })
                    .unwrap_or(0);

                counts.insert(*pr_number, count);
            }
        } else {
            // If repository data is missing, return 0 for all PRs
            for pr_number in pr_numbers {
                counts.insert(*pr_number, 0);
            }
        }

        Ok(counts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_crypto_provider() {
        // octocrab uses rustls which requires a CryptoProvider to be installed.
        // This is normally done at application startup, but tests run in isolation.
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    }

    #[tokio::test]
    async fn test_github_client_construction() {
        setup_crypto_provider();
        // Test that client construction works with a dummy token
        let result = GitHubClient::new("test_token".to_string());
        assert!(result.is_ok());

        let client = result.unwrap();
        // Verify we can access the inner client
        let _ = client.inner();
    }

    #[tokio::test]
    async fn test_github_client_clone() {
        setup_crypto_provider();
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

    #[test]
    fn test_graphql_response_parsing_with_unresolved_threads() {
        let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": [
                                {"isResolved": false},
                                {"isResolved": true},
                                {"isResolved": false},
                                {"isResolved": true}
                            ]
                        }
                    }
                }
            }
        }"#;

        let response: GraphQLResponse = serde_json::from_str(json).unwrap();
        let count = response
            .data
            .and_then(|d| d.repository)
            .and_then(|r| r.pull_request)
            .map(|pr| {
                pr.review_threads
                    .nodes
                    .iter()
                    .filter(|t| !t.is_resolved)
                    .count()
            })
            .unwrap_or(0);

        assert_eq!(count, 2);
    }

    #[test]
    fn test_graphql_response_parsing_no_threads() {
        let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": []
                        }
                    }
                }
            }
        }"#;

        let response: GraphQLResponse = serde_json::from_str(json).unwrap();
        let count = response
            .data
            .and_then(|d| d.repository)
            .and_then(|r| r.pull_request)
            .map(|pr| {
                pr.review_threads
                    .nodes
                    .iter()
                    .filter(|t| !t.is_resolved)
                    .count()
            })
            .unwrap_or(0);

        assert_eq!(count, 0);
    }

    #[test]
    fn test_graphql_response_parsing_null_pr() {
        let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": null
                }
            }
        }"#;

        let response: GraphQLResponse = serde_json::from_str(json).unwrap();
        let count = response
            .data
            .and_then(|d| d.repository)
            .and_then(|r| r.pull_request)
            .map(|pr| {
                pr.review_threads
                    .nodes
                    .iter()
                    .filter(|t| !t.is_resolved)
                    .count()
            })
            .unwrap_or(0);

        assert_eq!(count, 0);
    }

    #[test]
    fn test_graphql_response_parsing_with_errors() {
        let json = r#"{
            "data": null,
            "errors": [
                {"message": "Could not resolve to a Repository"},
                {"message": "Another error"}
            ]
        }"#;

        let response: GraphQLResponse = serde_json::from_str(json).unwrap();
        assert!(response.errors.is_some());
        assert_eq!(response.errors.unwrap().len(), 2);
    }

    #[tokio::test]
    #[ignore = "Requires valid GitHub token - run with GITHUB_TOKEN env var"]
    async fn test_get_unresolved_thread_count_real_api() {
        let token = std::env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN must be set");
        let client = GitHubClient::new(token).unwrap();

        // Test with a known public repo PR (rust-lang/rust has many PRs with reviews)
        let result = client
            .get_unresolved_thread_count("rust-lang", "rust", 1)
            .await;

        // Should either succeed or fail gracefully
        match result {
            Ok(count) => println!("Unresolved threads: {}", count),
            Err(e) => println!("API error (expected for old/invalid PRs): {}", e),
        }
    }

    #[test]
    fn test_batch_graphql_response_parsing_multiple_prs() {
        // Simulates the batch response format with multiple PRs using aliases
        let json = r#"{
            "data": {
                "repository": {
                    "pr1": {
                        "reviewThreads": {
                            "nodes": [
                                {"isResolved": false},
                                {"isResolved": true}
                            ]
                        }
                    },
                    "pr2": {
                        "reviewThreads": {
                            "nodes": [
                                {"isResolved": false},
                                {"isResolved": false},
                                {"isResolved": false}
                            ]
                        }
                    },
                    "pr3": {
                        "reviewThreads": {
                            "nodes": []
                        }
                    }
                }
            }
        }"#;

        let response: JsonValue = serde_json::from_str(json).unwrap();
        let pr_numbers = vec![1u64, 2u64, 3u64];
        let mut counts = HashMap::new();

        if let Some(repo_data) = response
            .get("data")
            .and_then(|d| d.get("repository"))
            .and_then(|r| r.as_object())
        {
            for pr_number in &pr_numbers {
                let alias = format!("pr{}", pr_number);
                let count = repo_data
                    .get(&alias)
                    .and_then(|pr| pr.get("reviewThreads"))
                    .and_then(|rt| rt.get("nodes"))
                    .and_then(|nodes| nodes.as_array())
                    .map(|threads| {
                        threads
                            .iter()
                            .filter(|t| {
                                t.get("isResolved")
                                    .and_then(|v| v.as_bool())
                                    .map(|resolved| !resolved)
                                    .unwrap_or(false)
                            })
                            .count()
                    })
                    .unwrap_or(0);

                counts.insert(*pr_number, count);
            }
        }

        assert_eq!(counts.get(&1), Some(&1)); // PR 1: 1 unresolved, 1 resolved
        assert_eq!(counts.get(&2), Some(&3)); // PR 2: 3 unresolved
        assert_eq!(counts.get(&3), Some(&0)); // PR 3: no threads
    }

    #[test]
    fn test_batch_graphql_response_parsing_partial_failure() {
        // Simulates a response where some PRs are null (e.g., PR not found)
        let json = r#"{
            "data": {
                "repository": {
                    "pr1": {
                        "reviewThreads": {
                            "nodes": [{"isResolved": false}]
                        }
                    },
                    "pr2": null,
                    "pr3": {
                        "reviewThreads": {
                            "nodes": [{"isResolved": true}]
                        }
                    }
                }
            }
        }"#;

        let response: JsonValue = serde_json::from_str(json).unwrap();
        let pr_numbers = vec![1u64, 2u64, 3u64];
        let mut counts = HashMap::new();

        if let Some(repo_data) = response
            .get("data")
            .and_then(|d| d.get("repository"))
            .and_then(|r| r.as_object())
        {
            for pr_number in &pr_numbers {
                let alias = format!("pr{}", pr_number);
                let count = repo_data
                    .get(&alias)
                    .and_then(|pr| pr.get("reviewThreads"))
                    .and_then(|rt| rt.get("nodes"))
                    .and_then(|nodes| nodes.as_array())
                    .map(|threads| {
                        threads
                            .iter()
                            .filter(|t| {
                                t.get("isResolved")
                                    .and_then(|v| v.as_bool())
                                    .map(|resolved| !resolved)
                                    .unwrap_or(false)
                            })
                            .count()
                    })
                    .unwrap_or(0);

                counts.insert(*pr_number, count);
            }
        }

        assert_eq!(counts.get(&1), Some(&1)); // PR 1: 1 unresolved
        assert_eq!(counts.get(&2), Some(&0)); // PR 2: null returns 0
        assert_eq!(counts.get(&3), Some(&0)); // PR 3: 0 unresolved (1 resolved)
    }

    #[test]
    fn test_batch_graphql_response_parsing_null_repository() {
        // Simulates a response where the repository is null
        let json = r#"{
            "data": {
                "repository": null
            }
        }"#;

        let response: JsonValue = serde_json::from_str(json).unwrap();
        let pr_numbers = vec![1u64, 2u64];
        let mut counts = HashMap::new();

        if let Some(repo_data) = response
            .get("data")
            .and_then(|d| d.get("repository"))
            .and_then(|r| r.as_object())
        {
            for pr_number in &pr_numbers {
                let alias = format!("pr{}", pr_number);
                let count = repo_data
                    .get(&alias)
                    .and_then(|pr| pr.get("reviewThreads"))
                    .and_then(|rt| rt.get("nodes"))
                    .and_then(|nodes| nodes.as_array())
                    .map(|threads| {
                        threads
                            .iter()
                            .filter(|t| {
                                t.get("isResolved")
                                    .and_then(|v| v.as_bool())
                                    .map(|resolved| !resolved)
                                    .unwrap_or(false)
                            })
                            .count()
                    })
                    .unwrap_or(0);

                counts.insert(*pr_number, count);
            }
        } else {
            // Repository is null, return 0 for all PRs
            for pr_number in &pr_numbers {
                counts.insert(*pr_number, 0);
            }
        }

        assert_eq!(counts.get(&1), Some(&0));
        assert_eq!(counts.get(&2), Some(&0));
    }

    #[tokio::test]
    #[ignore = "Requires valid GitHub token - run with GITHUB_TOKEN env var"]
    async fn test_get_unresolved_thread_counts_batch_real_api() {
        let token = std::env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN must be set");
        let client = GitHubClient::new(token).unwrap();

        // Test with known public repo PRs
        let result = client
            .get_unresolved_thread_counts_batch("rust-lang", "rust", &[1, 2, 3])
            .await;

        match result {
            Ok(counts) => {
                println!("Batch unresolved thread counts: {:?}", counts);
                assert_eq!(counts.len(), 3);
            }
            Err(e) => println!("API error (expected for some PRs): {}", e),
        }
    }

    #[tokio::test]
    #[ignore = "Requires valid GitHub token - run with GITHUB_TOKEN env var"]
    async fn test_get_unresolved_thread_counts_batch_empty_input() {
        let token = std::env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN must be set");
        let client = GitHubClient::new(token).unwrap();

        // Test with empty PR list
        let result = client
            .get_unresolved_thread_counts_batch("rust-lang", "rust", &[])
            .await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}
