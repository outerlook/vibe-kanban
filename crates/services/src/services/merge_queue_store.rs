use std::sync::Arc;

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utils::msg_store::MsgStore;
use uuid::Uuid;

use super::events::patches::merge_queue_patch;

/// Status of a merge queue entry
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum MergeQueueStatus {
    /// Entry is waiting in queue
    Queued,
    /// Entry is currently being merged
    Merging,
}

/// An entry in the merge queue
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MergeQueueEntry {
    pub id: Uuid,
    pub project_id: Uuid,
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub queued_at: DateTime<Utc>,
    pub status: MergeQueueStatus,
    pub commit_message: String,
}

impl MergeQueueEntry {
    pub fn new(
        project_id: Uuid,
        workspace_id: Uuid,
        repo_id: Uuid,
        commit_message: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            project_id,
            workspace_id,
            repo_id,
            queued_at: Utc::now(),
            status: MergeQueueStatus::Queued,
            commit_message,
        }
    }
}

/// In-memory store for merge queue entries.
/// Entries are ephemeral - lost on server restart (acceptable).
/// Uses workspace_id as primary key since each workspace can only have one queue entry.
#[derive(Clone)]
pub struct MergeQueueStore {
    /// Queue entries keyed by workspace_id
    entries: Arc<RwLock<Vec<MergeQueueEntry>>>,
    /// MsgStore for broadcasting changes via SSE
    msg_store: Arc<MsgStore>,
}

impl MergeQueueStore {
    pub fn new(msg_store: Arc<MsgStore>) -> Self {
        Self {
            entries: Arc::new(RwLock::new(Vec::new())),
            msg_store,
        }
    }

    /// Add an entry to the queue.
    /// Returns the created entry.
    pub fn enqueue(
        &self,
        project_id: Uuid,
        workspace_id: Uuid,
        repo_id: Uuid,
        commit_message: String,
    ) -> MergeQueueEntry {
        let entry = MergeQueueEntry::new(project_id, workspace_id, repo_id, commit_message);

        {
            let mut entries = self.entries.write();
            // Remove any existing entry for this workspace
            entries.retain(|e| e.workspace_id != workspace_id);
            entries.push(entry.clone());
        }

        let patch = merge_queue_patch::add(&entry);
        self.msg_store.push_patch(patch);

        entry
    }

    /// Atomically claim the next Queued entry for a project.
    /// Returns the entry with status updated to Merging, or None if no Queued entries exist.
    /// FIFO ordering: returns the entry with the oldest queued_at timestamp.
    pub fn claim_next(&self, project_id: Uuid) -> Option<MergeQueueEntry> {
        let mut entries = self.entries.write();

        // Find the oldest Queued entry for this project
        let idx = entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.project_id == project_id && e.status == MergeQueueStatus::Queued)
            .min_by_key(|(_, e)| e.queued_at)
            .map(|(idx, _)| idx)?;

        // Update status to Merging
        entries[idx].status = MergeQueueStatus::Merging;
        let entry = entries[idx].clone();

        // Drop lock before broadcasting
        drop(entries);

        let patch = merge_queue_patch::replace(&entry);
        self.msg_store.push_patch(patch);

        Some(entry)
    }

    /// Remove an entry from the queue by workspace_id.
    /// Called when merge completes (success or failure).
    pub fn remove(&self, workspace_id: Uuid) -> Option<MergeQueueEntry> {
        let removed = {
            let mut entries = self.entries.write();
            let idx = entries
                .iter()
                .position(|e| e.workspace_id == workspace_id)?;
            Some(entries.remove(idx))
        };

        if let Some(ref entry) = removed {
            let patch = merge_queue_patch::remove(entry.workspace_id);
            self.msg_store.push_patch(patch);
        }

        removed
    }

    /// Get the queue entry for a workspace.
    pub fn get(&self, workspace_id: Uuid) -> Option<MergeQueueEntry> {
        self.entries
            .read()
            .iter()
            .find(|e| e.workspace_id == workspace_id)
            .cloned()
    }

    /// List all queue entries for a project, ordered by queued_at (oldest first).
    pub fn list_by_project(&self, project_id: Uuid) -> Vec<MergeQueueEntry> {
        let entries = self.entries.read();
        let mut result: Vec<_> = entries
            .iter()
            .filter(|e| e.project_id == project_id)
            .cloned()
            .collect();
        result.sort_by_key(|e| e.queued_at);
        result
    }

    /// Get all queue entries.
    /// Useful for initial state sync when a client connects.
    pub fn get_all(&self) -> Vec<MergeQueueEntry> {
        let mut entries: Vec<_> = self.entries.read().clone();
        entries.sort_by_key(|e| e.queued_at);
        entries
    }

    /// Count entries for a project.
    pub fn count_by_project(&self, project_id: Uuid) -> i64 {
        self.entries
            .read()
            .iter()
            .filter(|e| e.project_id == project_id)
            .count() as i64
    }

    /// Count entries for a set of workspace IDs.
    /// Useful for counting entries belonging to a task group.
    pub fn count_by_workspace_ids(&self, workspace_ids: &[Uuid]) -> i64 {
        self.entries
            .read()
            .iter()
            .filter(|e| workspace_ids.contains(&e.workspace_id))
            .count() as i64
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn create_store() -> MergeQueueStore {
        let msg_store = Arc::new(MsgStore::new());
        MergeQueueStore::new(msg_store)
    }

    #[test]
    fn test_enqueue_and_get() {
        let store = create_store();
        let project_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let repo_id = Uuid::new_v4();

        let entry = store.enqueue(project_id, workspace_id, repo_id, "Test commit".to_string());

        assert_eq!(entry.project_id, project_id);
        assert_eq!(entry.workspace_id, workspace_id);
        assert_eq!(entry.status, MergeQueueStatus::Queued);

        let retrieved = store.get(workspace_id).unwrap();
        assert_eq!(retrieved.id, entry.id);
    }

    #[test]
    fn test_fifo_ordering() {
        let store = create_store();
        let project_id = Uuid::new_v4();
        let repo_id = Uuid::new_v4();

        // Enqueue 3 entries with small delays to ensure different timestamps
        let ws1 = Uuid::new_v4();
        let entry1 = store.enqueue(project_id, ws1, repo_id, "First".to_string());

        std::thread::sleep(Duration::from_millis(10));
        let ws2 = Uuid::new_v4();
        let _entry2 = store.enqueue(project_id, ws2, repo_id, "Second".to_string());

        std::thread::sleep(Duration::from_millis(10));
        let ws3 = Uuid::new_v4();
        let _entry3 = store.enqueue(project_id, ws3, repo_id, "Third".to_string());

        // claim_next should return the oldest entry
        let claimed = store.claim_next(project_id).unwrap();
        assert_eq!(claimed.workspace_id, entry1.workspace_id);
        assert_eq!(claimed.commit_message, "First");
        assert_eq!(claimed.status, MergeQueueStatus::Merging);

        // Verify list_by_project returns in order
        let list = store.list_by_project(project_id);
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].commit_message, "First");
        assert_eq!(list[1].commit_message, "Second");
        assert_eq!(list[2].commit_message, "Third");
    }

    #[test]
    fn test_claim_skips_merging_entries() {
        let store = create_store();
        let project_id = Uuid::new_v4();
        let repo_id = Uuid::new_v4();

        let ws1 = Uuid::new_v4();
        let _entry1 = store.enqueue(project_id, ws1, repo_id, "First".to_string());

        std::thread::sleep(Duration::from_millis(10));
        let ws2 = Uuid::new_v4();
        let entry2 = store.enqueue(project_id, ws2, repo_id, "Second".to_string());

        // Claim the first entry
        let claimed1 = store.claim_next(project_id).unwrap();
        assert_eq!(claimed1.commit_message, "First");

        // Next claim should skip the Merging entry and get the second
        let claimed2 = store.claim_next(project_id).unwrap();
        assert_eq!(claimed2.workspace_id, entry2.workspace_id);
        assert_eq!(claimed2.commit_message, "Second");

        // No more Queued entries
        assert!(store.claim_next(project_id).is_none());
    }

    #[test]
    fn test_remove() {
        let store = create_store();
        let project_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let repo_id = Uuid::new_v4();

        store.enqueue(project_id, workspace_id, repo_id, "Test".to_string());
        assert!(store.get(workspace_id).is_some());

        let removed = store.remove(workspace_id);
        assert!(removed.is_some());
        assert!(store.get(workspace_id).is_none());

        // Removing again should return None
        assert!(store.remove(workspace_id).is_none());
    }

    #[test]
    fn test_project_isolation() {
        let store = create_store();
        let project1 = Uuid::new_v4();
        let project2 = Uuid::new_v4();
        let repo_id = Uuid::new_v4();

        let ws1 = Uuid::new_v4();
        let ws2 = Uuid::new_v4();

        store.enqueue(project1, ws1, repo_id, "Project 1".to_string());
        store.enqueue(project2, ws2, repo_id, "Project 2".to_string());

        let list1 = store.list_by_project(project1);
        let list2 = store.list_by_project(project2);

        assert_eq!(list1.len(), 1);
        assert_eq!(list2.len(), 1);
        assert_eq!(list1[0].commit_message, "Project 1");
        assert_eq!(list2[0].commit_message, "Project 2");

        // claim_next respects project isolation
        let claimed = store.claim_next(project1).unwrap();
        assert_eq!(claimed.workspace_id, ws1);
    }

    #[test]
    fn test_enqueue_replaces_existing_workspace_entry() {
        let store = create_store();
        let project_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let repo_id = Uuid::new_v4();

        store.enqueue(project_id, workspace_id, repo_id, "First".to_string());
        store.enqueue(project_id, workspace_id, repo_id, "Second".to_string());

        let list = store.list_by_project(project_id);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].commit_message, "Second");
    }

    #[test]
    fn test_concurrent_claim() {
        use std::thread;

        let store = create_store();
        let project_id = Uuid::new_v4();
        let repo_id = Uuid::new_v4();

        // Enqueue 10 entries
        let workspace_ids: Vec<_> = (0..10).map(|_| Uuid::new_v4()).collect();
        for (i, ws_id) in workspace_ids.iter().enumerate() {
            store.enqueue(project_id, *ws_id, repo_id, format!("Entry {}", i));
            std::thread::sleep(Duration::from_millis(1));
        }

        // Spawn multiple threads trying to claim
        let store_clone = store.clone();
        let handles: Vec<_> = (0..5)
            .map(|_| {
                let store = store_clone.clone();
                thread::spawn(move || {
                    let mut claimed = Vec::new();
                    while let Some(entry) = store.claim_next(project_id) {
                        claimed.push(entry.workspace_id);
                    }
                    claimed
                })
            })
            .collect();

        // Collect all claimed workspace IDs
        let mut all_claimed: Vec<Uuid> = handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect();
        all_claimed.sort();

        let mut expected = workspace_ids.clone();
        expected.sort();

        // Each entry should be claimed exactly once
        assert_eq!(all_claimed, expected);
    }
}
