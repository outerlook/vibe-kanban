use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::execution_process::ExecutionProcessStatus;
use super::task::TaskStatus;

/// Represents a task for Gantt chart visualization
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct GanttTask {
    pub id: Uuid,
    pub name: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub progress: f32,
    pub dependencies: Vec<Uuid>,
    pub task_status: TaskStatus,
}

/// Represents execution overlay information for a Gantt task
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct GanttExecutionOverlay {
    pub task_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: ExecutionProcessStatus,
}
