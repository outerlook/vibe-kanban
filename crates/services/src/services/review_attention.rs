//! ReviewAttentionService for hydrating persisted review-attention records.

use db::models::review_attention::ReviewAttention;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReviewAttentionHydrationSummary {
    pub id: String,
    pub execution_process_id: String,
    pub task_id: String,
    pub workspace_id: String,
    pub needs_attention: bool,
    pub reasoning: Option<String>,
    pub analyzed_at: String,
}

/// Service for hydrating stored review-attention records.
#[derive(Clone, Default)]
pub struct ReviewAttentionService;

impl ReviewAttentionService {
    pub fn summarize_record(record: &ReviewAttention) -> ReviewAttentionHydrationSummary {
        ReviewAttentionHydrationSummary {
            id: record.id.to_string(),
            execution_process_id: record.execution_process_id.to_string(),
            task_id: record.task_id.to_string(),
            workspace_id: record.workspace_id.to_string(),
            needs_attention: record.needs_attention,
            reasoning: record.reasoning.clone(),
            analyzed_at: record.analyzed_at.to_rfc3339(),
        }
    }
}
