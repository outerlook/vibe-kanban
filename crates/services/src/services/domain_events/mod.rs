mod handler;
pub mod handlers;

pub use handler::{EventHandler, ExecutionMode, HandlerContext, HandlerError};
pub use handlers::{FeedbackCollectionHandler, RemoteSyncHandler};

use db::models::{
    execution_process::ExecutionProcess, project::Project, task::Task, workspace::Workspace,
};
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
