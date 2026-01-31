//! Hook execution status tracking types and store.

use std::{collections::HashMap, sync::Arc};

/// Handler names that should be tracked and displayed in the UI.
/// Only user-actionable handlers are included; internal infrastructure
/// handlers (websocket_broadcast, notifications, remote_sync) are excluded.
pub const TRACKED_HANDLERS: &[&str] = &["autopilot", "feedback_collection", "review_attention"];

use chrono::{DateTime, Utc};
use db::models::execution_process::ExecutionProcessStatus;
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
    /// Linked execution process ID for hooks that spawn execution processes.
    /// Used by handlers like `feedback_collection` and `review_attention` that
    /// trigger separate execution processes via ExecutionTrigger callback.
    pub linked_execution_process_id: Option<Uuid>,
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
            linked_execution_process_id: None,
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
    ///
    /// Returns `None` if the handler is not in the `TRACKED_HANDLERS` whitelist,
    /// meaning it won't be tracked or displayed in the UI.
    pub fn start_execution(
        &self,
        task_id: Uuid,
        handler_name: impl Into<String>,
        hook_point: HookPoint,
    ) -> Option<Uuid> {
        let handler_name = handler_name.into();

        // Only track whitelisted handlers
        if !TRACKED_HANDLERS.contains(&handler_name.as_str()) {
            return None;
        }

        let execution = HookExecution::new(task_id, handler_name, hook_point);
        let execution_id = execution.id;

        {
            let mut execs = self.executions.write();
            execs.entry(task_id).or_default().push(execution.clone());
        }

        let patch = hook_execution_patch::add(&execution);
        self.msg_store.push_patch(patch);

        Some(execution_id)
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

    /// Link a hook execution to a spawned execution process.
    /// Used by handlers that trigger separate execution processes (e.g., feedback_collection).
    /// Broadcasts the update via MsgStore.
    pub fn link_execution_process(&self, execution_id: Uuid, process_id: Uuid) {
        let execution = {
            let mut execs = self.executions.write();
            Self::find_and_update(&mut execs, execution_id, |exec| {
                exec.linked_execution_process_id = Some(process_id);
            })
        };

        if let Some(exec) = execution {
            let patch = hook_execution_patch::replace(&exec);
            self.msg_store.push_patch(patch);
        }
    }

    /// Update hook execution status based on linked execution process completion.
    /// Searches by `linked_execution_process_id` and updates status/completed_at.
    /// Broadcasts the update via MsgStore.
    pub fn update_from_execution_process(
        &self,
        process_id: Uuid,
        status: ExecutionProcessStatus,
        completed_at: DateTime<Utc>,
    ) {
        let execution = {
            let mut execs = self.executions.write();
            Self::find_and_update_by_process_id(&mut execs, process_id, |exec| {
                exec.status = match status {
                    ExecutionProcessStatus::Completed => HookExecutionStatus::Completed,
                    ExecutionProcessStatus::Failed | ExecutionProcessStatus::Killed => {
                        HookExecutionStatus::Failed
                    }
                    ExecutionProcessStatus::Running => HookExecutionStatus::Running,
                };
                exec.completed_at = Some(completed_at);
            })
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

    /// Helper to find an execution by linked execution process ID and apply an update function.
    /// Returns the updated execution if found.
    fn find_and_update_by_process_id<F>(
        execs: &mut HashMap<Uuid, Vec<HookExecution>>,
        process_id: Uuid,
        update_fn: F,
    ) -> Option<HookExecution>
    where
        F: FnOnce(&mut HookExecution),
    {
        for task_execs in execs.values_mut() {
            if let Some(exec) = task_execs
                .iter_mut()
                .find(|e| e.linked_execution_process_id == Some(process_id))
            {
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

        let exec_id = store
            .start_execution(task_id, "autopilot", HookPoint::PostTaskStatusChange)
            .expect("autopilot should be tracked");

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

        // Use a tracked handler name
        let exec_id = store
            .start_execution(task_id, "autopilot", HookPoint::PostTaskCreate)
            .expect("autopilot should be tracked");
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

        // Use a tracked handler name
        let exec_id = store
            .start_execution(task_id, "feedback_collection", HookPoint::PostAgentComplete)
            .expect("feedback_collection should be tracked");
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

        // Use tracked handler names
        store.start_execution(task_id, "autopilot", HookPoint::PostTaskCreate);
        store.start_execution(task_id, "feedback_collection", HookPoint::PostTaskStatusChange);

        let execs = store.get_for_task(task_id);
        assert_eq!(execs.len(), 2);
    }

    #[test]
    fn test_store_get_all() {
        let store = create_test_store();
        let task1 = Uuid::new_v4();
        let task2 = Uuid::new_v4();

        // Use tracked handler names
        store.start_execution(task1, "autopilot", HookPoint::PostTaskCreate);
        store.start_execution(task2, "review_attention", HookPoint::PostAgentComplete);

        let all = store.get_all();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_store_clear_completed_for_task() {
        let store = create_test_store();
        let task_id = Uuid::new_v4();

        // Use tracked handler names
        let exec1 = store
            .start_execution(task_id, "autopilot", HookPoint::PostTaskCreate)
            .expect("autopilot should be tracked");
        let exec2 = store
            .start_execution(task_id, "feedback_collection", HookPoint::PostTaskStatusChange)
            .expect("feedback_collection should be tracked");
        let _exec3 = store
            .start_execution(task_id, "review_attention", HookPoint::PostAgentComplete)
            .expect("review_attention should be tracked");

        store.complete_execution(exec1);
        store.fail_execution(exec2, "error");
        // exec3 stays running

        store.clear_completed_for_task(task_id);

        let execs = store.get_for_task(task_id);
        assert_eq!(execs.len(), 1);
        assert_eq!(execs[0].handler_name, "review_attention");
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

    #[test]
    fn test_start_execution_filters_untracked_handlers() {
        let store = create_test_store();
        let task_id = Uuid::new_v4();

        // Non-whitelisted handlers should return None and not be tracked
        let result = store.start_execution(task_id, "websocket_broadcast", HookPoint::PostTaskCreate);
        assert!(result.is_none(), "websocket_broadcast should not be tracked");

        let result = store.start_execution(task_id, "notifications", HookPoint::PostTaskCreate);
        assert!(result.is_none(), "notifications should not be tracked");

        let result = store.start_execution(task_id, "remote_sync", HookPoint::PostTaskCreate);
        assert!(result.is_none(), "remote_sync should not be tracked");

        // No executions should be stored
        let execs = store.get_for_task(task_id);
        assert!(execs.is_empty(), "untracked handlers should not create executions");
    }

    #[test]
    fn test_start_execution_tracks_whitelisted_handlers() {
        let store = create_test_store();
        let task_id = Uuid::new_v4();

        // All whitelisted handlers should be tracked
        let autopilot = store.start_execution(task_id, "autopilot", HookPoint::PostTaskCreate);
        assert!(autopilot.is_some(), "autopilot should be tracked");

        let feedback = store.start_execution(task_id, "feedback_collection", HookPoint::PostTaskStatusChange);
        assert!(feedback.is_some(), "feedback_collection should be tracked");

        let review = store.start_execution(task_id, "review_attention", HookPoint::PostAgentComplete);
        assert!(review.is_some(), "review_attention should be tracked");

        // All three should be stored
        let execs = store.get_for_task(task_id);
        assert_eq!(execs.len(), 3, "all whitelisted handlers should create executions");

        // Verify the handler names
        let names: Vec<_> = execs.iter().map(|e| e.handler_name.as_str()).collect();
        assert!(names.contains(&"autopilot"));
        assert!(names.contains(&"feedback_collection"));
        assert!(names.contains(&"review_attention"));
    }

    #[test]
    fn test_tracked_handlers_constant() {
        // Verify the TRACKED_HANDLERS constant contains expected values
        assert!(TRACKED_HANDLERS.contains(&"autopilot"));
        assert!(TRACKED_HANDLERS.contains(&"feedback_collection"));
        assert!(TRACKED_HANDLERS.contains(&"review_attention"));

        // Verify it does NOT contain infrastructure handlers
        assert!(!TRACKED_HANDLERS.contains(&"websocket_broadcast"));
        assert!(!TRACKED_HANDLERS.contains(&"notifications"));
        assert!(!TRACKED_HANDLERS.contains(&"remote_sync"));
    }

    #[test]
    fn test_link_execution_process() {
        let store = create_test_store();
        let task_id = Uuid::new_v4();
        let process_id = Uuid::new_v4();

        let exec_id = store
            .start_execution(task_id, "feedback_collection", HookPoint::PostAgentComplete)
            .expect("feedback_collection should be tracked");

        // Initially, no linked process
        let execs = store.get_for_task(task_id);
        assert!(execs[0].linked_execution_process_id.is_none());

        // Link the execution process
        store.link_execution_process(exec_id, process_id);

        // Verify the link was set
        let execs = store.get_for_task(task_id);
        assert_eq!(execs[0].linked_execution_process_id, Some(process_id));
    }

    #[test]
    fn test_update_from_execution_process() {
        let store = create_test_store();
        let task_id = Uuid::new_v4();
        let process_id = Uuid::new_v4();

        let exec_id = store
            .start_execution(task_id, "review_attention", HookPoint::PostAgentComplete)
            .expect("review_attention should be tracked");

        // Link and then update from execution process
        store.link_execution_process(exec_id, process_id);

        let completed_at = Utc::now();
        store.update_from_execution_process(
            process_id,
            ExecutionProcessStatus::Completed,
            completed_at,
        );

        // Verify the status and completed_at were updated
        let execs = store.get_for_task(task_id);
        assert_eq!(execs[0].status, HookExecutionStatus::Completed);
        assert_eq!(execs[0].completed_at, Some(completed_at));
    }

    #[test]
    fn test_update_from_execution_process_failed() {
        let store = create_test_store();
        let task_id = Uuid::new_v4();
        let process_id = Uuid::new_v4();

        let exec_id = store
            .start_execution(task_id, "feedback_collection", HookPoint::PostAgentComplete)
            .expect("feedback_collection should be tracked");

        store.link_execution_process(exec_id, process_id);

        let completed_at = Utc::now();
        store.update_from_execution_process(process_id, ExecutionProcessStatus::Failed, completed_at);

        let execs = store.get_for_task(task_id);
        assert_eq!(execs[0].status, HookExecutionStatus::Failed);
        assert_eq!(execs[0].completed_at, Some(completed_at));
    }

    #[test]
    fn test_update_from_execution_process_killed() {
        let store = create_test_store();
        let task_id = Uuid::new_v4();
        let process_id = Uuid::new_v4();

        let exec_id = store
            .start_execution(task_id, "autopilot", HookPoint::PostTaskStatusChange)
            .expect("autopilot should be tracked");

        store.link_execution_process(exec_id, process_id);

        let completed_at = Utc::now();
        store.update_from_execution_process(process_id, ExecutionProcessStatus::Killed, completed_at);

        let execs = store.get_for_task(task_id);
        assert_eq!(execs[0].status, HookExecutionStatus::Failed);
        assert_eq!(execs[0].completed_at, Some(completed_at));
    }

    #[test]
    fn test_update_from_execution_process_no_match() {
        let store = create_test_store();
        let task_id = Uuid::new_v4();
        let unlinked_process_id = Uuid::new_v4();

        let _exec_id = store
            .start_execution(task_id, "autopilot", HookPoint::PostTaskCreate)
            .expect("autopilot should be tracked");

        // Try to update with a process_id that doesn't match any linked execution
        // This should not panic and should be a no-op
        let completed_at = Utc::now();
        store.update_from_execution_process(
            unlinked_process_id,
            ExecutionProcessStatus::Completed,
            completed_at,
        );

        // The execution should remain unchanged (still Running, no completed_at)
        let execs = store.get_for_task(task_id);
        assert_eq!(execs[0].status, HookExecutionStatus::Running);
        assert!(execs[0].completed_at.is_none());
    }

    #[test]
    fn test_new_hook_execution_has_no_linked_process() {
        let task_id = Uuid::new_v4();
        let exec = HookExecution::new(task_id, "autopilot", HookPoint::PostTaskStatusChange);

        assert!(exec.linked_execution_process_id.is_none());
    }
}
