//! FeedbackService for generating prompts and extracting agent feedback responses.
//!
//! This service handles the prompt generation for collecting feedback from agents
//! and extracting valid JSON from their responses for storage.

use executors::{
    actions::{
        coding_agent_follow_up::CodingAgentFollowUpRequest, ExecutorAction, ExecutorActionType,
    },
    profile::ExecutorProfileId,
};
use thiserror::Error;

/// Errors that can occur during feedback operations.
#[derive(Debug, Error)]
pub enum FeedbackError {
    /// Failed to parse feedback response as JSON.
    #[error("Failed to parse feedback response: {0}")]
    ParseError(String),
}

pub type Result<T> = std::result::Result<T, FeedbackError>;

/// Service for generating feedback prompts and parsing agent responses.
#[derive(Clone, Default)]
pub struct FeedbackService;

impl FeedbackService {
    pub fn new() -> Self {
        Self
    }

    /// Generate a prompt that asks the agent for structured feedback.
    ///
    /// The prompt requests JSON format with 5 categories of feedback.
    pub fn generate_feedback_prompt() -> String {
        r#"Please provide feedback about your experience working on this task.

Respond with a JSON object containing the following fields (use null for any field you don't have feedback on):

```json
{
  "task_clarity": "Your feedback on whether the task description was clear and complete. What was confusing or missing?",
  "missing_tools": "What tools, capabilities, or access would have made this task easier? What couldn't you do that you needed to?",
  "integration_problems": "Any issues with the development environment, build system, dependencies, or integration with external services?",
  "improvement_suggestions": "General suggestions for improving the system, workflow, or agent capabilities.",
  "agent_documentation": "Any notes, learnings, or documentation you'd like to record about this task for future reference."
}
```

Be specific and actionable in your feedback. If a category doesn't apply, set it to null."#.to_string()
    }

    /// Extract valid JSON from an agent's feedback response.
    ///
    /// Handles multiple response formats:
    /// - Raw JSON object
    /// - JSON embedded in markdown code blocks
    /// - JSON embedded in surrounding text
    ///
    /// # Arguments
    /// * `assistant_message` - The raw text response from the agent
    ///
    /// # Returns
    /// * `Ok(String)` - The extracted raw JSON string
    /// * `Err(FeedbackError::ParseError)` - Failed to extract valid JSON
    pub fn parse_feedback_response(assistant_message: &str) -> Result<String> {
        let trimmed = assistant_message.trim();

        if trimmed.is_empty() {
            return Err(FeedbackError::ParseError(
                "Empty response received".to_string(),
            ));
        }

        // Extract and validate JSON from the response
        Self::extract_json(trimmed)
    }

    /// Extract JSON content from a response that might contain markdown or other text.
    fn extract_json(text: &str) -> Result<String> {
        // Strategy 1: Try parsing the entire text as JSON
        if serde_json::from_str::<serde_json::Value>(text).is_ok() {
            return Ok(text.to_string());
        }

        // Strategy 2: Look for JSON in code blocks (```json ... ``` or ``` ... ```)
        if let Some(json) = Self::extract_from_code_block(text)
            && serde_json::from_str::<serde_json::Value>(&json).is_ok()
        {
            return Ok(json);
        }

        // Strategy 3: Find JSON object by looking for { ... } pattern
        if let Some(json) = Self::extract_json_object(text)
            && serde_json::from_str::<serde_json::Value>(&json).is_ok()
        {
            return Ok(json);
        }

        Err(FeedbackError::ParseError(
            "Could not find valid JSON in response".to_string(),
        ))
    }

    /// Extract content from markdown code blocks.
    fn extract_from_code_block(text: &str) -> Option<String> {
        // Match ```json ... ``` or ``` ... ```
        let patterns = ["```json", "```"];

        for pattern in patterns {
            if let Some(start_idx) = text.find(pattern) {
                let content_start = start_idx + pattern.len();
                if let Some(end_idx) = text[content_start..].find("```") {
                    let content = text[content_start..content_start + end_idx].trim();
                    if !content.is_empty() {
                        return Some(content.to_string());
                    }
                }
            }
        }

        None
    }

    /// Extract a JSON object by finding matching braces.
    fn extract_json_object(text: &str) -> Option<String> {
        let start = text.find('{')?;
        let mut depth = 0;
        let mut in_string = false;
        let mut escape_next = false;

        for (i, c) in text[start..].char_indices() {
            if escape_next {
                escape_next = false;
                continue;
            }

            match c {
                '\\' if in_string => escape_next = true,
                '"' => in_string = !in_string,
                '{' if !in_string => depth += 1,
                '}' if !in_string => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(text[start..=start + i].to_string());
                    }
                }
                _ => {}
            }
        }

        None
    }

    /// Create an executor action for collecting feedback from an agent.
    ///
    /// # Arguments
    /// * `session_id` - The session ID to continue the conversation
    /// * `executor_profile_id` - The executor profile to use
    /// * `working_dir` - Optional working directory for the agent
    ///
    /// # Returns
    /// An `ExecutorAction` configured to send the feedback prompt
    pub fn create_feedback_action(
        session_id: String,
        executor_profile_id: ExecutorProfileId,
        working_dir: Option<String>,
    ) -> ExecutorAction {
        let follow_up = CodingAgentFollowUpRequest {
            prompt: Self::generate_feedback_prompt(),
            session_id,
            executor_profile_id,
            working_dir,
        };

        ExecutorAction::new(ExecutorActionType::CodingAgentFollowUpRequest(follow_up), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_feedback_prompt_contains_all_fields() {
        let prompt = FeedbackService::generate_feedback_prompt();

        assert!(prompt.contains("task_clarity"));
        assert!(prompt.contains("missing_tools"));
        assert!(prompt.contains("integration_problems"));
        assert!(prompt.contains("improvement_suggestions"));
        assert!(prompt.contains("agent_documentation"));
        assert!(prompt.contains("JSON"));
    }

    #[test]
    fn test_parse_valid_json_response() {
        let response = r#"{
            "task_clarity": "The task was clear",
            "missing_tools": "Would have liked a database viewer",
            "integration_problems": null,
            "improvement_suggestions": "Better error messages",
            "agent_documentation": "Completed the refactoring"
        }"#;

        let result = FeedbackService::parse_feedback_response(response);
        assert!(result.is_ok());

        let json_str = result.unwrap();
        // Verify it's valid JSON by parsing it
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["task_clarity"], "The task was clear");
        assert_eq!(parsed["missing_tools"], "Would have liked a database viewer");
        assert!(parsed["integration_problems"].is_null());
    }

    #[test]
    fn test_parse_json_in_markdown_code_block() {
        let response = r#"Here's my feedback:

```json
{
    "task_clarity": "Very clear",
    "missing_tools": null,
    "integration_problems": null,
    "improvement_suggestions": null,
    "agent_documentation": "All done"
}
```

Hope this helps!"#;

        let result = FeedbackService::parse_feedback_response(response);
        assert!(result.is_ok());

        let json_str = result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["task_clarity"], "Very clear");
        assert_eq!(parsed["agent_documentation"], "All done");
    }

    #[test]
    fn test_parse_json_in_plain_code_block() {
        let response = r#"My feedback:

```
{
    "task_clarity": "Mostly clear",
    "missing_tools": "Git integration",
    "integration_problems": null,
    "improvement_suggestions": null,
    "agent_documentation": null
}
```"#;

        let result = FeedbackService::parse_feedback_response(response);
        assert!(result.is_ok());

        let json_str = result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["task_clarity"], "Mostly clear");
        assert_eq!(parsed["missing_tools"], "Git integration");
    }

    #[test]
    fn test_parse_partial_fields() {
        let response = r#"{
            "task_clarity": "Clear enough",
            "missing_tools": null,
            "integration_problems": null,
            "improvement_suggestions": null,
            "agent_documentation": null
        }"#;

        let result = FeedbackService::parse_feedback_response(response);
        assert!(result.is_ok());

        let json_str = result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["task_clarity"], "Clear enough");
        assert!(parsed["missing_tools"].is_null());
    }

    #[test]
    fn test_parse_malformed_json_returns_error() {
        let response = r#"This is not valid JSON at all {broken"#;

        let result = FeedbackService::parse_feedback_response(response);
        assert!(result.is_err());

        match result {
            Err(FeedbackError::ParseError(msg)) => {
                assert!(msg.contains("Could not find valid JSON"));
            }
            _ => panic!("Expected ParseError"),
        }
    }

    #[test]
    fn test_parse_empty_response_returns_error() {
        let result = FeedbackService::parse_feedback_response("");
        assert!(result.is_err());

        match result {
            Err(FeedbackError::ParseError(msg)) => {
                assert!(msg.contains("Empty response"));
            }
            _ => panic!("Expected ParseError"),
        }
    }

    #[test]
    fn test_parse_whitespace_only_returns_error() {
        let result = FeedbackService::parse_feedback_response("   \n\t  ");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_json_embedded_in_text() {
        let response = r#"Sure, here's my feedback:

{
    "task_clarity": "Good",
    "missing_tools": null,
    "integration_problems": "Build was slow",
    "improvement_suggestions": null,
    "agent_documentation": null
}

Let me know if you need more details."#;

        let result = FeedbackService::parse_feedback_response(response);
        assert!(result.is_ok());

        let json_str = result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["task_clarity"], "Good");
        assert_eq!(parsed["integration_problems"], "Build was slow");
    }

    #[test]
    fn test_parse_nested_json_with_special_chars() {
        let response = r#"{
            "task_clarity": "The task said \"implement feature X\"",
            "missing_tools": "Need {curly} braces support",
            "integration_problems": null,
            "improvement_suggestions": null,
            "agent_documentation": "Used path: /home/user/project"
        }"#;

        let result = FeedbackService::parse_feedback_response(response);
        assert!(result.is_ok());

        let json_str = result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(parsed["task_clarity"].as_str().unwrap().contains("implement feature X"));
        assert!(parsed["missing_tools"].as_str().unwrap().contains("{curly}"));
    }

    #[test]
    fn test_create_feedback_action() {
        let session_id = "test-session-123".to_string();
        let profile_id = ExecutorProfileId {
            executor: executors::executors::BaseCodingAgent::ClaudeCode,
            variant: None,
        };
        let working_dir = Some("/path/to/work".to_string());

        let action =
            FeedbackService::create_feedback_action(session_id.clone(), profile_id.clone(), working_dir.clone());

        // Verify the action is a follow-up request
        match action.typ {
            ExecutorActionType::CodingAgentFollowUpRequest(ref req) => {
                assert_eq!(req.session_id, session_id);
                assert_eq!(req.executor_profile_id, profile_id);
                assert_eq!(req.working_dir, working_dir);
                assert!(req.prompt.contains("task_clarity"));
            }
            _ => panic!("Expected CodingAgentFollowUpRequest"),
        }

        // Verify no next action
        assert!(action.next_action.is_none());
    }
}
