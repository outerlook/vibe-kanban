use std::{
    collections::VecDeque,
    sync::{Arc, RwLock},
};

use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use ts_rs::TS;

/// 100 MB limit for history buffer
const HISTORY_BYTES: usize = 100000 * 1024;

/// A single server log entry captured from tracing.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ServerLogEntry {
    pub timestamp: DateTime<Utc>,
    /// Log level: "TRACE", "DEBUG", "INFO", "WARN", "ERROR"
    pub level: String,
    /// Module path (e.g., "server::routes::tasks")
    pub target: String,
    pub message: String,
}

impl ServerLogEntry {
    /// Approximate size in bytes for memory accounting.
    pub fn approx_bytes(&self) -> usize {
        const OVERHEAD: usize = 32; // DateTime + enum discriminants + struct overhead
        OVERHEAD + self.level.len() + self.target.len() + self.message.len()
    }
}

#[derive(Clone)]
struct StoredEntry {
    entry: ServerLogEntry,
    bytes: usize,
}

struct Inner {
    history: VecDeque<StoredEntry>,
    total_bytes: usize,
}

/// In-memory store for server log entries with ring buffer and broadcast.
///
/// Follows the same pattern as `MsgStore` - maintains a bounded history
/// and broadcasts new entries to live subscribers.
pub struct ServerLogStore {
    inner: RwLock<Inner>,
    sender: broadcast::Sender<ServerLogEntry>,
}

impl Default for ServerLogStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerLogStore {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(10_000);
        Self {
            inner: RwLock::new(Inner {
                history: VecDeque::with_capacity(32),
                total_bytes: 0,
            }),
            sender,
        }
    }

    /// Push a log entry to the store, evicting oldest entries if over limit.
    pub fn push(&self, entry: ServerLogEntry) {
        let _ = self.sender.send(entry.clone());
        let bytes = entry.approx_bytes();

        let mut inner = self.inner.write().unwrap();
        while inner.total_bytes.saturating_add(bytes) > HISTORY_BYTES {
            if let Some(front) = inner.history.pop_front() {
                inner.total_bytes = inner.total_bytes.saturating_sub(front.bytes);
            } else {
                break;
            }
        }
        inner.history.push_back(StoredEntry { entry, bytes });
        inner.total_bytes = inner.total_bytes.saturating_add(bytes);
    }

    /// Get a snapshot of the current history.
    pub fn get_history(&self) -> Vec<ServerLogEntry> {
        self.inner
            .read()
            .unwrap()
            .history
            .iter()
            .map(|s| s.entry.clone())
            .collect()
    }

    /// Subscribe to live log entries.
    pub fn subscribe(&self) -> broadcast::Receiver<ServerLogEntry> {
        self.sender.subscribe()
    }

    /// Returns a stream that first yields all history, then live entries.
    pub fn history_plus_stream(
        self: &Arc<Self>,
    ) -> futures::stream::BoxStream<'static, Result<ServerLogEntry, std::io::Error>> {
        let (history, rx) = (self.get_history(), self.subscribe());

        let hist = futures::stream::iter(history.into_iter().map(Ok::<_, std::io::Error>));
        let live = BroadcastStream::new(rx)
            .filter_map(|res| async move { res.ok().map(Ok::<_, std::io::Error>) });

        Box::pin(hist.chain(live))
    }
}
