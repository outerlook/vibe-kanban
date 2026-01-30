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

use super::HookPoint;

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
    ExecutionCompleted { process: ExecutionProcess, task_id: Uuid },

    /// A workspace was created.
    WorkspaceCreated { workspace: Workspace },

    /// A workspace was deleted.
    WorkspaceDeleted { workspace_id: Uuid, task_id: Uuid },

    /// A project was updated.
    ProjectUpdated { project: Project },
}

impl DomainEvent {
    /// Returns the task ID associated with this event, if any.
    ///
    /// Some events don't have an associated task directly available:
    /// - `ProjectUpdated`: No task context
    ///
    /// These return `None` and hook tracking will be skipped for them.
    pub fn task_id(&self) -> Option<Uuid> {
        match self {
            DomainEvent::TaskStatusChanged { task, .. } => Some(task.id),
            DomainEvent::ExecutionCompleted { task_id, .. } => Some(*task_id),
            DomainEvent::WorkspaceCreated { workspace } => Some(workspace.task_id),
            DomainEvent::WorkspaceDeleted { task_id, .. } => Some(*task_id),
            DomainEvent::ProjectUpdated { .. } => None,
        }
    }

    /// Returns the hook point associated with this event.
    ///
    /// Maps each event variant to its corresponding post-action hook point.
    pub fn hook_point(&self) -> HookPoint {
        match self {
            DomainEvent::TaskStatusChanged { .. } => HookPoint::PostTaskStatusChange,
            DomainEvent::ExecutionCompleted { .. } => HookPoint::PostAgentComplete,
            DomainEvent::WorkspaceCreated { .. } => HookPoint::PostTaskCreate,
            DomainEvent::WorkspaceDeleted { .. } => HookPoint::PostTaskStatusChange,
            DomainEvent::ProjectUpdated { .. } => HookPoint::PostTaskStatusChange, // Best approximation
        }
    }
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

/// Callback type for dispatching domain events.
///
/// This callback allows services to dispatch domain events without direct access
/// to the `DomainEventDispatcher`. Used by services like `PrMonitorService` and
/// `MergeQueueProcessor` that need to dispatch events from outside the container.
pub type EventDispatchCallback =
    Arc<dyn Fn(DomainEvent) -> BoxFuture<'static, ()> + Send + Sync>;
