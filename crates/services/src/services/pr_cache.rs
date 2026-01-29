use std::time::Duration;

use moka::future::Cache;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::github_client::PullRequestSummary;

/// Default cache TTL in seconds (2 minutes)
const DEFAULT_TTL_SECS: u64 = 120;

/// A pull request with its optional unresolved review thread count.
/// The count may be null if it's being loaded progressively.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct PrWithComments {
    #[serde(flatten)]
    pub pr: PullRequestSummary,
    pub unresolved_count: Option<usize>,
}

/// PRs grouped by repository.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct RepoPrs {
    pub repo_id: Uuid,
    pub repo_name: String,
    pub display_name: String,
    pub pull_requests: Vec<PrWithComments>,
}

/// Cached response for project PRs
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectPrsResponse {
    pub repos: Vec<RepoPrs>,
}

/// Cache for project PR data to reduce GitHub API calls.
///
/// Uses moka's async cache with TTL-based expiration.
pub struct PrCache {
    cache: Cache<Uuid, ProjectPrsResponse>,
}

impl PrCache {
    /// Create a new PR cache with default TTL (2 minutes)
    pub fn new() -> Self {
        Self::with_ttl(Duration::from_secs(DEFAULT_TTL_SECS))
    }

    /// Create a new PR cache with custom TTL
    pub fn with_ttl(ttl: Duration) -> Self {
        let cache = Cache::builder()
            .max_capacity(100) // Max 100 projects
            .time_to_live(ttl)
            .build();

        Self { cache }
    }

    /// Get cached PR data for a project
    pub async fn get(&self, project_id: Uuid) -> Option<ProjectPrsResponse> {
        self.cache.get(&project_id).await
    }

    /// Store PR data in cache
    pub async fn insert(&self, project_id: Uuid, response: ProjectPrsResponse) {
        self.cache.insert(project_id, response).await;
    }

    /// Invalidate cache for a specific project
    pub async fn invalidate(&self, project_id: Uuid) {
        self.cache.invalidate(&project_id).await;
    }

    /// Check if project data is cached
    pub async fn contains(&self, project_id: Uuid) -> bool {
        self.cache.contains_key(&project_id)
    }
}

impl Default for PrCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_insert_and_get() {
        let cache = PrCache::new();
        let project_id = Uuid::new_v4();
        let response = ProjectPrsResponse { repos: vec![] };

        assert!(cache.get(project_id).await.is_none());

        cache.insert(project_id, response.clone()).await;

        let cached = cache.get(project_id).await;
        assert!(cached.is_some());
    }

    #[tokio::test]
    async fn test_cache_invalidate() {
        let cache = PrCache::new();
        let project_id = Uuid::new_v4();
        let response = ProjectPrsResponse { repos: vec![] };

        cache.insert(project_id, response).await;
        assert!(cache.contains(project_id).await);

        cache.invalidate(project_id).await;
        assert!(!cache.contains(project_id).await);
    }

    #[tokio::test]
    async fn test_cache_ttl_expiration() {
        let cache = PrCache::with_ttl(Duration::from_millis(50));
        let project_id = Uuid::new_v4();
        let response = ProjectPrsResponse { repos: vec![] };

        cache.insert(project_id, response).await;
        assert!(cache.get(project_id).await.is_some());

        // Wait for TTL to expire
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Force sync to ensure expiration is processed
        cache.cache.run_pending_tasks().await;

        assert!(cache.get(project_id).await.is_none());
    }
}
