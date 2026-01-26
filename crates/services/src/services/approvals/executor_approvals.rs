use std::sync::Arc;

use async_trait::async_trait;
use db::{self, DBService, models::execution_process::ExecutionProcess};
use executors::approvals::{ExecutorApprovalError, ExecutorApprovalService};
use serde_json::Value;
use utils::approvals::{ApprovalRequest, ApprovalStatus, CreateApprovalRequest, QuestionData};
use uuid::Uuid;

use crate::services::{approvals::Approvals, notification::NotificationService};

pub struct ExecutorApprovalBridge {
    approvals: Approvals,
    db: DBService,
    notification_service: NotificationService,
    execution_process_id: Uuid,
}

impl ExecutorApprovalBridge {
    pub fn new(
        approvals: Approvals,
        db: DBService,
        notification_service: NotificationService,
        execution_process_id: Uuid,
    ) -> Arc<Self> {
        Arc::new(Self {
            approvals,
            db,
            notification_service,
            execution_process_id,
        })
    }
}

#[async_trait]
impl ExecutorApprovalService for ExecutorApprovalBridge {
    async fn request_tool_approval(
        &self,
        tool_name: &str,
        tool_input: Value,
        tool_call_id: &str,
    ) -> Result<ApprovalStatus, ExecutorApprovalError> {
        super::ensure_task_in_review(&self.db.pool, self.execution_process_id).await;

        let request = ApprovalRequest::from_create(
            CreateApprovalRequest {
                tool_name: tool_name.to_string(),
                tool_input,
                tool_call_id: tool_call_id.to_string(),
            },
            self.execution_process_id,
        );

        let (_, waiter) = self
            .approvals
            .create_with_waiter(request)
            .await
            .map_err(ExecutorApprovalError::request_failed)?;

        // OS notification sound when approval is needed
        self.notification_service
            .notify(
                "Approval Needed",
                &format!("Tool '{}' requires approval", tool_name),
            )
            .await;

        // In-app notification when approval is needed
        if let Ok(ctx) =
            ExecutionProcess::load_context(&self.db.pool, self.execution_process_id).await
            && let Err(e) = NotificationService::notify_agent_approval_needed(
                &self.db.pool,
                ctx.project.id,
                ctx.workspace.id,
                tool_name,
            )
            .await
        {
            tracing::warn!("Failed to create in-app approval notification: {}", e);
        }

        let status = waiter.clone().await;

        if matches!(status, ApprovalStatus::Pending) {
            return Err(ExecutorApprovalError::request_failed(
                "approval finished in pending state",
            ));
        }

        Ok(status)
    }

    async fn request_user_question(
        &self,
        questions: Vec<QuestionData>,
        tool_call_id: &str,
    ) -> Result<ApprovalStatus, ExecutorApprovalError> {
        super::ensure_task_in_review(&self.db.pool, self.execution_process_id).await;

        let request = ApprovalRequest::from_user_question(
            questions.clone(),
            tool_call_id.to_string(),
            self.execution_process_id,
        );

        let (_, waiter) = self
            .approvals
            .create_with_waiter(request)
            .await
            .map_err(ExecutorApprovalError::request_failed)?;

        // OS notification sound when user input is needed
        self.notification_service
            .notify("User Input Needed", "Agent is asking for your input")
            .await;

        // In-app notification when user input is needed
        if let Ok(ctx) =
            ExecutionProcess::load_context(&self.db.pool, self.execution_process_id).await
            && let Err(e) = NotificationService::notify_agent_question(
                &self.db.pool,
                ctx.project.id,
                ctx.workspace.id,
            )
            .await
        {
            tracing::warn!("Failed to create in-app question notification: {}", e);
        }

        let status = waiter.clone().await;

        if matches!(status, ApprovalStatus::Pending) {
            return Err(ExecutorApprovalError::request_failed(
                "user question finished in pending state",
            ));
        }

        Ok(status)
    }
}
