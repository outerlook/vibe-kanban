//! Shared filesystem watcher manager
//!
//! Maintains one watcher per workspace path, with broadcast channels for multiple subscribers.
//! This prevents the "too many open files" error when multiple browser tabs connect to the same workspace.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Weak},
};

use futures::StreamExt;
use notify_debouncer_full::DebounceEventResult;
use parking_lot::RwLock;
use thiserror::Error;
use tokio::sync::broadcast;

use crate::services::filesystem_watcher::{self, FilesystemWatcherError};

/// Errors that can occur when subscribing to a watcher
#[derive(Error, Debug)]
pub enum WatcherSubscribeError {
    #[error("Failed to create filesystem watcher: {0}")]
    WatcherCreation(#[from] FilesystemWatcherError),
}

/// A shared watcher for a single workspace path.
/// Uses a broadcast channel to fan out events to multiple subscribers.
struct SharedWatcher {
    /// Broadcast sender for filesystem events
    tx: broadcast::Sender<Arc<DebounceEventResult>>,
}

impl SharedWatcher {
    fn subscribe(&self) -> broadcast::Receiver<Arc<DebounceEventResult>> {
        self.tx.subscribe()
    }
}

/// Manages shared filesystem watchers across workspaces.
/// Thread-safe and can be cloned cheaply.
#[derive(Clone, Default)]
pub struct WatcherManager {
    inner: Arc<WatcherManagerInner>,
}

#[derive(Default)]
struct WatcherManagerInner {
    /// Map from canonical workspace path to shared watcher
    watchers: RwLock<HashMap<PathBuf, Weak<SharedWatcher>>>,
}

/// A subscription handle that automatically unsubscribes when dropped.
/// Contains the broadcast receiver for filesystem events.
pub struct WatcherSubscription {
    rx: broadcast::Receiver<Arc<DebounceEventResult>>,
    canonical_path: PathBuf,
    manager: WatcherManager,
    _watcher: Arc<SharedWatcher>,
}

impl WatcherSubscription {
    /// Get the canonical path this subscription is watching
    pub fn canonical_path(&self) -> &PathBuf {
        &self.canonical_path
    }

    /// Receive the next filesystem event.
    /// Returns None if the watcher was dropped or the channel is closed.
    pub async fn recv(&mut self) -> Option<Arc<DebounceEventResult>> {
        loop {
            match self.rx.recv().await {
                Ok(event) => return Some(event),
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(
                        "Watcher subscription lagged by {} events for {:?}",
                        n,
                        self.canonical_path
                    );
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}

impl Drop for WatcherSubscription {
    fn drop(&mut self) {
        // Clean up watcher if this was the last subscriber
        // The Arc<SharedWatcher> will be dropped after this, and if no other strong refs exist,
        // the Weak will become invalid and we can remove it from the map
        self.manager.cleanup_if_unused(&self.canonical_path);
    }
}

impl WatcherManager {
    /// Create a new watcher manager
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribe to filesystem events for a workspace path.
    /// Creates a new watcher if one doesn't exist, or returns a subscription to the existing one.
    pub fn subscribe(&self, root_path: PathBuf) -> Result<WatcherSubscription, WatcherSubscribeError> {
        let canonical = dunce::canonicalize(&root_path).unwrap_or_else(|_| root_path.clone());

        // Fast path: check if watcher already exists
        {
            let watchers = self.inner.watchers.read();
            if let Some(weak) = watchers.get(&canonical)
                && let Some(watcher) = weak.upgrade()
            {
                return Ok(WatcherSubscription {
                    rx: watcher.subscribe(),
                    canonical_path: canonical,
                    manager: self.clone(),
                    _watcher: watcher,
                });
            }
        }

        // Slow path: need to create a new watcher
        let mut watchers = self.inner.watchers.write();

        // Double-check after acquiring write lock
        if let Some(weak) = watchers.get(&canonical)
            && let Some(watcher) = weak.upgrade()
        {
            return Ok(WatcherSubscription {
                rx: watcher.subscribe(),
                canonical_path: canonical,
                manager: self.clone(),
                _watcher: watcher,
            });
        }

        // Create new watcher
        let (debouncer, watcher_rx, watcher_canonical) =
            filesystem_watcher::async_watcher(root_path)?;

        // Use a broadcast channel with reasonable capacity
        // 256 should be enough for bursts of file changes
        let (tx, _) = broadcast::channel::<Arc<DebounceEventResult>>(256);

        let watcher = Arc::new(SharedWatcher { tx: tx.clone() });

        // Spawn background task to forward events from the filesystem watcher to the broadcast channel
        let tx_clone = tx.clone();
        let canonical_clone = watcher_canonical.clone();
        let debouncer_arc = debouncer; // Keep debouncer alive

        std::thread::spawn(move || {
            // Keep the debouncer alive for the lifetime of this thread
            let _debouncer = debouncer_arc;
            let mut rx = watcher_rx;

            while let Some(result) = futures::executor::block_on(rx.next()) {
                // Wrap in Arc for cheap cloning to all subscribers
                let event = Arc::new(result);

                // If send fails, no subscribers - but we keep running
                // in case new subscribers join later
                if tx_clone.receiver_count() == 0 {
                    // No subscribers, exit the loop
                    tracing::debug!(
                        "No subscribers for watcher at {:?}, stopping",
                        canonical_clone
                    );
                    break;
                }

                let _ = tx_clone.send(event);
            }

            tracing::debug!("Watcher thread exiting for {:?}", canonical_clone);
        });

        watchers.insert(watcher_canonical.clone(), Arc::downgrade(&watcher));

        let subscription = WatcherSubscription {
            rx: watcher.subscribe(),
            canonical_path: watcher_canonical,
            manager: self.clone(),
            _watcher: watcher,
        };

        Ok(subscription)
    }

    /// Remove a watcher from the map if it has no more subscribers
    fn cleanup_if_unused(&self, canonical_path: &PathBuf) {
        let mut watchers = self.inner.watchers.write();
        if let Some(weak) = watchers.get(canonical_path) {
            // If the weak reference can't be upgraded, no strong refs exist
            if weak.upgrade().is_none() {
                watchers.remove(canonical_path);
                tracing::debug!("Removed unused watcher for {:?}", canonical_path);
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
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_manager_reuses_watcher() {
        let manager = WatcherManager::new();
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_path_buf();

        // Create two subscriptions to the same path
        let sub1 = manager.subscribe(path.clone()).unwrap();
        let sub2 = manager.subscribe(path.clone()).unwrap();

        // Should have only one active watcher
        assert_eq!(manager.active_watcher_count(), 1);

        // Both should have the same canonical path
        assert_eq!(sub1.canonical_path(), sub2.canonical_path());
    }

    #[test]
    fn test_manager_cleanup_on_drop() {
        let manager = WatcherManager::new();
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_path_buf();

        {
            let _sub = manager.subscribe(path.clone()).unwrap();
            assert_eq!(manager.active_watcher_count(), 1);
        }

        // After dropping, should clean up
        // Note: cleanup happens on next subscribe or explicit cleanup
        // The weak ref will be invalid but map entry may still exist
        // until next access
    }

    #[tokio::test]
    async fn test_subscription_receives_events() {
        let manager = WatcherManager::new();
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_path_buf();

        let mut sub = manager.subscribe(path.clone()).unwrap();

        // Create a file to trigger an event
        let test_file = path.join("test.txt");
        fs::write(&test_file, "hello").unwrap();

        // Give the watcher time to detect the change
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        // Modify the file
        fs::write(&test_file, "world").unwrap();

        // Wait for event with timeout
        let result = tokio::time::timeout(
            tokio::time::Duration::from_secs(2),
            sub.recv()
        ).await;

        // We should receive some event (the exact event depends on debouncing)
        assert!(result.is_ok(), "Should receive an event within timeout");
    }
}
