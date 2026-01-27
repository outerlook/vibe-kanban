use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::post,
};
use db::models::{
    execution_process::{ExecutionProcess, ExecutionProcessRunReason},
    execution_queue::ExecutionQueue,
    project_repo::ProjectRepo,
    user_question::UserQuestion,
};
use deployment::Deployment;
use executors::actions::{
    ExecutorAction, ExecutorActionType, coding_agent_follow_up::CodingAgentFollowUpRequest,
};
use services::services::container::ContainerService;
use utils::approvals::{
    ApprovalResponse, ApprovalStatus, QuestionAnswer, QuestionData, format_qa_as_follow_up_prompt,
};

use crate::DeploymentImpl;

pub async fn respond_to_approval(
    State(deployment): State<DeploymentImpl>,
    Path(id): Path<String>,
    Json(request): Json<ApprovalResponse>,
) -> Result<Json<ApprovalStatus>, StatusCode> {
    let service = deployment.approvals();
    let pool = &deployment.db().pool;

    match service.respond(pool, &id, request).await {
        Ok((status, context)) => {
            deployment
                .track_if_analytics_allowed(
                    "approval_responded",
                    serde_json::json!({
                        "approval_id": &id,
                        "status": format!("{:?}", status),
                        "tool_name": context.tool_name,
                        "execution_process_id": context.execution_process_id.to_string(),
                    }),
                )
                .await;

            // If the executor was dead and this is an answered question, trigger follow-up
            if context.needs_follow_up {
                if let ApprovalStatus::Answered { ref answers } = status {
                    if let Err(e) = trigger_follow_up_for_answered_question(
                        &deployment,
                        context.execution_process_id,
                        &id,
                        answers,
                    )
                    .await
                    {
                        tracing::error!(
                            "Failed to trigger follow-up for answered question {}: {:?}",
                            id,
                            e
                        );
                        // Don't fail the request - the answer was saved, follow-up can be retried
                    }
                }
            }

            Ok(Json(status))
        }
        Err(e) => {
            tracing::error!("Failed to respond to approval: {:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Trigger a follow-up execution when a user answers a question but the executor was dead.
async fn trigger_follow_up_for_answered_question(
    deployment: &DeploymentImpl,
    execution_process_id: uuid::Uuid,
    approval_id: &str,
    answers: &[QuestionAnswer],
) -> Result<(), anyhow::Error> {
    let pool = &deployment.db().pool;

    // Load the user question to get the original questions
    let user_question = UserQuestion::get_by_approval_id(pool, approval_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("User question not found for approval_id: {}", approval_id))?;

    // Parse the questions JSON
    let questions: Vec<QuestionData> = serde_json::from_str(&user_question.questions)?;

    // Format the Q&A as a follow-up prompt
    let prompt = format_qa_as_follow_up_prompt(&questions, answers);

    // Load execution context to get workspace, session, etc.
    let ctx = ExecutionProcess::load_context(pool, execution_process_id).await?;

    // Ensure container exists
    deployment
        .container()
        .ensure_container_exists(&ctx.workspace)
        .await?;

    // Get executor profile from the original execution process
    let executor_profile_id =
        ExecutionProcess::latest_executor_profile_for_session(pool, ctx.session.id).await?;

    // Get the latest agent session ID for continuation
    let latest_agent_session_id =
        ExecutionProcess::find_latest_coding_agent_turn_session_id(pool, ctx.session.id).await?;

    // Get project repos for cleanup action
    let project_repos = ProjectRepo::find_by_project_id_with_names(pool, ctx.project.id).await?;
    let cleanup_action = deployment
        .container()
        .cleanup_actions_for_repos(&project_repos);

    let working_dir = ctx
        .workspace
        .agent_working_dir
        .as_ref()
        .filter(|dir| !dir.is_empty())
        .cloned();

    // Build the executor action - use follow-up if we have an agent session, initial otherwise
    let action_type = if let Some(agent_session_id) = latest_agent_session_id {
        ExecutorActionType::CodingAgentFollowUpRequest(CodingAgentFollowUpRequest {
            prompt: prompt.clone(),
            session_id: agent_session_id,
            executor_profile_id: executor_profile_id.clone(),
            working_dir: working_dir.clone(),
        })
    } else {
        ExecutorActionType::CodingAgentInitialRequest(
            executors::actions::coding_agent_initial::CodingAgentInitialRequest {
                prompt,
                executor_profile_id: executor_profile_id.clone(),
                working_dir,
            },
        )
    };

    let action = ExecutorAction::new(action_type, cleanup_action.map(Box::new));

    // Check if we should queue this execution due to concurrency limit
    if deployment.container().should_queue_execution().await? {
        tracing::info!(
            "At concurrency limit, queueing follow-up for answered question {} workspace {}",
            approval_id,
            ctx.workspace.id
        );
        ExecutionQueue::create_follow_up(pool, ctx.workspace.id, ctx.session.id, &action).await?;
    } else {
        tracing::info!(
            "Starting follow-up execution for answered question {} workspace {}",
            approval_id,
            ctx.workspace.id
        );
        deployment
            .container()
            .start_execution(
                &ctx.workspace,
                &ctx.session,
                &action,
                &ExecutionProcessRunReason::CodingAgent,
                None,
            )
            .await?;
    }

    Ok(())
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new().route("/approvals/{id}/respond", post(respond_to_approval))
}
