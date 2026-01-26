use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

pub const APPROVAL_TIMEOUT_SECONDS: i64 = 3600; // 1 hour

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct QuestionOption {
    pub label: String,
    #[ts(optional)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct QuestionData {
    pub question: String,
    #[ts(optional)]
    pub header: Option<String>,
    pub multi_select: bool,
    pub options: Vec<QuestionOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct QuestionAnswer {
    pub question_index: usize,
    pub selected_indices: Vec<usize>,
    #[ts(optional)]
    pub other_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApprovalRequestType {
    ToolApproval {
        tool_name: String,
        tool_input: serde_json::Value,
    },
    UserQuestion {
        questions: Vec<QuestionData>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ApprovalRequest {
    pub id: String,
    pub request_type: ApprovalRequestType,
    pub tool_call_id: String,
    pub execution_process_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub timeout_at: DateTime<Utc>,
}

impl ApprovalRequest {
    pub fn from_create(request: CreateApprovalRequest, execution_process_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            request_type: ApprovalRequestType::ToolApproval {
                tool_name: request.tool_name,
                tool_input: request.tool_input,
            },
            tool_call_id: request.tool_call_id,
            execution_process_id,
            created_at: now,
            timeout_at: now + Duration::seconds(APPROVAL_TIMEOUT_SECONDS),
        }
    }

    /// Creates a user question approval request
    pub fn from_user_question(
        questions: Vec<QuestionData>,
        tool_call_id: String,
        execution_process_id: Uuid,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            request_type: ApprovalRequestType::UserQuestion { questions },
            tool_call_id,
            execution_process_id,
            created_at: now,
            timeout_at: now + Duration::seconds(APPROVAL_TIMEOUT_SECONDS),
        }
    }

    /// Returns the tool name if this is a tool approval request
    pub fn tool_name(&self) -> Option<&str> {
        match &self.request_type {
            ApprovalRequestType::ToolApproval { tool_name, .. } => Some(tool_name),
            ApprovalRequestType::UserQuestion { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CreateApprovalRequest {
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub tool_call_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Denied {
        #[ts(optional)]
        reason: Option<String>,
    },
    Answered {
        answers: Vec<QuestionAnswer>,
    },
    TimedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ApprovalResponse {
    pub execution_process_id: Uuid,
    pub status: ApprovalStatus,
    #[ts(optional)]
    pub answers: Option<Vec<QuestionAnswer>>,
}
