//! Hook execution status tracking types.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Status of a hook execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum HookExecutionStatus {
    Running,
    Completed,
    Failed,
}
