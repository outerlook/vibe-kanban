//! Hook execution status tracking types and store.

use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utils::msg_store::MsgStore;
use uuid::Uuid;

use super::hook_points::HookPoint;
use crate::services::events::patches::hook_execution_patch;

/// Status of a hook execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum HookExecutionStatus {
    Running,
    Completed,
    Failed,
}

/// Tracks an individual hook execution instance.
///
/// Used by HookExecutionStore to track running/completed hooks
/// and sent to frontend via SSE for real-time status updates.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HookExecution {
    /// Unique identifier for this execution instance.
    pub id: Uuid,
    /// The task this hook execution relates to.
    pub task_id: Uuid,
    /// Name of the handler (e.g., "autopilot", "feedback_collection").
    pub handler_name: String,
    /// The hook point that triggered this execution.
    pub hook_point: HookPoint,
    /// Current status of the execution.
    pub status: HookExecutionStatus,
    /// When the execution started.
    pub started_at: DateTime<Utc>,
    /// When the execution completed (if finished).
    pub completed_at: Option<DateTime<Utc>>,
    /// Error message if the execution failed.
    pub error: Option<String>,
}

impl HookExecution {
    /// Creates a new hook execution in the Running state.
    pub fn new(task_id: Uuid, handler_name: impl Into<String>, hook_point: HookPoint) -> Self {
        Self {
            id: Uuid::new_v4(),
            task_id,
            handler_name: handler_name.into(),
            hook_point,
            status: HookExecutionStatus::Running,
            started_at: Utc::now(),
            completed_at: None,
            error: None,
        }
    }

    /// Marks the execution as completed successfully.
    pub fn set_completed(&mut self) {
        self.status = HookExecutionStatus::Completed;
        self.completed_at = Some(Utc::now());
    }

    /// Marks the execution as failed with an error message.
    pub fn set_failed(&mut self, error: impl Into<String>) {
        self.status = HookExecutionStatus::Failed;
        self.completed_at = Some(Utc::now());
        self.error = Some(error.into());
    }
}

/// In-memory store for tracking active hook executions.
/// Executions are ephemeral - lost on server restart (acceptable).
/// Keyed by task_id to support multiple executions per task.
#[derive(Clone)]
pub struct HookExecutionStore {
    /// Active executions keyed by task_id, each task can have multiple executions
    executions: Arc<RwLock<HashMap<Uuid, Vec<HookExecution>>>>,
    /// MsgStore for broadcasting changes via SSE
    msg_store: Arc<MsgStore>,
}

impl HookExecutionStore {
    pub fn new(msg_store: Arc<MsgStore>) -> Self {
        Self {
            executions: Arc::new(RwLock::new(HashMap::new())),
            msg_store,
        }
    }

    /// Start a new hook execution for a task.
    /// Creates a Running execution, broadcasts it, and returns the execution id.
    pub fn start_execution(
        &self,
        task_id: Uuid,
        handler_name: impl Into<String>,
        hook_point: HookPoint,
    ) -> Uuid {
        let execution = HookExecution::new(task_id, handler_name, hook_point);
        let execution_id = execution.id;

        {
            let mut execs = self.executions.write();
            execs.entry(task_id).or_default().push(execution.clone());
        }

        let patch = hook_execution_patch::add(&execution);
        self.msg_store.push_patch(patch);

        execution_id
    }

    /// Mark an execution as completed successfully.
    /// Broadcasts the update via MsgStore.
    pub fn complete_execution(&self, execution_id: Uuid) {
        let execution = {
            let mut execs = self.executions.write();
            Self::find_and_update(&mut execs, execution_id, |exec| exec.set_completed())
        };

        if let Some(exec) = execution {
            let patch = hook_execution_patch::replace(&exec);
            self.msg_store.push_patch(patch);
        }
    }

    /// Mark an execution as failed with an error message.
    /// Broadcasts the update via MsgStore.
    pub fn fail_execution(&self, execution_id: Uuid, error: impl Into<String>) {
        let error_str = error.into();
        let execution = {
            let mut execs = self.executions.write();
            Self::find_and_update(&mut execs, execution_id, |exec| exec.set_failed(&error_str))
        };

        if let Some(exec) = execution {
            let patch = hook_execution_patch::replace(&exec);
            self.msg_store.push_patch(patch);
        }
    }

    /// Get all executions for a specific task.
    pub fn get_for_task(&self, task_id: Uuid) -> Vec<HookExecution> {
        self.executions
            .read()
            .get(&task_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Get all active executions across all tasks.
    /// Useful for initial state sync when a client connects.
    pub fn get_all(&self) -> Vec<HookExecution> {
        self.executions.read().values().flatten().cloned().collect()
    }

    /// Clear completed (non-Running) executions for a task.
    /// Broadcasts removal patches for each cleared execution.
    pub fn clear_completed_for_task(&self, task_id: Uuid) {
        let removed_ids: Vec<Uuid> = {
            let mut execs = self.executions.write();
            if let Some(task_execs) = execs.get_mut(&task_id) {
                let (completed, running): (Vec<_>, Vec<_>) = task_execs
                    .drain(..)
                    .partition(|e| e.status != HookExecutionStatus::Running);

                *task_execs = running;
                completed.into_iter().map(|e| e.id).collect()
            } else {
                vec![]
            }
        };

        for id in removed_ids {
            let patch = hook_execution_patch::remove(task_id, id);
            self.msg_store.push_patch(patch);
        }
    }

    /// Helper to find an execution across all tasks and apply an update function.
    /// Returns the updated execution if found.
    fn find_and_update<F>(
        execs: &mut HashMap<Uuid, Vec<HookExecution>>,
        execution_id: Uuid,
        update_fn: F,
    ) -> Option<HookExecution>
    where
        F: FnOnce(&mut HookExecution),
    {
        for task_execs in execs.values_mut() {
            if let Some(exec) = task_execs.iter_mut().find(|e| e.id == execution_id) {
                update_fn(exec);
                return Some(exec.clone());
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_hook_execution() {
        let task_id = Uuid::new_v4();
        let exec = HookExecution::new(task_id, "autopilot", HookPoint::PostTaskStatusChange);

        assert_eq!(exec.task_id, task_id);
        assert_eq!(exec.handler_name, "autopilot");
        assert_eq!(exec.hook_point, HookPoint::PostTaskStatusChange);
        assert_eq!(exec.status, HookExecutionStatus::Running);
        assert!(exec.completed_at.is_none());
        assert!(exec.error.is_none());
    }

    #[test]
    fn test_set_completed() {
        let mut exec = HookExecution::new(Uuid::new_v4(), "test", HookPoint::PostTaskCreate);

        exec.set_completed();

        assert_eq!(exec.status, HookExecutionStatus::Completed);
        assert!(exec.completed_at.is_some());
        assert!(exec.error.is_none());
    }

    #[test]
    fn test_set_failed() {
        let mut exec = HookExecution::new(Uuid::new_v4(), "test", HookPoint::PostAgentComplete);

        exec.set_failed("Something went wrong");

        assert_eq!(exec.status, HookExecutionStatus::Failed);
        assert!(exec.completed_at.is_some());
        assert_eq!(exec.error, Some("Something went wrong".to_string()));
    }

    fn create_test_store() -> HookExecutionStore {
        let msg_store = Arc::new(MsgStore::new());
        HookExecutionStore::new(msg_store)
    }

    #[test]
    fn test_store_start_execution() {
        let store = create_test_store();
        let task_id = Uuid::new_v4();

        let exec_id = store.start_execution(task_id, "autopilot", HookPoint::PostTaskStatusChange);

        let execs = store.get_for_task(task_id);
        assert_eq!(execs.len(), 1);
        assert_eq!(execs[0].id, exec_id);
        assert_eq!(execs[0].task_id, task_id);
        assert_eq!(execs[0].handler_name, "autopilot");
        assert_eq!(execs[0].status, HookExecutionStatus::Running);
    }

    #[test]
    fn test_store_complete_execution() {
        let store = create_test_store();
        let task_id = Uuid::new_v4();

        let exec_id = store.start_execution(task_id, "test", HookPoint::PostTaskCreate);
        store.complete_execution(exec_id);

        let execs = store.get_for_task(task_id);
        assert_eq!(execs.len(), 1);
        assert_eq!(execs[0].status, HookExecutionStatus::Completed);
        assert!(execs[0].completed_at.is_some());
    }

    #[test]
    fn test_store_fail_execution() {
        let store = create_test_store();
        let task_id = Uuid::new_v4();

        let exec_id = store.start_execution(task_id, "test", HookPoint::PostAgentComplete);
        store.fail_execution(exec_id, "Test error");

        let execs = store.get_for_task(task_id);
        assert_eq!(execs.len(), 1);
        assert_eq!(execs[0].status, HookExecutionStatus::Failed);
        assert_eq!(execs[0].error, Some("Test error".to_string()));
    }

    #[test]
    fn test_store_multiple_executions_per_task() {
        let store = create_test_store();
        let task_id = Uuid::new_v4();

        store.start_execution(task_id, "handler1", HookPoint::PostTaskCreate);
        store.start_execution(task_id, "handler2", HookPoint::PostTaskStatusChange);

        let execs = store.get_for_task(task_id);
        assert_eq!(execs.len(), 2);
    }

    #[test]
    fn test_store_get_all() {
        let store = create_test_store();
        let task1 = Uuid::new_v4();
        let task2 = Uuid::new_v4();

        store.start_execution(task1, "handler1", HookPoint::PostTaskCreate);
        store.start_execution(task2, "handler2", HookPoint::PostAgentComplete);

        let all = store.get_all();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_store_clear_completed_for_task() {
        let store = create_test_store();
        let task_id = Uuid::new_v4();

        let exec1 = store.start_execution(task_id, "handler1", HookPoint::PostTaskCreate);
        let exec2 = store.start_execution(task_id, "handler2", HookPoint::PostTaskStatusChange);
        let _exec3 = store.start_execution(task_id, "handler3", HookPoint::PostAgentComplete);

        store.complete_execution(exec1);
        store.fail_execution(exec2, "error");
        // exec3 stays running

        store.clear_completed_for_task(task_id);

        let execs = store.get_for_task(task_id);
        assert_eq!(execs.len(), 1);
        assert_eq!(execs[0].handler_name, "handler3");
        assert_eq!(execs[0].status, HookExecutionStatus::Running);
    }

    #[test]
    fn test_store_get_for_nonexistent_task() {
        let store = create_test_store();
        let execs = store.get_for_task(Uuid::new_v4());
        assert!(execs.is_empty());
    }

    #[test]
    fn test_store_complete_nonexistent_execution() {
        let store = create_test_store();
        // Should not panic when trying to complete a non-existent execution
        store.complete_execution(Uuid::new_v4());
    }

    #[test]
    fn test_store_fail_nonexistent_execution() {
        let store = create_test_store();
        // Should not panic when trying to fail a non-existent execution
        store.fail_execution(Uuid::new_v4(), "error");
    }
}
