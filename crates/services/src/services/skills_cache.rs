use std::sync::Arc;

use chrono::{DateTime, Utc};
use executors::executors::claude::SkillsData;
use tokio::sync::RwLock;

/// Global in-memory cache for Claude Code skills data.
///
/// This cache stores the most recent skills data from any Claude Code session,
/// making it available to new/loose conversations without waiting for a session to start.
/// The cache is ephemeral - data is lost on server restart, which is acceptable since
/// skills will be repopulated when the next Claude Code session starts.
#[derive(Clone)]
pub struct GlobalSkillsCache {
    inner: Arc<RwLock<GlobalSkillsCacheInner>>,
}

struct GlobalSkillsCacheInner {
    skills: Option<SkillsData>,
    last_updated: Option<DateTime<Utc>>,
}

impl GlobalSkillsCache {
    /// Create a new empty skills cache.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(GlobalSkillsCacheInner {
                skills: None,
                last_updated: None,
            })),
        }
    }

    /// Update the cached skills data.
    ///
    /// This should be called when a Claude Code session provides skills data
    /// in its init message. The timestamp is automatically set to the current time.
    pub async fn update_skills(&self, skills: SkillsData) {
        let mut inner = self.inner.write().await;
        inner.skills = Some(skills);
        inner.last_updated = Some(Utc::now());
    }

    /// Get the cached skills data, if available.
    ///
    /// Returns `None` if no skills have been cached yet (no Claude Code session
    /// has started since server launch).
    pub async fn get_skills(&self) -> Option<SkillsData> {
        let inner = self.inner.read().await;
        inner.skills.clone()
    }

    /// Get the timestamp of the last skills update, if available.
    pub async fn last_updated(&self) -> Option<DateTime<Utc>> {
        let inner = self.inner.read().await;
        inner.last_updated
    }

    /// Get both skills and last_updated atomically.
    pub async fn get_skills_with_timestamp(&self) -> (Option<SkillsData>, Option<DateTime<Utc>>) {
        let inner = self.inner.read().await;
        (inner.skills.clone(), inner.last_updated)
    }

    /// Check if any skills data has been cached.
    pub async fn has_skills(&self) -> bool {
        let inner = self.inner.read().await;
        inner.skills.is_some()
    }

    /// Get the count of cached skills, or 0 if none cached.
    pub async fn skills_count(&self) -> usize {
        let inner = self.inner.read().await;
        inner
            .skills
            .as_ref()
            .map(|s| s.skills.len())
            .unwrap_or(0)
    }

    /// Get the count of cached slash commands, or 0 if none cached.
    pub async fn slash_commands_count(&self) -> usize {
        let inner = self.inner.read().await;
        inner
            .skills
            .as_ref()
            .map(|s| s.slash_commands.len())
            .unwrap_or(0)
    }
}

impl Default for GlobalSkillsCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use executors::executors::claude::SkillInfo;

    fn make_test_skills_data() -> SkillsData {
        SkillsData {
            slash_commands: vec!["commit".to_string(), "review".to_string()],
            skills: vec![
                SkillInfo {
                    name: "code-review".to_string(),
                    description: Some("Reviews code for issues".to_string()),
                    namespace: Some("dev".to_string()),
                },
                SkillInfo {
                    name: "test-gen".to_string(),
                    description: None,
                    namespace: None,
                },
            ],
        }
    }

    #[tokio::test]
    async fn test_skills_cache_returns_none_when_empty() {
        let cache = GlobalSkillsCache::new();

        assert!(cache.get_skills().await.is_none());
        assert!(cache.last_updated().await.is_none());
        assert!(!cache.has_skills().await);
        assert_eq!(cache.skills_count().await, 0);
        assert_eq!(cache.slash_commands_count().await, 0);
    }

    #[tokio::test]
    async fn test_skills_cache_update_and_get() {
        let cache = GlobalSkillsCache::new();
        let skills = make_test_skills_data();

        // Update the cache
        cache.update_skills(skills.clone()).await;

        // Verify skills were stored
        let retrieved = cache.get_skills().await;
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.slash_commands.len(), 2);
        assert_eq!(retrieved.skills.len(), 2);
        assert_eq!(retrieved.slash_commands[0], "commit");
        assert_eq!(retrieved.skills[0].name, "code-review");

        // Verify timestamp was set
        assert!(cache.last_updated().await.is_some());
        assert!(cache.has_skills().await);
        assert_eq!(cache.skills_count().await, 2);
        assert_eq!(cache.slash_commands_count().await, 2);
    }

    #[tokio::test]
    async fn test_skills_cache_update_overwrites() {
        let cache = GlobalSkillsCache::new();

        // First update
        let skills1 = SkillsData {
            slash_commands: vec!["cmd1".to_string()],
            skills: vec![SkillInfo {
                name: "skill1".to_string(),
                description: None,
                namespace: None,
            }],
        };
        cache.update_skills(skills1).await;
        let ts1 = cache.last_updated().await.unwrap();

        // Small delay to ensure different timestamp
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Second update
        let skills2 = SkillsData {
            slash_commands: vec!["cmd2".to_string(), "cmd3".to_string()],
            skills: vec![],
        };
        cache.update_skills(skills2).await;
        let ts2 = cache.last_updated().await.unwrap();

        // Verify update overwrote previous data
        let retrieved = cache.get_skills().await.unwrap();
        assert_eq!(retrieved.slash_commands.len(), 2);
        assert_eq!(retrieved.skills.len(), 0);
        assert!(ts2 > ts1);
    }

    #[tokio::test]
    async fn test_skills_cache_clone_shares_state() {
        let cache1 = GlobalSkillsCache::new();
        let cache2 = cache1.clone();

        // Update via cache1
        cache1.update_skills(make_test_skills_data()).await;

        // Should be visible via cache2
        assert!(cache2.has_skills().await);
        assert_eq!(cache2.skills_count().await, 2);
    }

    #[tokio::test]
    async fn test_skills_cache_get_with_timestamp() {
        let cache = GlobalSkillsCache::new();

        // Empty cache
        let (skills, ts) = cache.get_skills_with_timestamp().await;
        assert!(skills.is_none());
        assert!(ts.is_none());

        // After update
        cache.update_skills(make_test_skills_data()).await;
        let (skills, ts) = cache.get_skills_with_timestamp().await;
        assert!(skills.is_some());
        assert!(ts.is_some());
    }
}
