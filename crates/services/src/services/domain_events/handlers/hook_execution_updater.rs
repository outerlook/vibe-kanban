//! Handler for updating hook execution status when linked execution processes complete.
//!
//! This handler listens to `ExecutionCompleted` events and updates the corresponding
//! hook execution's status and `completed_at` timestamp to reflect the actual execution
//! duration rather than the handler's quick "fire-and-forget" completion.

use async_trait::async_trait;

use crate::services::domain_events::{DomainEvent, EventHandler, ExecutionMode, HandlerContext, HandlerError};

/// Updates hook execution status when linked execution processes complete.
///
/// When handlers like `feedback_collection` or `review_attention` trigger new
/// execution processes, those processes are linked to hook executions. This handler
/// monitors `ExecutionCompleted` events and updates the linked hook execution's
/// status and completion time to match the actual execution process outcome.
pub struct HookExecutionUpdaterHandler;

impl HookExecutionUpdaterHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for HookExecutionUpdaterHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventHandler for HookExecutionUpdaterHandler {
    fn name(&self) -> &'static str {
        "hook_execution_updater"
    }

    fn execution_mode(&self) -> ExecutionMode {
        // Inline mode - we don't want this handler tracked as a hook execution itself,
        // and it only does a quick in-memory update
        ExecutionMode::Inline
    }

    fn handles(&self, event: &DomainEvent) -> bool {
        matches!(event, DomainEvent::ExecutionCompleted { .. })
    }

    async fn handle(&self, event: DomainEvent, ctx: &HandlerContext) -> Result<(), HandlerError> {
        let DomainEvent::ExecutionCompleted { process, .. } = event else {
            return Ok(());
        };

        // Only update if we have a hook execution store
        let Some(store) = &ctx.hook_execution_store else {
            return Ok(());
        };

        // Only update if the process has completed (has a completed_at timestamp)
        let Some(completed_at) = process.completed_at else {
            return Ok(());
        };

        // Update the hook execution that has this process linked
        // This is a no-op if no hook execution has this process linked
        store.update_from_execution_process(process.id, process.status, completed_at);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::Utc;
    use db::models::execution_process::{
        ExecutionProcess, ExecutionProcessRunReason, ExecutionProcessStatus, ExecutorActionField,
    };
    use serde_json::json;
    use sqlx::types::Json;
    use tokio::sync::RwLock;
    use utils::msg_store::MsgStore;
    use uuid::Uuid;

    use super::*;
    use crate::services::{
        config::Config,
        domain_events::{HookExecutionStatus, HookExecutionStore, HookPoint},
    };

    fn test_context_with_store(store: HookExecutionStore) -> HandlerContext {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect_lazy("sqlite::memory:")
            .unwrap();
        let db = db::DBService { pool };
        let config = Arc::new(RwLock::new(Config::default()));
        let msg_store = Arc::new(MsgStore::default());
        HandlerContext::new(db, config, msg_store, None).with_hook_execution_store(store)
    }

    fn test_context_without_store() -> HandlerContext {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect_lazy("sqlite::memory:")
            .unwrap();
        let db = db::DBService { pool };
        let config = Arc::new(RwLock::new(Config::default()));
        let msg_store = Arc::new(MsgStore::default());
        HandlerContext::new(db, config, msg_store, None)
    }

    fn create_test_store() -> HookExecutionStore {
        let msg_store = Arc::new(MsgStore::new());
        HookExecutionStore::new(msg_store)
    }

    fn create_completed_execution_process(status: ExecutionProcessStatus) -> ExecutionProcess {
        ExecutionProcess {
            id: Uuid::new_v4(),
            session_id: Some(Uuid::new_v4()),
            conversation_session_id: None,
            run_reason: ExecutionProcessRunReason::CodingAgent,
            executor_action: Json(ExecutorActionField::Other(json!({}))),
            status,
            exit_code: None,
            dropped: false,
            input_tokens: None,
            output_tokens: None,
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn create_running_execution_process() -> ExecutionProcess {
        ExecutionProcess {
            id: Uuid::new_v4(),
            session_id: Some(Uuid::new_v4()),
            conversation_session_id: None,
            run_reason: ExecutionProcessRunReason::CodingAgent,
            executor_action: Json(ExecutorActionField::Other(json!({}))),
            status: ExecutionProcessStatus::Running,
            exit_code: None,
            dropped: false,
            input_tokens: None,
            output_tokens: None,
            started_at: Utc::now(),
            completed_at: None, // Still running
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_handler_properties() {
        let handler = HookExecutionUpdaterHandler::new();

        assert_eq!(handler.name(), "hook_execution_updater");
        assert_eq!(handler.execution_mode(), ExecutionMode::Inline);
    }

    #[test]
    fn test_handles_execution_completed_events() {
        let handler = HookExecutionUpdaterHandler::new();

        let process = create_completed_execution_process(ExecutionProcessStatus::Completed);
        let event = DomainEvent::ExecutionCompleted {
            process,
            task_id: Uuid::new_v4(),
        };

        assert!(handler.handles(&event));
    }

    #[test]
    fn test_does_not_handle_task_status_changed() {
        use db::models::task::{Task, TaskStatus};

        let handler = HookExecutionUpdaterHandler::new();

        let task = Task {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            title: "Test".to_string(),
            description: None,
            status: TaskStatus::Done,
            parent_workspace_id: None,
            shared_task_id: None,
            task_group_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            is_blocked: false,
            has_in_progress_attempt: false,
            last_attempt_failed: false,
            is_queued: false,
            last_executor: String::new(),
            needs_attention: None,
        };

        let event = DomainEvent::TaskStatusChanged {
            task,
            previous_status: TaskStatus::InProgress,
        };

        assert!(!handler.handles(&event));
    }

    #[tokio::test]
    async fn test_updates_hook_status_when_execution_completes() {
        let handler = HookExecutionUpdaterHandler::new();
        let store = create_test_store();
        let task_id = Uuid::new_v4();

        // Start a hook execution and link it to an execution process
        let hook_exec_id = store
            .start_execution(task_id, "feedback_collection", HookPoint::PostAgentComplete)
            .expect("feedback_collection should be tracked");

        let process = create_completed_execution_process(ExecutionProcessStatus::Completed);
        let process_id = process.id;
        let completed_at = process.completed_at.unwrap();

        // Link the execution process to the hook execution
        store.link_execution_process(hook_exec_id, process_id);

        // Verify hook is still running
        let execs = store.get_for_task(task_id);
        assert_eq!(execs[0].status, HookExecutionStatus::Running);

        // Create context with the store and handle the event
        let ctx = test_context_with_store(store.clone());
        let event = DomainEvent::ExecutionCompleted { process, task_id };

        let result = handler.handle(event, &ctx).await;
        assert!(result.is_ok());

        // Verify hook status was updated
        let execs = store.get_for_task(task_id);
        assert_eq!(execs.len(), 1);
        assert_eq!(execs[0].status, HookExecutionStatus::Completed);
        assert_eq!(execs[0].completed_at, Some(completed_at));
    }

    #[tokio::test]
    async fn test_updates_hook_status_to_failed_when_execution_fails() {
        let handler = HookExecutionUpdaterHandler::new();
        let store = create_test_store();
        let task_id = Uuid::new_v4();

        // Start a hook execution and link it to an execution process
        let hook_exec_id = store
            .start_execution(task_id, "review_attention", HookPoint::PostAgentComplete)
            .expect("review_attention should be tracked");

        let process = create_completed_execution_process(ExecutionProcessStatus::Failed);
        let process_id = process.id;
        let completed_at = process.completed_at.unwrap();

        store.link_execution_process(hook_exec_id, process_id);

        let ctx = test_context_with_store(store.clone());
        let event = DomainEvent::ExecutionCompleted { process, task_id };

        let result = handler.handle(event, &ctx).await;
        assert!(result.is_ok());

        // Verify hook status was updated to failed
        let execs = store.get_for_task(task_id);
        assert_eq!(execs[0].status, HookExecutionStatus::Failed);
        assert_eq!(execs[0].completed_at, Some(completed_at));
    }

    #[tokio::test]
    async fn test_updates_hook_status_to_failed_when_execution_killed() {
        let handler = HookExecutionUpdaterHandler::new();
        let store = create_test_store();
        let task_id = Uuid::new_v4();

        let hook_exec_id = store
            .start_execution(task_id, "autopilot", HookPoint::PostTaskStatusChange)
            .expect("autopilot should be tracked");

        let process = create_completed_execution_process(ExecutionProcessStatus::Killed);
        let process_id = process.id;
        let completed_at = process.completed_at.unwrap();

        store.link_execution_process(hook_exec_id, process_id);

        let ctx = test_context_with_store(store.clone());
        let event = DomainEvent::ExecutionCompleted { process, task_id };

        let result = handler.handle(event, &ctx).await;
        assert!(result.is_ok());

        // Killed executions should be marked as failed
        let execs = store.get_for_task(task_id);
        assert_eq!(execs[0].status, HookExecutionStatus::Failed);
        assert_eq!(execs[0].completed_at, Some(completed_at));
    }

    #[tokio::test]
    async fn test_no_op_when_no_hook_execution_store() {
        let handler = HookExecutionUpdaterHandler::new();
        let ctx = test_context_without_store();

        let process = create_completed_execution_process(ExecutionProcessStatus::Completed);
        let event = DomainEvent::ExecutionCompleted {
            process,
            task_id: Uuid::new_v4(),
        };

        // Should succeed without doing anything
        let result = handler.handle(event, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_no_op_when_no_linked_hook_execution() {
        let handler = HookExecutionUpdaterHandler::new();
        let store = create_test_store();
        let task_id = Uuid::new_v4();

        // Start a hook execution but don't link it to any process
        let _hook_exec_id = store
            .start_execution(task_id, "feedback_collection", HookPoint::PostAgentComplete)
            .expect("feedback_collection should be tracked");

        // Create an execution process with a different ID
        let process = create_completed_execution_process(ExecutionProcessStatus::Completed);

        let ctx = test_context_with_store(store.clone());
        let event = DomainEvent::ExecutionCompleted { process, task_id };

        let result = handler.handle(event, &ctx).await;
        assert!(result.is_ok());

        // Hook should still be running since it wasn't linked to the completed process
        let execs = store.get_for_task(task_id);
        assert_eq!(execs[0].status, HookExecutionStatus::Running);
    }

    #[tokio::test]
    async fn test_no_op_when_process_has_no_completed_at() {
        let handler = HookExecutionUpdaterHandler::new();
        let store = create_test_store();
        let task_id = Uuid::new_v4();

        let hook_exec_id = store
            .start_execution(task_id, "feedback_collection", HookPoint::PostAgentComplete)
            .expect("feedback_collection should be tracked");

        // Process that's still running (no completed_at)
        let process = create_running_execution_process();
        let process_id = process.id;

        store.link_execution_process(hook_exec_id, process_id);

        let ctx = test_context_with_store(store.clone());
        let event = DomainEvent::ExecutionCompleted { process, task_id };

        let result = handler.handle(event, &ctx).await;
        assert!(result.is_ok());

        // Hook should still be running
        let execs = store.get_for_task(task_id);
        assert_eq!(execs[0].status, HookExecutionStatus::Running);
    }

    #[tokio::test]
    async fn test_handles_wrong_event_type_gracefully() {
        use db::models::task::{Task, TaskStatus};

        let handler = HookExecutionUpdaterHandler::new();
        let store = create_test_store();

        let task = Task {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            title: "Test".to_string(),
            description: None,
            status: TaskStatus::Done,
            parent_workspace_id: None,
            shared_task_id: None,
            task_group_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            is_blocked: false,
            has_in_progress_attempt: false,
            last_attempt_failed: false,
            is_queued: false,
            last_executor: String::new(),
            needs_attention: None,
        };

        let event = DomainEvent::TaskStatusChanged {
            task,
            previous_status: TaskStatus::InProgress,
        };

        let ctx = test_context_with_store(store);

        // Should return Ok even for wrong event type
        let result = handler.handle(event, &ctx).await;
        assert!(result.is_ok());
    }
}
