//! Hook execution status tracking types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::hook_points::HookPoint;

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
}
