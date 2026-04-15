//! ReviewAttentionService for parsing and summarizing external review verdicts.

use db::models::review_attention::ReviewAttention;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

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

/// Internal struct for deserializing the JSON response.
#[derive(Debug, Deserialize)]
struct ReviewAttentionResponse {
    needs_attention: bool,
    reasoning: Option<String>,
}

/// Service for parsing review verdicts and hydrating stored review records.
#[derive(Clone, Default)]
pub struct ReviewAttentionService;

impl ReviewAttentionService {
    pub fn new() -> Self {
        Self
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
