//! ReviewAttentionService for analyzing if completed work needs human attention.
//!
//! This service generates prompts to ask agents to analyze their work output
//! and extracts structured responses indicating whether human review is needed.

use executors::{
    actions::{
        ExecutorAction, ExecutorActionType, coding_agent_follow_up::CodingAgentFollowUpRequest,
    },
    profile::ExecutorProfileId,
};
use serde::Deserialize;
use thiserror::Error;

/// Errors that can occur during review attention operations.
#[derive(Debug, Error)]
pub enum ReviewAttentionError {
    /// Failed to parse review attention response as JSON.
    #[error("Failed to parse review attention response: {0}")]
    ParseError(String),
}

pub type Result<T> = std::result::Result<T, ReviewAttentionError>;

/// Result of parsing an agent's review attention response.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewAttentionResult {
    pub needs_attention: bool,
    pub reasoning: Option<String>,
}

/// Internal struct for deserializing the JSON response.
#[derive(Debug, Deserialize)]
struct ReviewAttentionResponse {
    needs_attention: bool,
    reasoning: Option<String>,
}

/// Service for generating review attention prompts and parsing agent responses.
#[derive(Clone, Default)]
pub struct ReviewAttentionService;

impl ReviewAttentionService {
    pub fn new() -> Self {
        Self
    }

    /// Generate a prompt that asks the agent to analyze if the work needs human attention.
    ///
    /// # Arguments
    /// * `task_description` - The original task description
    /// * `agent_summary` - The agent's summary of what was done
    ///
    /// # Returns
    /// A prompt string requesting structured analysis
    pub fn generate_review_attention_prompt(task_description: &str, agent_summary: &str) -> String {
        format!(
            r#"Please analyze whether your completed work requires human attention or review.

## Original Task
{task_description}

## Your Work Summary
{agent_summary}

## Analysis Instructions
Evaluate your work and determine if a human needs to review it. Consider:

**Needs attention if ANY of these apply:**
- Errors occurred during execution that weren't fully resolved
- Work is incomplete or partially done
- You encountered blockers or made significant assumptions
- Tests are failing or were skipped
- You're uncertain about the correctness of your implementation
- Security-sensitive changes were made
- Breaking changes or API modifications were introduced
- You had to deviate significantly from the task requirements
- Configuration or environment issues remain unresolved

**Does NOT need attention if:**
- Task was completed successfully with all requirements met
- All tests pass (if applicable)
- No errors or warnings remain
- Implementation follows established patterns
- Changes are straightforward and low-risk

Respond with a JSON object:

```json
{{
  "needs_attention": true,
  "reasoning": "Brief explanation of why attention is or isn't needed"
}}
```

Be honest and conservative - when in doubt, flag for attention."#
        )
    }

    /// Parse an agent's review attention response to extract the structured result.
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
    /// * `Ok(ReviewAttentionResult)` - The parsed result
    /// * `Err(ReviewAttentionError::ParseError)` - Failed to extract valid JSON
    pub fn parse_review_attention_response(
        assistant_message: &str,
    ) -> Result<ReviewAttentionResult> {
        let trimmed = assistant_message.trim();

        if trimmed.is_empty() {
            return Err(ReviewAttentionError::ParseError(
                "Empty response received".to_string(),
            ));
        }

        let json_str = Self::extract_json(trimmed)?;

        let response: ReviewAttentionResponse = serde_json::from_str(&json_str).map_err(|e| {
            ReviewAttentionError::ParseError(format!("Failed to deserialize JSON: {}", e))
        })?;

        Ok(ReviewAttentionResult {
            needs_attention: response.needs_attention,
            reasoning: response.reasoning,
        })
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

        Err(ReviewAttentionError::ParseError(
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

    /// Create an executor action for collecting review attention analysis from an agent.
    ///
    /// # Arguments
    /// * `session_id` - The session ID to continue the conversation
    /// * `executor_profile_id` - The executor profile to use
    /// * `working_dir` - Optional working directory for the agent
    /// * `task_description` - The original task description
    /// * `agent_summary` - The agent's summary of completed work
    ///
    /// # Returns
    /// An `ExecutorAction` configured to send the review attention prompt
    pub fn create_review_attention_action(
        session_id: String,
        executor_profile_id: ExecutorProfileId,
        working_dir: Option<String>,
        task_description: &str,
        agent_summary: &str,
    ) -> ExecutorAction {
        let follow_up = CodingAgentFollowUpRequest {
            prompt: Self::generate_review_attention_prompt(task_description, agent_summary),
            session_id,
            executor_profile_id,
            working_dir,
        };

        ExecutorAction::new(
            ExecutorActionType::CodingAgentFollowUpRequest(follow_up),
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_prompt_contains_required_fields() {
        let prompt = ReviewAttentionService::generate_review_attention_prompt(
            "Implement user login",
            "Added login form and validation",
        );

        // Check that task and summary are included
        assert!(prompt.contains("Implement user login"));
        assert!(prompt.contains("Added login form and validation"));

        // Check that key analysis criteria are mentioned
        assert!(prompt.contains("needs_attention"));
        assert!(prompt.contains("reasoning"));
        assert!(prompt.contains("Errors"));
        assert!(prompt.contains("incomplete"));
        assert!(prompt.contains("Tests"));
        assert!(prompt.contains("JSON"));
    }

    #[test]
    fn test_parse_valid_json_response() {
        let response = r#"{
            "needs_attention": true,
            "reasoning": "Tests are failing for edge cases"
        }"#;

        let result = ReviewAttentionService::parse_review_attention_response(response);
        assert!(result.is_ok());

        let parsed = result.unwrap();
        assert!(parsed.needs_attention);
        assert_eq!(
            parsed.reasoning,
            Some("Tests are failing for edge cases".to_string())
        );
    }

    #[test]
    fn test_parse_valid_json_no_attention_needed() {
        let response = r#"{
            "needs_attention": false,
            "reasoning": "All tests pass, implementation is complete"
        }"#;

        let result = ReviewAttentionService::parse_review_attention_response(response);
        assert!(result.is_ok());

        let parsed = result.unwrap();
        assert!(!parsed.needs_attention);
        assert_eq!(
            parsed.reasoning,
            Some("All tests pass, implementation is complete".to_string())
        );
    }

    #[test]
    fn test_parse_json_with_null_reasoning() {
        let response = r#"{
            "needs_attention": false,
            "reasoning": null
        }"#;

        let result = ReviewAttentionService::parse_review_attention_response(response);
        assert!(result.is_ok());

        let parsed = result.unwrap();
        assert!(!parsed.needs_attention);
        assert!(parsed.reasoning.is_none());
    }

    #[test]
    fn test_parse_json_in_markdown_code_block() {
        let response = r#"Based on my analysis:

```json
{
    "needs_attention": true,
    "reasoning": "Security-sensitive authentication changes require review"
}
```

Please review the authentication module changes."#;

        let result = ReviewAttentionService::parse_review_attention_response(response);
        assert!(result.is_ok());

        let parsed = result.unwrap();
        assert!(parsed.needs_attention);
        assert!(parsed.reasoning.unwrap().contains("Security-sensitive"));
    }

    #[test]
    fn test_parse_json_in_plain_code_block() {
        let response = r#"My analysis:

```
{
    "needs_attention": false,
    "reasoning": "Straightforward refactoring with passing tests"
}
```"#;

        let result = ReviewAttentionService::parse_review_attention_response(response);
        assert!(result.is_ok());

        let parsed = result.unwrap();
        assert!(!parsed.needs_attention);
    }

    #[test]
    fn test_parse_json_embedded_in_text() {
        let response = r#"After analyzing the work, here's my assessment:

{
    "needs_attention": true,
    "reasoning": "Database migration needs verification"
}

Let me know if you need more details."#;

        let result = ReviewAttentionService::parse_review_attention_response(response);
        assert!(result.is_ok());

        let parsed = result.unwrap();
        assert!(parsed.needs_attention);
        assert!(parsed.reasoning.unwrap().contains("Database migration"));
    }

    #[test]
    fn test_parse_malformed_json_returns_error() {
        let response = r#"This is not valid JSON {broken"#;

        let result = ReviewAttentionService::parse_review_attention_response(response);
        assert!(result.is_err());

        match result {
            Err(ReviewAttentionError::ParseError(msg)) => {
                assert!(msg.contains("Could not find valid JSON"));
            }
            _ => panic!("Expected ParseError"),
        }
    }

    #[test]
    fn test_parse_empty_response_returns_error() {
        let result = ReviewAttentionService::parse_review_attention_response("");
        assert!(result.is_err());

        match result {
            Err(ReviewAttentionError::ParseError(msg)) => {
                assert!(msg.contains("Empty response"));
            }
            _ => panic!("Expected ParseError"),
        }
    }

    #[test]
    fn test_parse_missing_required_field_returns_error() {
        let response = r#"{
            "reasoning": "Some reasoning but missing needs_attention"
        }"#;

        let result = ReviewAttentionService::parse_review_attention_response(response);
        assert!(result.is_err());

        match result {
            Err(ReviewAttentionError::ParseError(msg)) => {
                assert!(msg.contains("Failed to deserialize"));
            }
            _ => panic!("Expected ParseError"),
        }
    }

    #[test]
    fn test_parse_json_with_special_chars() {
        let response = r#"{
            "needs_attention": true,
            "reasoning": "The task said \"implement feature\" with {curly} braces"
        }"#;

        let result = ReviewAttentionService::parse_review_attention_response(response);
        assert!(result.is_ok());

        let parsed = result.unwrap();
        assert!(parsed.needs_attention);
        assert!(parsed.reasoning.unwrap().contains("implement feature"));
    }

    #[test]
    fn test_create_review_attention_action() {
        let session_id = "test-session-456".to_string();
        let profile_id = ExecutorProfileId {
            executor: executors::executors::BaseCodingAgent::ClaudeCode,
            variant: None,
        };
        let working_dir = Some("/path/to/project".to_string());
        let task_description = "Fix the login bug";
        let agent_summary = "Patched the authentication flow";

        let action = ReviewAttentionService::create_review_attention_action(
            session_id.clone(),
            profile_id.clone(),
            working_dir.clone(),
            task_description,
            agent_summary,
        );

        // Verify the action is a follow-up request
        match action.typ {
            ExecutorActionType::CodingAgentFollowUpRequest(ref req) => {
                assert_eq!(req.session_id, session_id);
                assert_eq!(req.executor_profile_id, profile_id);
                assert_eq!(req.working_dir, working_dir);
                // Verify prompt contains task and summary
                assert!(req.prompt.contains("Fix the login bug"));
                assert!(req.prompt.contains("Patched the authentication flow"));
                assert!(req.prompt.contains("needs_attention"));
            }
            _ => panic!("Expected CodingAgentFollowUpRequest"),
        }

        // Verify no next action
        assert!(action.next_action.is_none());
    }

    #[test]
    fn test_create_review_attention_action_without_working_dir() {
        let session_id = "test-session-789".to_string();
        let profile_id = ExecutorProfileId {
            executor: executors::executors::BaseCodingAgent::ClaudeCode,
            variant: None,
        };

        let action = ReviewAttentionService::create_review_attention_action(
            session_id.clone(),
            profile_id.clone(),
            None,
            "Task",
            "Summary",
        );

        match action.typ {
            ExecutorActionType::CodingAgentFollowUpRequest(ref req) => {
                assert!(req.working_dir.is_none());
            }
            _ => panic!("Expected CodingAgentFollowUpRequest"),
        }
    }
}
