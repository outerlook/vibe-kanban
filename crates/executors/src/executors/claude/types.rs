//! Type definitions for Claude Code control protocol
//!
//! Similar to: https://github.com/ZhangHanDong/claude-code-api-rs/blob/main/claude-code-sdk-rs/src/types.rs

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Top-level message types from CLI stdout
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CLIMessage {
    ControlRequest {
        request_id: String,
        request: ControlRequestType,
    },
    ControlResponse {
        response: ControlResponseType,
    },
    Result(serde_json::Value),
    #[serde(untagged)]
    Other(serde_json::Value),
}

/// Control request from SDK to CLI (outgoing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SDKControlRequest {
    #[serde(rename = "type")]
    message_type: String, // Always "control_request"
    pub request_id: String,
    pub request: SDKControlRequestType,
}

impl SDKControlRequest {
    pub fn new(request: SDKControlRequestType) -> Self {
        use uuid::Uuid;
        Self {
            message_type: "control_request".to_string(),
            request_id: Uuid::new_v4().to_string(),
            request,
        }
    }
}

/// Control response from SDK to CLI (outgoing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlResponseMessage {
    #[serde(rename = "type")]
    message_type: String, // Always "control_response"
    pub response: ControlResponseType,
}

impl ControlResponseMessage {
    pub fn new(response: ControlResponseType) -> Self {
        Self {
            message_type: "control_response".to_string(),
            response,
        }
    }
}

/// Types of control requests
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
pub enum ControlRequestType {
    CanUseTool {
        tool_name: String,
        input: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        permission_suggestions: Option<Vec<PermissionUpdate>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_use_id: Option<String>,
    },
    HookCallback {
        #[serde(rename = "callback_id")]
        callback_id: String,
        input: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_use_id: Option<String>,
    },
}

/// Result of permission check
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "behavior", rename_all = "camelCase")]
pub enum PermissionResult {
    Allow {
        #[serde(rename = "updatedInput")]
        updated_input: Value,
        #[serde(skip_serializing_if = "Option::is_none", rename = "updatedPermissions")]
        updated_permissions: Option<Vec<PermissionUpdate>>,
    },
    Deny {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        interrupt: Option<bool>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionUpdateType {
    SetMode,
    AddRules,
    RemoveRules,
    ClearRules,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionUpdateDestination {
    Session,
    UserSettings,
    ProjectSettings,
    LocalSettings,
}

/// Permission update operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionUpdate {
    #[serde(rename = "type")]
    pub update_type: PermissionUpdateType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<PermissionMode>,
    pub destination: PermissionUpdateDestination,
}

/// Control response from SDK to CLI
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
pub enum ControlResponseType {
    Success {
        request_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        response: Option<Value>,
    },
    Error {
        request_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    User { message: ClaudeUserMessage },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeUserMessage {
    role: String,
    content: String,
}

impl Message {
    pub fn new_user(content: String) -> Self {
        Self::User {
            message: ClaudeUserMessage {
                role: "user".to_string(),
                content,
            },
        }
    }
}

/// Tool result message sent to Claude via stdin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultMessage {
    #[serde(rename = "type")]
    message_type: String, // Always "tool_result"
    pub tool_use_id: String,
    pub result: Value,
    pub is_error: bool,
}

impl ToolResultMessage {
    pub fn new(tool_use_id: String, result: Value, is_error: bool) -> Self {
        Self {
            message_type: "tool_result".to_string(),
            tool_use_id,
            result,
            is_error,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
pub enum SDKControlRequestType {
    SetPermissionMode {
        mode: PermissionMode,
    },
    Initialize {
        #[serde(skip_serializing_if = "Option::is_none")]
        hooks: Option<Value>,
    },
    Interrupt {},
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    Default,
    AcceptEdits,
    Plan,
    BypassPermissions,
}

impl PermissionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::AcceptEdits => "acceptEdits",
            Self::Plan => "plan",
            Self::BypassPermissions => "bypassPermissions",
        }
    }
}

impl std::fmt::Display for PermissionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_result_message_json_format() {
        let result = serde_json::json!([
            {
                "question_index": 0,
                "selected_indices": [1],
                "other_text": null
            }
        ]);

        let message = ToolResultMessage::new("tool-use-123".to_string(), result.clone(), false);

        let json = serde_json::to_value(&message).unwrap();

        assert_eq!(json["type"], "tool_result");
        assert_eq!(json["tool_use_id"], "tool-use-123");
        assert_eq!(json["result"], result);
        assert_eq!(json["is_error"], false);
    }

    #[test]
    fn test_tool_result_message_with_error() {
        let error_result = serde_json::json!({
            "error": "User cancelled the question"
        });

        let message = ToolResultMessage::new("tool-456".to_string(), error_result.clone(), true);

        let json = serde_json::to_value(&message).unwrap();

        assert_eq!(json["type"], "tool_result");
        assert_eq!(json["tool_use_id"], "tool-456");
        assert_eq!(json["result"], error_result);
        assert_eq!(json["is_error"], true);
    }
}
