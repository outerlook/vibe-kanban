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
    #[ts(optional)]
    pub timeout_at: Option<DateTime<Utc>>,
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
            timeout_at: Some(now + Duration::seconds(APPROVAL_TIMEOUT_SECONDS)),
        }
    }

    /// Creates a user question approval request (no timeout)
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
            timeout_at: None,
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

/// Format questions and answers into a follow-up prompt for the agent.
/// This is used when the executor was dead and the user answered a question.
pub fn format_qa_as_follow_up_prompt(
    questions: &[QuestionData],
    answers: &[QuestionAnswer],
) -> String {
    let mut prompt = String::from(
        "The user has answered your question(s). Here is the Q&A:\n\n",
    );

    for answer in answers {
        if answer.question_index >= questions.len() {
            continue;
        }
        let question = &questions[answer.question_index];

        prompt.push_str(&format!("**Question:** {}\n", question.question));

        if answer.selected_indices.is_empty() && answer.other_text.is_none() {
            prompt.push_str("**Answer:** (No selection)\n\n");
            continue;
        }

        let mut answer_parts = Vec::new();

        // Add selected options
        for &idx in &answer.selected_indices {
            if idx < question.options.len() {
                answer_parts.push(question.options[idx].label.clone());
            }
        }

        // Add custom text if provided
        if let Some(ref other_text) = answer.other_text {
            if !other_text.is_empty() {
                answer_parts.push(format!("Other: {}", other_text));
            }
        }

        prompt.push_str(&format!("**Answer:** {}\n\n", answer_parts.join(", ")));
    }

    prompt.push_str("Please continue based on the user's response.");
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_qa_single_question_single_answer() {
        let questions = vec![QuestionData {
            question: "What color do you prefer?".to_string(),
            header: Some("Color".to_string()),
            multi_select: false,
            options: vec![
                QuestionOption {
                    label: "Red".to_string(),
                    description: Some("A warm color".to_string()),
                },
                QuestionOption {
                    label: "Blue".to_string(),
                    description: None,
                },
            ],
        }];

        let answers = vec![QuestionAnswer {
            question_index: 0,
            selected_indices: vec![1],
            other_text: None,
        }];

        let prompt = format_qa_as_follow_up_prompt(&questions, &answers);

        assert!(prompt.contains("**Question:** What color do you prefer?"));
        assert!(prompt.contains("**Answer:** Blue"));
        assert!(prompt.contains("Please continue based on the user's response."));
    }

    #[test]
    fn test_format_qa_with_other_text() {
        let questions = vec![QuestionData {
            question: "What framework do you use?".to_string(),
            header: None,
            multi_select: false,
            options: vec![
                QuestionOption {
                    label: "React".to_string(),
                    description: None,
                },
                QuestionOption {
                    label: "Vue".to_string(),
                    description: None,
                },
            ],
        }];

        let answers = vec![QuestionAnswer {
            question_index: 0,
            selected_indices: vec![],
            other_text: Some("Svelte".to_string()),
        }];

        let prompt = format_qa_as_follow_up_prompt(&questions, &answers);

        assert!(prompt.contains("**Question:** What framework do you use?"));
        assert!(prompt.contains("Other: Svelte"));
    }

    #[test]
    fn test_format_qa_multi_select() {
        let questions = vec![QuestionData {
            question: "Which features do you want?".to_string(),
            header: None,
            multi_select: true,
            options: vec![
                QuestionOption {
                    label: "Auth".to_string(),
                    description: None,
                },
                QuestionOption {
                    label: "Logging".to_string(),
                    description: None,
                },
                QuestionOption {
                    label: "Caching".to_string(),
                    description: None,
                },
            ],
        }];

        let answers = vec![QuestionAnswer {
            question_index: 0,
            selected_indices: vec![0, 2],
            other_text: None,
        }];

        let prompt = format_qa_as_follow_up_prompt(&questions, &answers);

        assert!(prompt.contains("**Answer:** Auth, Caching"));
    }

    #[test]
    fn test_format_qa_no_selection() {
        let questions = vec![QuestionData {
            question: "Any preference?".to_string(),
            header: None,
            multi_select: false,
            options: vec![QuestionOption {
                label: "Option A".to_string(),
                description: None,
            }],
        }];

        let answers = vec![QuestionAnswer {
            question_index: 0,
            selected_indices: vec![],
            other_text: None,
        }];

        let prompt = format_qa_as_follow_up_prompt(&questions, &answers);

        assert!(prompt.contains("(No selection)"));
    }
}
