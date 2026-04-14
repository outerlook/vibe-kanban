//! Domain event types for the hook system.
//!
//! These events represent significant state changes in the application
//! that handlers can respond to.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use db::models::{
    execution_process::ExecutionProcess, project::Project, task::Task, workspace::Workspace,
};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::HookPoint;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainEventEntityIds {
    pub task_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub execution_process_id: Option<Uuid>,
    pub task_group_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalEventKind {
    ToolApproval,
    UserQuestion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalResolution {
    Approved,
    Denied,
    Answered,
    TimedOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationMessageEventRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeQueueTransitionState {
    Queued,
    Claimed,
    Completed,
    Conflict,
    Removed,
    Skipped,
}

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
    ExecutionCompleted {
        process: ExecutionProcess,
        task_id: Uuid,
        workspace_id: Option<Uuid>,
        task_group_id: Option<Uuid>,
    },

    /// A workspace was created.
    WorkspaceCreated { workspace: Workspace },

    /// A workspace was deleted.
    WorkspaceDeleted { workspace_id: Uuid, task_id: Uuid },

    /// A project was updated.
    ProjectUpdated { project: Project },

    /// A tool approval or user-question prompt was created.
    ApprovalRequested {
        approval_id: String,
        kind: ApprovalEventKind,
        tool_call_id: String,
        tool_name: Option<String>,
        question_count: Option<usize>,
        entity_ids: DomainEventEntityIds,
        occurred_at: DateTime<Utc>,
    },

    /// An approval or question was resolved.
    ApprovalResolved {
        approval_id: String,
        resolution: ApprovalResolution,
        tool_call_id: Option<String>,
        entity_ids: DomainEventEntityIds,
        occurred_at: DateTime<Utc>,
    },

    /// A compact conversation message lifecycle event.
    ConversationMessageAdded {
        conversation_session_id: Uuid,
        message_id: Uuid,
        execution_process_id: Option<Uuid>,
        role: ConversationMessageEventRole,
        entity_ids: DomainEventEntityIds,
        occurred_at: DateTime<Utc>,
    },

    /// A merge queue entry changed state.
    MergeQueueTransition {
        entry_id: Uuid,
        project_id: Uuid,
        workspace_id: Uuid,
        task_id: Option<Uuid>,
        task_group_id: Option<Uuid>,
        repo_id: Uuid,
        state: MergeQueueTransitionState,
        merge_commit: Option<String>,
        detail: Option<String>,
        occurred_at: DateTime<Utc>,
    },

    /// All tasks in a group reached a terminal state.
    TaskGroupCompleted {
        project_id: Uuid,
        task_group_id: Uuid,
        completed_task_ids: Vec<Uuid>,
        terminal_task_count: usize,
        occurred_at: DateTime<Utc>,
    },
}

impl DomainEvent {
    pub fn entity_ids(&self) -> DomainEventEntityIds {
        match self {
            DomainEvent::TaskStatusChanged { task, .. } => DomainEventEntityIds {
                task_id: Some(task.id),
                workspace_id: task.parent_workspace_id,
                session_id: None,
                execution_process_id: None,
                task_group_id: task.task_group_id,
            },
            DomainEvent::ExecutionCompleted {
                process,
                task_id,
                workspace_id,
                task_group_id,
            } => DomainEventEntityIds {
                task_id: Some(*task_id),
                workspace_id: *workspace_id,
                session_id: process.session_id,
                execution_process_id: Some(process.id),
                task_group_id: *task_group_id,
            },
            DomainEvent::WorkspaceCreated { workspace } => DomainEventEntityIds {
                task_id: Some(workspace.task_id),
                workspace_id: Some(workspace.id),
                session_id: None,
                execution_process_id: None,
                task_group_id: None,
            },
            DomainEvent::WorkspaceDeleted {
                workspace_id,
                task_id,
            } => DomainEventEntityIds {
                task_id: Some(*task_id),
                workspace_id: Some(*workspace_id),
                session_id: None,
                execution_process_id: None,
                task_group_id: None,
            },
            DomainEvent::ProjectUpdated { .. } => DomainEventEntityIds::default(),
            DomainEvent::ApprovalRequested { entity_ids, .. }
            | DomainEvent::ApprovalResolved { entity_ids, .. }
            | DomainEvent::ConversationMessageAdded { entity_ids, .. } => *entity_ids,
            DomainEvent::MergeQueueTransition {
                workspace_id,
                task_id,
                task_group_id,
                ..
            } => DomainEventEntityIds {
                task_id: *task_id,
                workspace_id: Some(*workspace_id),
                session_id: None,
                execution_process_id: None,
                task_group_id: *task_group_id,
            },
            DomainEvent::TaskGroupCompleted { task_group_id, .. } => DomainEventEntityIds {
                task_id: None,
                workspace_id: None,
                session_id: None,
                execution_process_id: None,
                task_group_id: Some(*task_group_id),
            },
        }
    }

    /// Returns the task ID associated with this event, if any.
    pub fn task_id(&self) -> Option<Uuid> {
        self.entity_ids().task_id
    }

    pub fn workspace_id(&self) -> Option<Uuid> {
        self.entity_ids().workspace_id
    }

    pub fn session_id(&self) -> Option<Uuid> {
        self.entity_ids().session_id
    }

    pub fn execution_process_id(&self) -> Option<Uuid> {
        self.entity_ids().execution_process_id
    }

    pub fn task_group_id(&self) -> Option<Uuid> {
        self.entity_ids().task_group_id
    }

    pub fn event_name(&self) -> &'static str {
        match self {
            DomainEvent::TaskStatusChanged { .. } => "task_status_changed",
            DomainEvent::ExecutionCompleted { .. } => "execution_completed",
            DomainEvent::WorkspaceCreated { .. } => "workspace_created",
            DomainEvent::WorkspaceDeleted { .. } => "workspace_deleted",
            DomainEvent::ProjectUpdated { .. } => "project_updated",
            DomainEvent::ApprovalRequested { .. } => "approval_requested",
            DomainEvent::ApprovalResolved { .. } => "approval_resolved",
            DomainEvent::ConversationMessageAdded { .. } => "conversation_message_added",
            DomainEvent::MergeQueueTransition { .. } => "merge_queue_transition",
            DomainEvent::TaskGroupCompleted { .. } => "task_group_completed",
        }
    }

    pub fn occurred_at(&self) -> DateTime<Utc> {
        match self {
            DomainEvent::TaskStatusChanged { task, .. } => task.updated_at,
            DomainEvent::ExecutionCompleted { process, .. } => {
                process.completed_at.unwrap_or(process.updated_at)
            }
            DomainEvent::WorkspaceCreated { workspace } => workspace.created_at,
            DomainEvent::WorkspaceDeleted { .. } => Utc::now(),
            DomainEvent::ProjectUpdated { project } => project.updated_at,
            DomainEvent::ApprovalRequested { occurred_at, .. }
            | DomainEvent::ApprovalResolved { occurred_at, .. }
            | DomainEvent::ConversationMessageAdded { occurred_at, .. }
            | DomainEvent::MergeQueueTransition { occurred_at, .. }
            | DomainEvent::TaskGroupCompleted { occurred_at, .. } => *occurred_at,
        }
    }

    /// Returns the hook point associated with this event.
    pub fn hook_point(&self) -> HookPoint {
        match self {
            DomainEvent::TaskStatusChanged { .. } => HookPoint::PostTaskStatusChange,
            DomainEvent::ExecutionCompleted { .. }
            | DomainEvent::ApprovalRequested { .. }
            | DomainEvent::ApprovalResolved { .. }
            | DomainEvent::ConversationMessageAdded { .. } => HookPoint::PostAgentComplete,
            DomainEvent::WorkspaceCreated { .. } => HookPoint::PostTaskCreate,
            DomainEvent::WorkspaceDeleted { .. }
            | DomainEvent::ProjectUpdated { .. }
            | DomainEvent::MergeQueueTransition { .. }
            | DomainEvent::TaskGroupCompleted { .. } => HookPoint::PostTaskStatusChange,
        }
    }
}

/// Triggers that handlers can return to request execution starts.
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

    /// Trigger processing of the execution queue.
    /// Used when new items are added to the queue (e.g., by autopilot)
    /// to ensure they are processed if capacity is available.
    ProcessQueue,
}

/// Callback type for triggering executions from handlers.
pub type ExecutionTriggerCallback =
    Arc<dyn Fn(ExecutionTrigger) -> BoxFuture<'static, Result<Uuid, anyhow::Error>> + Send + Sync>;

/// Callback type for dispatching domain events.
pub type EventDispatchCallback = Arc<dyn Fn(DomainEvent) -> BoxFuture<'static, ()> + Send + Sync>;
