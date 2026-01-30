//! Hook points defining WHERE hooks can fire in the application lifecycle.
//!
//! These represent specific points in the application flow where custom
//! handlers can be triggered to perform additional actions.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Points in the application lifecycle where hooks can be triggered.
///
/// Hook points are divided into "Pre" and "Post" variants:
/// - "Pre" hooks fire before the action occurs and can potentially modify or cancel it.
/// - "Post" hooks fire after the action has completed and are informational.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum HookPoint {
    /// Before a new task is created.
    PreTaskCreate,

    /// After a new task has been created.
    PostTaskCreate,

    /// Before a task's status is changed.
    PreTaskStatusChange,

    /// After a task's status has been changed.
    PostTaskStatusChange,

    /// After an agent has completed its execution.
    PostAgentComplete,

    /// After a task becomes unblocked (all dependencies are now complete).
    PostDependencyUnblocked,
}

impl HookPoint {
    /// Returns true if this is a "pre" hook that fires before the action.
    pub fn is_pre_hook(&self) -> bool {
        matches!(
            self,
            HookPoint::PreTaskCreate | HookPoint::PreTaskStatusChange
        )
    }

    /// Returns true if this is a "post" hook that fires after the action.
    pub fn is_post_hook(&self) -> bool {
        !self.is_pre_hook()
    }

    /// Returns the display name for this hook point.
    pub fn display_name(&self) -> &'static str {
        match self {
            HookPoint::PreTaskCreate => "Pre-Task Create",
            HookPoint::PostTaskCreate => "Post-Task Create",
            HookPoint::PreTaskStatusChange => "Pre-Task Status Change",
            HookPoint::PostTaskStatusChange => "Post-Task Status Change",
            HookPoint::PostAgentComplete => "Post-Agent Complete",
            HookPoint::PostDependencyUnblocked => "Post-Dependency Unblocked",
        }
    }
}
