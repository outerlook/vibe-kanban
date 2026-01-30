//! Domain event types for the hook system.
//!
//! These events represent significant state changes in the application
//! that handlers can respond to.

use std::sync::Arc;

use db::models::{
    execution_process::ExecutionProcess, project::Project, task::Task, workspace::Workspace,
};
use futures::future::BoxFuture;
use uuid::Uuid;

/// Domain events that can trigger handler execution.
///
/// These events represent significant state changes in the system
/// that handlers may want to react to.
#[derive(Debug, Clone)]
pub enum DomainEvent {
    /// A task's status changed.
    TaskStatusChanged {
        task: Task,
        previous_status: db::models::task::TaskStatus,
    },

    /// An execution process completed (success or failure).
    ExecutionCompleted { process: ExecutionProcess },

    /// A workspace was created.
    WorkspaceCreated { workspace: Workspace },

    /// A workspace was deleted.
    WorkspaceDeleted { workspace_id: Uuid, task_id: Uuid },

    /// A project was updated.
    ProjectUpdated { project: Project },
}

/// Triggers that handlers can return to request execution starts.
///
/// This allows handlers to request new executions without direct
/// access to ContainerService, enabling loose coupling in the
/// callback-based execution system.
#[derive(Debug, Clone)]
pub enum ExecutionTrigger {
    /// Trigger execution for feedback collection from a workspace.
    FeedbackCollection {
        workspace_id: Uuid,
        task_id: Uuid,
        execution_process_id: Uuid,
    },

    /// Trigger execution when a task needs review attention.
    ReviewAttention {
        task_id: Uuid,
        execution_process_id: Uuid,
    },
}

/// Callback type for triggering executions from handlers.
///
/// This callback is injected into `HandlerContext` to allow handlers
/// to request new executions without direct access to `ContainerService`.
/// The callback accepts an `ExecutionTrigger` and returns a future that
/// resolves when the execution has been triggered (not completed).
pub type ExecutionTriggerCallback =
    Arc<dyn Fn(ExecutionTrigger) -> BoxFuture<'static, Result<(), anyhow::Error>> + Send + Sync>;
