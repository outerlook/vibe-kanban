use std::sync::Arc;

use workspace_utils::approvals::{ApprovalStatus, QuestionAnswer, QuestionData};

use super::types::PermissionMode;
use crate::{
    approvals::{ExecutorApprovalError, ExecutorApprovalService},
    executors::{
        ExecutorError,
        claude::{
            ClaudeJson, ClaudeQuestionData,
            types::{
                PermissionResult, PermissionUpdate, PermissionUpdateDestination,
                PermissionUpdateType,
            },
        },
        codex::client::LogWriter,
    },
};

const EXIT_PLAN_MODE_NAME: &str = "ExitPlanMode";
const ASK_USER_QUESTION_NAME: &str = "AskUserQuestion";
pub const AUTO_APPROVE_CALLBACK_ID: &str = "AUTO_APPROVE_CALLBACK_ID";

/// Claude Agent client with control protocol support
pub struct ClaudeAgentClient {
    log_writer: LogWriter,
    approvals: Option<Arc<dyn ExecutorApprovalService>>,
    auto_approve: bool, // true when approvals is None
}

impl ClaudeAgentClient {
    /// Create a new client with optional approval service
    pub fn new(
        log_writer: LogWriter,
        approvals: Option<Arc<dyn ExecutorApprovalService>>,
    ) -> Arc<Self> {
        let auto_approve = approvals.is_none();
        Arc::new(Self {
            log_writer,
            approvals,
            auto_approve,
        })
    }

    async fn handle_approval(
        &self,
        tool_use_id: String,
        tool_name: String,
        tool_input: serde_json::Value,
    ) -> Result<PermissionResult, ExecutorError> {
        // Use approval service to request tool approval
        let approval_service = self
            .approvals
            .as_ref()
            .ok_or(ExecutorApprovalError::ServiceUnavailable)?;
        let status = approval_service
            .request_tool_approval(&tool_name, tool_input.clone(), &tool_use_id)
            .await;
        match status {
            Ok(status) => {
                // Log the approval response so we it appears in the executor logs
                self.log_writer
                    .log_raw(&serde_json::to_string(&ClaudeJson::ApprovalResponse {
                        call_id: tool_use_id.clone(),
                        tool_name: tool_name.clone(),
                        approval_status: status.clone(),
                    })?)
                    .await?;
                match status {
                    ApprovalStatus::Approved | ApprovalStatus::Answered { .. } => {
                        if tool_name == EXIT_PLAN_MODE_NAME {
                            Ok(PermissionResult::Allow {
                                updated_input: tool_input,
                                updated_permissions: Some(vec![PermissionUpdate {
                                    update_type: PermissionUpdateType::SetMode,
                                    mode: Some(PermissionMode::BypassPermissions),
                                    destination: PermissionUpdateDestination::Session,
                                }]),
                            })
                        } else {
                            Ok(PermissionResult::Allow {
                                updated_input: tool_input,
                                updated_permissions: None,
                            })
                        }
                    }
                    ApprovalStatus::Denied { reason } => {
                        let message = reason.unwrap_or("Denied by user".to_string());
                        Ok(PermissionResult::Deny {
                            message,
                            interrupt: Some(false),
                        })
                    }
                    ApprovalStatus::TimedOut => Ok(PermissionResult::Deny {
                        message: "Approval request timed out".to_string(),
                        interrupt: Some(false),
                    }),
                    ApprovalStatus::Pending => Ok(PermissionResult::Deny {
                        message: "Approval still pending (unexpected)".to_string(),
                        interrupt: Some(false),
                    }),
                }
            }
            Err(e) => {
                tracing::error!("Tool approval request failed: {e}");
                Ok(PermissionResult::Deny {
                    message: "Tool approval request failed".to_string(),
                    interrupt: Some(false),
                })
            }
        }
    }

    /// Handle AskUserQuestion tool by routing to the approval service for user input.
    /// Returns Allow with answers embedded in updated_input, or Deny on timeout/cancel.
    async fn handle_user_question(
        &self,
        tool_use_id: String,
        tool_input: serde_json::Value,
    ) -> Result<PermissionResult, ExecutorError> {
        // Extract questions from tool input
        let questions = Self::extract_questions(&tool_input)?;

        tracing::debug!(
            tool_use_id = %tool_use_id,
            question_count = questions.len(),
            "Requesting user input for AskUserQuestion"
        );

        // Use approval service to request user input
        let approval_service = self
            .approvals
            .as_ref()
            .ok_or(ExecutorApprovalError::ServiceUnavailable)?;

        let status = approval_service
            .request_user_question(questions, &tool_use_id)
            .await;

        match status {
            Ok(status) => {
                // Log the response for debugging
                self.log_writer
                    .log_raw(&serde_json::to_string(&ClaudeJson::ApprovalResponse {
                        call_id: tool_use_id.clone(),
                        tool_name: ASK_USER_QUESTION_NAME.to_string(),
                        approval_status: status.clone(),
                    })?)
                    .await?;

                match status {
                    ApprovalStatus::Answered { answers } => {
                        tracing::debug!(
                            tool_use_id = %tool_use_id,
                            answer_count = answers.len(),
                            "User answered questions"
                        );
                        // Return Allow with answers embedded in the updated_input
                        let updated_input = Self::build_answered_input(&tool_input, &answers);
                        Ok(PermissionResult::Allow {
                            updated_input,
                            updated_permissions: None,
                        })
                    }
                    ApprovalStatus::Denied { reason } => {
                        let message = reason.unwrap_or("User cancelled question".to_string());
                        tracing::debug!(
                            tool_use_id = %tool_use_id,
                            message = %message,
                            "User question denied"
                        );
                        Ok(PermissionResult::Deny {
                            message,
                            interrupt: Some(false),
                        })
                    }
                    ApprovalStatus::TimedOut => {
                        tracing::debug!(
                            tool_use_id = %tool_use_id,
                            "User question timed out"
                        );
                        Ok(PermissionResult::Deny {
                            message: "User question request timed out".to_string(),
                            interrupt: Some(false),
                        })
                    }
                    ApprovalStatus::Pending => {
                        tracing::warn!(
                            tool_use_id = %tool_use_id,
                            "User question returned pending (unexpected)"
                        );
                        Ok(PermissionResult::Deny {
                            message: "User question still pending (unexpected)".to_string(),
                            interrupt: Some(false),
                        })
                    }
                    ApprovalStatus::Approved => {
                        // Approved without answers is unexpected for user questions
                        tracing::warn!(
                            tool_use_id = %tool_use_id,
                            "User question received Approved instead of Answered"
                        );
                        Ok(PermissionResult::Allow {
                            updated_input: tool_input,
                            updated_permissions: None,
                        })
                    }
                }
            }
            Err(e) => {
                tracing::error!("User question request failed: {e}");
                Ok(PermissionResult::Deny {
                    message: "User question request failed".to_string(),
                    interrupt: Some(false),
                })
            }
        }
    }

    /// Extract questions from AskUserQuestion tool input.
    fn extract_questions(
        tool_input: &serde_json::Value,
    ) -> Result<Vec<QuestionData>, ExecutorError> {
        // The tool input should have a "questions" field containing an array
        let questions_value = tool_input.get("questions").ok_or_else(|| {
            ExecutorApprovalError::RequestFailed(
                "AskUserQuestion missing 'questions' field".to_string(),
            )
        })?;

        let claude_questions: Vec<ClaudeQuestionData> =
            serde_json::from_value(questions_value.clone())?;

        Ok(claude_questions
            .iter()
            .map(|q| q.to_question_data())
            .collect())
    }

    /// Build the updated_input with answers embedded for the tool result.
    fn build_answered_input(
        original_input: &serde_json::Value,
        answers: &[QuestionAnswer],
    ) -> serde_json::Value {
        let mut updated = original_input.clone();
        if let Some(obj) = updated.as_object_mut() {
            obj.insert(
                "answers".to_string(),
                serde_json::to_value(answers).unwrap_or(serde_json::Value::Null),
            );
        }
        updated
    }

    pub async fn on_can_use_tool(
        &self,
        tool_name: String,
        input: serde_json::Value,
        _permission_suggestions: Option<Vec<PermissionUpdate>>,
        tool_use_id: Option<String>,
    ) -> Result<PermissionResult, ExecutorError> {
        if self.auto_approve {
            Ok(PermissionResult::Allow {
                updated_input: input,
                updated_permissions: None,
            })
        } else if let Some(latest_tool_use_id) = tool_use_id {
            // Route AskUserQuestion to dedicated handler
            if tool_name == ASK_USER_QUESTION_NAME {
                self.handle_user_question(latest_tool_use_id, input).await
            } else {
                self.handle_approval(latest_tool_use_id, tool_name, input)
                    .await
            }
        } else {
            // Auto approve tools with no matching tool_use_id
            // tool_use_id is undocumented so this may not be possible
            tracing::warn!(
                "No tool_use_id available for tool '{}', cannot request approval",
                tool_name
            );
            Ok(PermissionResult::Allow {
                updated_input: input,
                updated_permissions: None,
            })
        }
    }

    pub async fn on_hook_callback(
        &self,
        callback_id: String,
        _input: serde_json::Value,
        _tool_use_id: Option<String>,
    ) -> Result<serde_json::Value, ExecutorError> {
        if self.auto_approve {
            Ok(serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "allow",
                    "permissionDecisionReason": "Auto-approved by SDK"
                }
            }))
        } else {
            match callback_id.as_str() {
                AUTO_APPROVE_CALLBACK_ID => Ok(serde_json::json!({
                    "hookSpecificOutput": {
                        "hookEventName": "PreToolUse",
                        "permissionDecision": "allow",
                        "permissionDecisionReason": "Approved by SDK"
                    }
                })),
                _ => {
                    // Hook callbacks is only used to forward approval requests to can_use_tool.
                    // This works because `ask` decision in hook callback triggers a can_use_tool request
                    // https://docs.claude.com/en/api/agent-sdk/permissions#permission-flow-diagram
                    Ok(serde_json::json!({
                        "hookSpecificOutput": {
                            "hookEventName": "PreToolUse",
                            "permissionDecision": "ask",
                            "permissionDecisionReason": "Forwarding to canusetool service"
                        }
                    }))
                }
            }
        }
    }

    pub async fn on_non_control(&self, line: &str) -> Result<(), ExecutorError> {
        // Forward all non-control messages to stdout
        self.log_writer.log_raw(line).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approvals::NoopExecutorApprovalService;

    /// Creates a test client with the given approval service
    fn create_test_client(
        approvals: Option<Arc<dyn ExecutorApprovalService>>,
    ) -> Arc<ClaudeAgentClient> {
        // Use a sink writer that discards all output for tests
        let log_writer = LogWriter::new(tokio::io::sink());
        ClaudeAgentClient::new(log_writer, approvals)
    }

    #[test]
    fn test_extract_questions_success() {
        let input = serde_json::json!({
            "questions": [
                {
                    "question": "What is your name?",
                    "multiSelect": false,
                    "options": [
                        {"label": "Alice"},
                        {"label": "Bob"}
                    ]
                }
            ]
        });

        let questions = ClaudeAgentClient::extract_questions(&input).unwrap();
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question, "What is your name?");
        assert!(!questions[0].multi_select);
        assert_eq!(questions[0].options.len(), 2);
        assert_eq!(questions[0].options[0].label, "Alice");
    }

    #[test]
    fn test_extract_questions_missing_field() {
        let input = serde_json::json!({
            "not_questions": []
        });

        let result = ClaudeAgentClient::extract_questions(&input);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_answered_input() {
        let original = serde_json::json!({
            "questions": [
                {"question": "Choose one", "multiSelect": false, "options": []}
            ]
        });

        let answers = vec![QuestionAnswer {
            question_index: 0,
            selected_indices: vec![1],
            other_text: None,
        }];

        let updated = ClaudeAgentClient::build_answered_input(&original, &answers);

        assert!(updated.get("questions").is_some());
        assert!(updated.get("answers").is_some());

        let answers_value = updated.get("answers").unwrap();
        let parsed_answers: Vec<QuestionAnswer> =
            serde_json::from_value(answers_value.clone()).unwrap();
        assert_eq!(parsed_answers.len(), 1);
        assert_eq!(parsed_answers[0].question_index, 0);
        assert_eq!(parsed_answers[0].selected_indices, vec![1]);
    }

    #[tokio::test]
    async fn test_on_can_use_tool_routes_ask_user_question() {
        // Use NoopExecutorApprovalService which returns empty answers
        let approvals: Arc<dyn ExecutorApprovalService> =
            Arc::new(NoopExecutorApprovalService::default());
        let client = create_test_client(Some(approvals));

        let input = serde_json::json!({
            "questions": [
                {
                    "question": "Select your preference",
                    "multiSelect": false,
                    "options": [{"label": "Option A"}, {"label": "Option B"}]
                }
            ]
        });

        let result = client
            .on_can_use_tool(
                "AskUserQuestion".to_string(),
                input,
                None,
                Some("tool-123".to_string()),
            )
            .await
            .unwrap();

        // NoopExecutorApprovalService returns Answered with empty answers
        match result {
            PermissionResult::Allow { updated_input, .. } => {
                // Should have answers field added
                assert!(updated_input.get("answers").is_some());
            }
            PermissionResult::Deny { .. } => {
                panic!("Expected Allow, got Deny");
            }
        }
    }

    #[tokio::test]
    async fn test_on_can_use_tool_auto_approve_mode() {
        // No approval service = auto-approve mode
        let client = create_test_client(None);

        let input = serde_json::json!({
            "questions": [{"question": "Test?", "multiSelect": false, "options": []}]
        });

        let result = client
            .on_can_use_tool(
                "AskUserQuestion".to_string(),
                input.clone(),
                None,
                Some("tool-456".to_string()),
            )
            .await
            .unwrap();

        // In auto-approve mode, should just return Allow with original input
        match result {
            PermissionResult::Allow { updated_input, .. } => {
                // Original input unchanged in auto-approve mode
                assert_eq!(updated_input, input);
            }
            PermissionResult::Deny { .. } => {
                panic!("Expected Allow, got Deny");
            }
        }
    }

    #[tokio::test]
    async fn test_on_can_use_tool_other_tools_go_to_approval() {
        let approvals: Arc<dyn ExecutorApprovalService> =
            Arc::new(NoopExecutorApprovalService::default());
        let client = create_test_client(Some(approvals));

        let input = serde_json::json!({"command": "ls"});

        let result = client
            .on_can_use_tool(
                "Bash".to_string(),
                input.clone(),
                None,
                Some("tool-789".to_string()),
            )
            .await
            .unwrap();

        // NoopExecutorApprovalService returns Approved for regular tools
        match result {
            PermissionResult::Allow { updated_input, .. } => {
                assert_eq!(updated_input, input);
            }
            PermissionResult::Deny { .. } => {
                panic!("Expected Allow, got Deny");
            }
        }
    }
}
