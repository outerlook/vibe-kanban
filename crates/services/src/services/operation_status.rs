use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utils::msg_store::MsgStore;
use uuid::Uuid;

use super::events::patches::operation_status_patch;

/// Type of operation being tracked
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum OperationStatusType {
    GeneratingCommit,
    Rebasing,
    Pushing,
    Merging,
}

/// Status of an in-progress operation
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OperationStatus {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub operation_type: OperationStatusType,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
}

impl OperationStatus {
    pub fn new(workspace_id: Uuid, operation_type: OperationStatusType) -> Self {
        Self {
            id: Uuid::new_v4(),
            workspace_id,
            operation_type,
            error: None,
            started_at: Utc::now(),
        }
    }

    pub fn with_error(mut self, error: String) -> Self {
        self.error = Some(error);
        self
    }
}

/// In-memory store for tracking active operations.
/// Operations are ephemeral - lost on server restart (acceptable).
#[derive(Clone)]
pub struct OperationStatusStore {
    /// Active operations keyed by workspace_id
    operations: Arc<RwLock<HashMap<Uuid, OperationStatus>>>,
    /// MsgStore for broadcasting changes via SSE
    msg_store: Arc<MsgStore>,
}

impl OperationStatusStore {
    pub fn new(msg_store: Arc<MsgStore>) -> Self {
        Self {
            operations: Arc::new(RwLock::new(HashMap::new())),
            msg_store,
        }
    }

    /// Set the current operation status for a workspace.
    /// Broadcasts the change via MsgStore.
    pub fn set(&self, status: OperationStatus) {
        let workspace_id = status.workspace_id;
        let is_new = {
            let mut ops = self.operations.write();
            let is_new = !ops.contains_key(&workspace_id);
            ops.insert(workspace_id, status.clone());
            is_new
        };

        let patch = if is_new {
            operation_status_patch::add(&status)
        } else {
            operation_status_patch::replace(&status)
        };
        self.msg_store.push_patch(patch);
    }

    /// Clear the operation status for a workspace.
    /// Broadcasts the removal via MsgStore.
    pub fn clear(&self, workspace_id: Uuid) {
        let removed = self.operations.write().remove(&workspace_id);
        if removed.is_some() {
            let patch = operation_status_patch::remove(workspace_id);
            self.msg_store.push_patch(patch);
        }
    }

    /// Get the current operation status for a workspace.
    pub fn get(&self, workspace_id: Uuid) -> Option<OperationStatus> {
        self.operations.read().get(&workspace_id).cloned()
    }

    /// Get all active operation statuses.
    /// Useful for initial state sync when a client connects.
    pub fn get_all(&self) -> Vec<OperationStatus> {
        self.operations.read().values().cloned().collect()
    }
}
