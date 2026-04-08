use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use moka::future::Cache;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use ts_rs::TS;
use uuid::Uuid;

use super::github_client::PullRequestSummary;

/// Default cache TTL in seconds (2 minutes)
const DEFAULT_TTL_SECS: u64 = 120;

/// A single PR entry returned for a paginated overview page.
/// The unresolved count may be null until that same page's counts are loaded.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectPrSummary {
    #[serde(flatten)]
    pub pr: PullRequestSummary,
    pub unresolved_count: Option<usize>,
}

/// PR entries for one repository within a paginated overview page.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectRepoPrPage {
    pub repo_id: Uuid,
    pub repo_name: String,
    pub display_name: String,
    pub pull_requests: Vec<ProjectPrSummary>,
}

/// Cached response for one project PR overview page.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectPrPageResponse {
    pub repos: Vec<ProjectRepoPrPage>,
    pub page: ProjectPrPage,
}

/// Pagination metadata for a PR overview page.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectPrPage {
    pub limit: usize,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

/// Cache key for a specific project PR page request.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProjectPrPageCacheKey {
    pub project_id: Uuid,
    pub cursor: Option<String>,
    pub limit: usize,
    pub base_branch: Option<String>,
    pub search: Option<String>,
}

/// Cache for project PR data to reduce GitHub API calls.
///
/// Uses moka's async cache with TTL-based expiration.
pub struct PrCache {
    cache: Cache<ProjectPrPageCacheKey, ProjectPrPageResponse>,
    keys_by_project: RwLock<HashMap<Uuid, HashSet<ProjectPrPageCacheKey>>>,
}

impl PrCache {
    /// Create a new PR cache with default TTL (2 minutes)
    pub fn new() -> Self {
        Self::with_ttl(Duration::from_secs(DEFAULT_TTL_SECS))
    }

    /// Create a new PR cache with custom TTL
    pub fn with_ttl(ttl: Duration) -> Self {
        let cache = Cache::builder()
            .max_capacity(500) // Max 500 cached project PR pages
            .time_to_live(ttl)
            .build();

        Self {
            cache,
            keys_by_project: RwLock::new(HashMap::new()),
        }
    }

    /// Get cached PR data for a specific overview page.
    pub async fn get(&self, key: &ProjectPrPageCacheKey) -> Option<ProjectPrPageResponse> {
        self.cache.get(key).await
    }

    /// Store PR data for a specific overview page.
    pub async fn insert(&self, key: ProjectPrPageCacheKey, response: ProjectPrPageResponse) {
        self.cache.insert(key.clone(), response).await;
        self.keys_by_project
            .write()
            .await
            .entry(key.project_id)
            .or_default()
            .insert(key);
    }

    /// Invalidate all cached overview pages for a specific project.
    pub async fn invalidate(&self, project_id: Uuid) {
        let keys = self.keys_by_project.write().await.remove(&project_id);
        if let Some(keys) = keys {
            for key in keys {
                self.cache.invalidate(&key).await;
            }
        }
    }

    /// Check if a specific overview page is cached.
    pub async fn contains(&self, key: &ProjectPrPageCacheKey) -> bool {
        self.cache.contains_key(key)
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
        let key = ProjectPrPageCacheKey {
            project_id,
            cursor: None,
            limit: 25,
            base_branch: None,
            search: None,
        };
        let response = ProjectPrPageResponse {
            repos: vec![],
            page: ProjectPrPage {
                limit: 25,
                next_cursor: None,
                has_more: false,
            },
        };

        assert!(cache.get(&key).await.is_none());

        cache.insert(key.clone(), response.clone()).await;

        let cached = cache.get(&key).await;
        assert_eq!(cached.unwrap().page.limit, response.page.limit);
    }

    #[tokio::test]
    async fn test_cache_invalidate() {
        let cache = PrCache::new();
        let project_id = Uuid::new_v4();
        let other_project_id = Uuid::new_v4();
        let key = ProjectPrPageCacheKey {
            project_id,
            cursor: None,
            limit: 25,
            base_branch: None,
            search: None,
        };
        let other_key = ProjectPrPageCacheKey {
            project_id: other_project_id,
            cursor: Some("cursor".to_string()),
            limit: 10,
            base_branch: Some("main".to_string()),
            search: Some("bugfix".to_string()),
        };
        let response = ProjectPrPageResponse {
            repos: vec![],
            page: ProjectPrPage {
                limit: 25,
                next_cursor: None,
                has_more: false,
            },
        };

        cache.insert(key.clone(), response.clone()).await;
        cache.insert(other_key.clone(), response).await;
        assert!(cache.contains(&key).await);
        assert!(cache.contains(&other_key).await);

        cache.invalidate(project_id).await;
        assert!(!cache.contains(&key).await);
        assert!(cache.contains(&other_key).await);
    }

    #[tokio::test]
    async fn test_cache_keys_include_request_identity() {
        let cache = PrCache::new();
        let project_id = Uuid::new_v4();
        let first_key = ProjectPrPageCacheKey {
            project_id,
            cursor: None,
            limit: 25,
            base_branch: None,
            search: None,
        };
        let second_key = ProjectPrPageCacheKey {
            project_id,
            cursor: Some("next-page".to_string()),
            limit: 25,
            base_branch: None,
            search: None,
        };
        let response = ProjectPrPageResponse {
            repos: vec![],
            page: ProjectPrPage {
                limit: 25,
                next_cursor: Some("next-page".to_string()),
                has_more: true,
            },
        };

        cache.insert(first_key.clone(), response.clone()).await;

        assert!(cache.get(&first_key).await.is_some());
        assert!(cache.get(&second_key).await.is_none());
    }

    #[tokio::test]
    async fn test_cache_ttl_expiration() {
        let cache = PrCache::with_ttl(Duration::from_millis(50));
        let project_id = Uuid::new_v4();
        let key = ProjectPrPageCacheKey {
            project_id,
            cursor: None,
            limit: 25,
            base_branch: None,
            search: None,
        };
        let response = ProjectPrPageResponse {
            repos: vec![],
            page: ProjectPrPage {
                limit: 25,
                next_cursor: None,
                has_more: false,
            },
        };

        cache.insert(key.clone(), response).await;
        assert!(cache.get(&key).await.is_some());

        // Wait for TTL to expire
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Force sync to ensure expiration is processed
        cache.cache.run_pending_tasks().await;

        assert!(cache.get(&key).await.is_none());
    }
}
