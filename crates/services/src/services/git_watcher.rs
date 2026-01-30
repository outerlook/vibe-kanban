//! Git state watcher service
//!
//! Watches git state files to detect changes (branch switches, staging changes, rebases, etc.)
//! instead of expensive polling. For worktrees, resolves the actual `.git` directory.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher, event::EventKind};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{broadcast, mpsc};
use ts_rs::TS;

/// Git state change event types
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum GitStateChangeKind {
    /// HEAD changed (branch switch, checkout)
    Head,
    /// Index changed (staging area)
    Index,
    /// Refs changed (branch/tag create/delete/update)
    Refs,
    /// Rebase in progress
    RebaseHead,
    /// Merge in progress
    MergeHead,
    /// Cherry-pick in progress
    CherryPickHead,
    /// Revert in progress
    RevertHead,
    /// Config changed
    Config,
    /// Unknown change in git directory
    Other,
}

/// A git state change event
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GitStateChange {
    pub kind: GitStateChangeKind,
    /// Relative path within .git directory that changed
    pub path: String,
}

#[derive(Debug, Error)]
pub enum GitWatcherError {
    #[error("Failed to create watcher: {0}")]
    WatcherCreation(#[from] notify::Error),
    #[error("Invalid path: {0}")]
    InvalidPath(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Not a git repository or worktree: {0}")]
    NotGitRepo(String),
}

/// Resolve the actual git directory for a path.
/// For regular repos: returns `path/.git`
/// For worktrees: reads `.git` file and returns the linked directory
pub fn resolve_git_dir(worktree_path: &Path) -> Result<PathBuf, GitWatcherError> {
    let git_path = worktree_path.join(".git");

    if !git_path.exists() {
        return Err(GitWatcherError::NotGitRepo(
            worktree_path.display().to_string(),
        ));
    }

    if git_path.is_dir() {
        // Regular repository
        Ok(git_path)
    } else if git_path.is_file() {
        // Worktree - .git is a file containing path to actual git dir
        let content = std::fs::read_to_string(&git_path)?;
        let gitdir_line = content
            .lines()
            .find(|line| line.starts_with("gitdir:"))
            .ok_or_else(|| {
                GitWatcherError::InvalidPath(format!(
                    "Invalid .git file format at {}",
                    git_path.display()
                ))
            })?;

        let gitdir_path = gitdir_line
            .strip_prefix("gitdir:")
            .map(|s| s.trim())
            .ok_or_else(|| {
                GitWatcherError::InvalidPath(format!(
                    "Invalid gitdir line in {}",
                    git_path.display()
                ))
            })?;

        let resolved = if Path::new(gitdir_path).is_absolute() {
            PathBuf::from(gitdir_path)
        } else {
            worktree_path.join(gitdir_path)
        };

        // Canonicalize to resolve any .. components
        dunce::canonicalize(&resolved).map_err(|e| {
            GitWatcherError::InvalidPath(format!(
                "Failed to canonicalize {}: {}",
                resolved.display(),
                e
            ))
        })
    } else {
        Err(GitWatcherError::NotGitRepo(
            worktree_path.display().to_string(),
        ))
    }
}

/// Classify a changed path within the git directory
fn classify_git_change(relative_path: &Path) -> GitStateChangeKind {
    let path_str = relative_path.to_string_lossy();

    if path_str == "HEAD" {
        GitStateChangeKind::Head
    } else if path_str == "index" || path_str == "index.lock" {
        GitStateChangeKind::Index
    } else if path_str.starts_with("refs/") || path_str == "packed-refs" {
        GitStateChangeKind::Refs
    } else if path_str == "REBASE_HEAD"
        || path_str.starts_with("rebase-merge/")
        || path_str.starts_with("rebase-apply/")
    {
        GitStateChangeKind::RebaseHead
    } else if path_str == "MERGE_HEAD" {
        GitStateChangeKind::MergeHead
    } else if path_str == "CHERRY_PICK_HEAD" {
        GitStateChangeKind::CherryPickHead
    } else if path_str == "REVERT_HEAD" {
        GitStateChangeKind::RevertHead
    } else if path_str == "config" {
        GitStateChangeKind::Config
    } else {
        GitStateChangeKind::Other
    }
}

/// Paths within .git that we care about watching
const GIT_WATCH_PATHS: &[&str] = &[
    "HEAD",
    "index",
    "refs",
    "REBASE_HEAD",
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "rebase-merge",
    "rebase-apply",
    "config",
    "packed-refs",
];

/// Internal watcher state
#[allow(dead_code)]
struct GitWatcherInner {
    /// The git directory being watched
    git_dir: PathBuf,
    /// The original worktree path (for reference)
    worktree_path: PathBuf,
    /// Broadcast sender for events
    tx: broadcast::Sender<Arc<GitStateChange>>,
    /// The underlying notify watcher - kept alive
    _watcher: RecommendedWatcher,
}

/// A subscription to git state changes
pub struct GitWatcherSubscription {
    rx: broadcast::Receiver<Arc<GitStateChange>>,
    git_dir: PathBuf,
    manager: GitWatcherManager,
    _watcher: Arc<GitWatcherInner>,
}

impl GitWatcherSubscription {
    /// Get the git directory being watched
    pub fn git_dir(&self) -> &Path {
        &self.git_dir
    }

    /// Receive the next git state change event.
    /// Returns None if the watcher was dropped.
    pub async fn recv(&mut self) -> Option<Arc<GitStateChange>> {
        loop {
            match self.rx.recv().await {
                Ok(event) => return Some(event),
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(
                        "Git watcher subscription lagged by {} events for {:?}",
                        n,
                        self.git_dir
                    );
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}

impl Drop for GitWatcherSubscription {
    fn drop(&mut self) {
        self.manager.cleanup_if_unused(&self.git_dir);
    }
}

/// Manages shared git watchers across workspaces.
/// Thread-safe and can be cloned cheaply.
#[derive(Clone, Default)]
pub struct GitWatcherManager {
    inner: Arc<GitWatcherManagerInner>,
}

#[derive(Default)]
struct GitWatcherManagerInner {
    /// Map from git directory path to shared watcher
    watchers: RwLock<std::collections::HashMap<PathBuf, std::sync::Weak<GitWatcherInner>>>,
}

impl GitWatcherManager {
    /// Create a new git watcher manager
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribe to git state changes for a worktree path.
    /// Creates a new watcher if one doesn't exist, or returns a subscription to the existing one.
    pub fn subscribe(
        &self,
        worktree_path: PathBuf,
    ) -> Result<GitWatcherSubscription, GitWatcherError> {
        // Resolve the actual git directory
        let git_dir = resolve_git_dir(&worktree_path)?;
        let canonical_git_dir = dunce::canonicalize(&git_dir).unwrap_or_else(|_| git_dir.clone());

        // Fast path: check if watcher already exists
        {
            let watchers = self.inner.watchers.read();
            if let Some(weak) = watchers.get(&canonical_git_dir) {
                if let Some(watcher) = weak.upgrade() {
                    return Ok(GitWatcherSubscription {
                        rx: watcher.tx.subscribe(),
                        git_dir: canonical_git_dir,
                        manager: self.clone(),
                        _watcher: watcher,
                    });
                }
            }
        }

        // Slow path: need to create a new watcher
        let mut watchers = self.inner.watchers.write();

        // Double-check after acquiring write lock
        if let Some(weak) = watchers.get(&canonical_git_dir) {
            if let Some(watcher) = weak.upgrade() {
                return Ok(GitWatcherSubscription {
                    rx: watcher.tx.subscribe(),
                    git_dir: canonical_git_dir,
                    manager: self.clone(),
                    _watcher: watcher,
                });
            }
        }

        // Create new watcher
        let (tx, _) = broadcast::channel::<Arc<GitStateChange>>(256);
        let tx_clone = tx.clone();
        let git_dir_for_handler = canonical_git_dir.clone();

        // Debounce channel for coalescing rapid events
        let (debounce_tx, mut debounce_rx) = mpsc::channel::<PathBuf>(128);

        // Create the notify watcher
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    // Only process modify/create/remove events
                    if !matches!(
                        event.kind,
                        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                    ) {
                        return;
                    }

                    // Send paths to debounce channel
                    for path in event.paths {
                        let _ = debounce_tx.try_send(path);
                    }
                }
            },
            Config::default().with_poll_interval(Duration::from_secs(2)),
        )?;

        // Watch relevant paths within git directory
        for watch_path in GIT_WATCH_PATHS {
            let full_path = canonical_git_dir.join(watch_path);
            if full_path.exists() {
                let mode = if full_path.is_dir() {
                    RecursiveMode::Recursive
                } else {
                    RecursiveMode::NonRecursive
                };
                if let Err(e) = watcher.watch(&full_path, mode) {
                    tracing::debug!("Failed to watch {:?}: {}", full_path, e);
                }
            }
        }

        // Also watch the git dir itself for new files (like REBASE_HEAD appearing)
        if let Err(e) = watcher.watch(&canonical_git_dir, RecursiveMode::NonRecursive) {
            tracing::warn!("Failed to watch git dir {:?}: {}", canonical_git_dir, e);
        }

        let watcher_inner = Arc::new(GitWatcherInner {
            git_dir: canonical_git_dir.clone(),
            worktree_path,
            tx: tx.clone(),
            _watcher: watcher,
        });

        // Spawn debounce task
        let git_dir_for_task = git_dir_for_handler.clone();
        tokio::spawn(async move {
            let mut pending: HashSet<PathBuf> = HashSet::new();
            let debounce_duration = Duration::from_millis(150);

            loop {
                // Wait for events or timeout
                let result = tokio::time::timeout(debounce_duration, debounce_rx.recv()).await;

                match result {
                    Ok(Some(path)) => {
                        // Received a path, add to pending
                        pending.insert(path);
                    }
                    Ok(None) => {
                        // Channel closed, exit
                        break;
                    }
                    Err(_) => {
                        // Timeout - process pending events
                        if pending.is_empty() {
                            continue;
                        }

                        // Deduplicate and emit events
                        let mut emitted_kinds: HashSet<GitStateChangeKind> = HashSet::new();

                        for path in pending.drain() {
                            if let Ok(relative) = path.strip_prefix(&git_dir_for_task) {
                                let kind = classify_git_change(relative);

                                // Skip .lock files and only emit once per kind
                                let path_str = relative.to_string_lossy();
                                if path_str.ends_with(".lock") {
                                    continue;
                                }

                                if emitted_kinds.insert(kind.clone()) {
                                    let event = Arc::new(GitStateChange {
                                        kind,
                                        path: path_str.to_string(),
                                    });

                                    if tx_clone.receiver_count() > 0 {
                                        let _ = tx_clone.send(event);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        watchers.insert(canonical_git_dir.clone(), Arc::downgrade(&watcher_inner));

        Ok(GitWatcherSubscription {
            rx: watcher_inner.tx.subscribe(),
            git_dir: canonical_git_dir,
            manager: self.clone(),
            _watcher: watcher_inner,
        })
    }

    /// Remove a watcher from the map if it has no more subscribers
    fn cleanup_if_unused(&self, git_dir: &Path) {
        let mut watchers = self.inner.watchers.write();
        if let Some(weak) = watchers.get(git_dir) {
            if weak.upgrade().is_none() {
                watchers.remove(git_dir);
                tracing::debug!("Removed unused git watcher for {:?}", git_dir);
            }
        }
    }

    /// Get the number of active watchers (for debugging/metrics)
    pub fn active_watcher_count(&self) -> usize {
        let watchers = self.inner.watchers.read();
        watchers.values().filter(|w| w.upgrade().is_some()).count()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn init_git_repo(path: &Path) {
        // Create minimal .git structure
        let git_dir = path.join(".git");
        fs::create_dir_all(git_dir.join("refs/heads")).unwrap();
        fs::create_dir_all(git_dir.join("objects")).unwrap();
        fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(
            git_dir.join("config"),
            "[core]\n\trepositoryformatversion = 0\n",
        )
        .unwrap();
        // Create empty index
        fs::write(git_dir.join("index"), "").unwrap();
    }

    #[test]
    fn test_resolve_git_dir_regular_repo() {
        let temp_dir = TempDir::new().unwrap();
        init_git_repo(temp_dir.path());

        let result = resolve_git_dir(temp_dir.path()).unwrap();
        assert!(result.ends_with(".git"));
    }

    #[test]
    fn test_resolve_git_dir_worktree() {
        let temp_dir = TempDir::new().unwrap();

        // Create main repo
        let main_repo = temp_dir.path().join("main");
        fs::create_dir_all(&main_repo).unwrap();
        init_git_repo(&main_repo);

        // Create worktree structure
        let worktree = temp_dir.path().join("worktree");
        fs::create_dir_all(&worktree).unwrap();

        let worktree_git_dir = main_repo.join(".git/worktrees/feature");
        fs::create_dir_all(&worktree_git_dir).unwrap();

        // Create .git file in worktree pointing to the worktree git dir
        fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", worktree_git_dir.display()),
        )
        .unwrap();

        let result = resolve_git_dir(&worktree).unwrap();
        assert_eq!(
            dunce::canonicalize(&result).unwrap(),
            dunce::canonicalize(&worktree_git_dir).unwrap()
        );
    }

    #[test]
    fn test_classify_git_change() {
        assert_eq!(
            classify_git_change(Path::new("HEAD")),
            GitStateChangeKind::Head
        );
        assert_eq!(
            classify_git_change(Path::new("index")),
            GitStateChangeKind::Index
        );
        assert_eq!(
            classify_git_change(Path::new("refs/heads/main")),
            GitStateChangeKind::Refs
        );
        assert_eq!(
            classify_git_change(Path::new("REBASE_HEAD")),
            GitStateChangeKind::RebaseHead
        );
        assert_eq!(
            classify_git_change(Path::new("rebase-merge/done")),
            GitStateChangeKind::RebaseHead
        );
        assert_eq!(
            classify_git_change(Path::new("MERGE_HEAD")),
            GitStateChangeKind::MergeHead
        );
        assert_eq!(
            classify_git_change(Path::new("config")),
            GitStateChangeKind::Config
        );
        assert_eq!(
            classify_git_change(Path::new("random_file")),
            GitStateChangeKind::Other
        );
    }

    #[tokio::test]
    async fn test_manager_creates_watcher() {
        let manager = GitWatcherManager::new();
        let temp_dir = TempDir::new().unwrap();
        init_git_repo(temp_dir.path());

        let sub = manager.subscribe(temp_dir.path().to_path_buf()).unwrap();
        assert_eq!(manager.active_watcher_count(), 1);
        assert!(sub.git_dir().ends_with(".git"));
    }

    #[tokio::test]
    async fn test_manager_reuses_watcher() {
        let manager = GitWatcherManager::new();
        let temp_dir = TempDir::new().unwrap();
        init_git_repo(temp_dir.path());

        let _sub1 = manager.subscribe(temp_dir.path().to_path_buf()).unwrap();
        let _sub2 = manager.subscribe(temp_dir.path().to_path_buf()).unwrap();

        // Should have only one active watcher
        assert_eq!(manager.active_watcher_count(), 1);
    }

    #[tokio::test]
    async fn test_watcher_detects_head_change() {
        let manager = GitWatcherManager::new();
        let temp_dir = TempDir::new().unwrap();
        init_git_repo(temp_dir.path());

        let mut sub = manager.subscribe(temp_dir.path().to_path_buf()).unwrap();

        // Give watcher time to initialize
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Modify HEAD
        let head_path = temp_dir.path().join(".git/HEAD");
        fs::write(&head_path, "ref: refs/heads/feature\n").unwrap();

        // Wait for event with timeout
        let result = tokio::time::timeout(Duration::from_secs(2), sub.recv()).await;

        assert!(result.is_ok(), "Should receive an event within timeout");
        if let Ok(Some(event)) = result {
            assert_eq!(event.kind, GitStateChangeKind::Head);
        }
    }

    #[tokio::test]
    async fn test_watcher_detects_index_change() {
        let manager = GitWatcherManager::new();
        let temp_dir = TempDir::new().unwrap();
        init_git_repo(temp_dir.path());

        let mut sub = manager.subscribe(temp_dir.path().to_path_buf()).unwrap();

        // Give watcher time to initialize
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Modify index
        let index_path = temp_dir.path().join(".git/index");
        fs::write(&index_path, "modified content").unwrap();

        // Wait for event with timeout
        let result = tokio::time::timeout(Duration::from_secs(2), sub.recv()).await;

        assert!(result.is_ok(), "Should receive an event within timeout");
        if let Ok(Some(event)) = result {
            assert_eq!(event.kind, GitStateChangeKind::Index);
        }
    }

    #[tokio::test]
    async fn test_debouncing_coalesces_events() {
        let manager = GitWatcherManager::new();
        let temp_dir = TempDir::new().unwrap();
        init_git_repo(temp_dir.path());

        let mut sub = manager.subscribe(temp_dir.path().to_path_buf()).unwrap();

        // Give watcher time to initialize
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Rapidly modify index multiple times
        let index_path = temp_dir.path().join(".git/index");
        for i in 0..5 {
            fs::write(&index_path, format!("content {}", i)).unwrap();
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // Wait for debounced event
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Should receive one coalesced event, not five
        let mut event_count = 0;
        loop {
            let result = tokio::time::timeout(Duration::from_millis(100), sub.recv()).await;
            match result {
                Ok(Some(_)) => event_count += 1,
                _ => break,
            }
        }

        // Should have received just one event due to debouncing
        assert!(
            event_count <= 2,
            "Expected 1-2 events due to debouncing, got {}",
            event_count
        );
    }
}
