use std::fmt;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use workspace_utils::approvals::{ApprovalStatus, QuestionData};

use crate::executors::claude::protocol::ProtocolPeer;

/// Errors emitted by executor approval services.
#[derive(Debug, Error)]
pub enum ExecutorApprovalError {
    #[error("executor approval session not registered")]
    SessionNotRegistered,
    #[error("executor approval request failed: {0}")]
    RequestFailed(String),
    #[error("executor approval service unavailable")]
    ServiceUnavailable,
}

impl ExecutorApprovalError {
    pub fn request_failed<E: fmt::Display>(err: E) -> Self {
        Self::RequestFailed(err.to_string())
    }
}

/// Abstraction for executor approval backends.
#[async_trait]
pub trait ExecutorApprovalService: Send + Sync {
    /// Requests approval for a tool invocation and waits for the final decision.
    async fn request_tool_approval(
        &self,
        tool_name: &str,
        tool_input: Value,
        tool_call_id: &str,
    ) -> Result<ApprovalStatus, ExecutorApprovalError>;

    /// Requests user input via questions and waits for answers.
    /// Returns `ApprovalStatus::Answered { answers }` when the user responds,
    /// or `ApprovalStatus::TimedOut` if no response is received in time.
    async fn request_user_question(
        &self,
        questions: Vec<QuestionData>,
        tool_call_id: &str,
    ) -> Result<ApprovalStatus, ExecutorApprovalError>;

    /// Register a protocol peer for sending tool results.
    /// This is called by Claude executor when the protocol peer is created.
    /// The default implementation does nothing (for non-Claude executors).
    async fn register_protocol_peer(&self, _peer: ProtocolPeer) {}

    /// Unregister the protocol peer when execution completes.
    /// The default implementation does nothing.
    async fn unregister_protocol_peer(&self) {}
}

#[derive(Debug, Default)]
pub struct NoopExecutorApprovalService;

#[async_trait]
impl ExecutorApprovalService for NoopExecutorApprovalService {
    async fn request_tool_approval(
        &self,
        _tool_name: &str,
        _tool_input: Value,
        _tool_call_id: &str,
    ) -> Result<ApprovalStatus, ExecutorApprovalError> {
        Ok(ApprovalStatus::Approved)
    }

    async fn request_user_question(
        &self,
        _questions: Vec<QuestionData>,
        _tool_call_id: &str,
    ) -> Result<ApprovalStatus, ExecutorApprovalError> {
        // Noop returns empty answers
        Ok(ApprovalStatus::Answered { answers: vec![] })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallMetadata {
    pub tool_call_id: String,
}
