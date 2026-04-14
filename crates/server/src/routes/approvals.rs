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
use services::services::{
    container::ContainerService,
    domain_events::{
        DomainEvent, DomainEventEntityIds, FollowUpQueueKind, FollowUpScope,
        FollowUpTransitionState,
    },
};
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
        .ok_or_else(|| {
            anyhow::anyhow!("User question not found for approval_id: {}", approval_id)
        })?;

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
        if let Some(dispatcher) = deployment.container().event_dispatch_callback() {
            dispatcher(DomainEvent::FollowUpTransition {
                state: FollowUpTransitionState::Queued,
                scope: FollowUpScope::TaskSession,
                queue_kind: Some(FollowUpQueueKind::Concurrency),
                execution_process_id: None,
                entity_ids: DomainEventEntityIds {
                    task_id: Some(ctx.task.id),
                    workspace_id: Some(ctx.workspace.id),
                    session_id: Some(ctx.session.id),
                    task_group_id: ctx.task.task_group_id,
                    ..DomainEventEntityIds::default()
                },
                occurred_at: chrono::Utc::now(),
            })
            .await;
        }
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
        if let Some(execution_process) = ExecutionProcess::find_by_session_id(pool, ctx.session.id, false)
            .await?
            .into_iter()
            .next_back()
            && let Some(dispatcher) = deployment.container().event_dispatch_callback()
        {
            dispatcher(DomainEvent::FollowUpTransition {
                state: FollowUpTransitionState::Started,
                scope: FollowUpScope::TaskSession,
                queue_kind: None,
                execution_process_id: Some(execution_process.id),
                entity_ids: DomainEventEntityIds {
                    task_id: Some(ctx.task.id),
                    workspace_id: Some(ctx.workspace.id),
                    session_id: Some(ctx.session.id),
                    execution_process_id: Some(execution_process.id),
                    task_group_id: ctx.task.task_group_id,
                },
                occurred_at: chrono::Utc::now(),
            })
            .await;
        }
    }

    Ok(())
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new().route("/approvals/{id}/respond", post(respond_to_approval))
}


#[cfg(test)]
mod tests {
    use std::{path::Path, sync::Arc, time::Duration};

    use axum::{Json, extract::{Path as AxumPath, State}};
    use db::models::{
        execution_process::{CreateExecutionProcess, ExecutionProcess, ExecutionProcessRunReason},
        project::{CreateProject, Project},
        project_repo::ProjectRepo,
        repo::Repo,
        session::{CreateSession, Session},
        task::{CreateTask, Task, TaskStatus},
        user_question::{CreateUserQuestion, UserQuestion},
        workspace::{CreateWorkspace, Workspace},
        workspace_repo::{CreateWorkspaceRepo, WorkspaceRepo},
    };
    use executors::{
        actions::{ExecutorAction, ExecutorActionType, coding_agent_initial::CodingAgentInitialRequest},
        executors::BaseCodingAgent,
        profile::ExecutorProfileId,
    };
    use local_deployment::LocalDeployment;
    use services::services::domain_events::{
        OrchestrationEventPublisherHandle, OrchestrationEventType,
        RecordingOrchestrationEventPublisher,
    };
    use utils::approvals::{ApprovalResponse, ApprovalStatus, QuestionAnswer, QuestionData, QuestionOption};
    use uuid::Uuid;

    use super::*;

    async fn create_project(deployment: &DeploymentImpl, name: &str) -> Project {
        Project::create(
            &deployment.db().pool,
            &CreateProject {
                name: name.to_string(),
                repositories: vec![],
            },
            Uuid::new_v4(),
        )
        .await
        .unwrap()
    }

    async fn create_task(deployment: &DeploymentImpl, project_id: Uuid, title: &str) -> Task {
        Task::create(
            &deployment.db().pool,
            &CreateTask {
                project_id,
                title: title.to_string(),
                description: None,
                status: Some(TaskStatus::InReview),
                parent_workspace_id: None,
                image_ids: None,
                shared_task_id: None,
                task_group_id: None,
            },
            Uuid::new_v4(),
        )
        .await
        .unwrap()
    }

    async fn create_workspace_for_task(
        deployment: &DeploymentImpl,
        task_id: Uuid,
        branch: &str,
    ) -> Workspace {
        Workspace::create(
            &deployment.db().pool,
            &CreateWorkspace {
                branch: branch.to_string(),
                agent_working_dir: None,
            },
            Uuid::new_v4(),
            task_id,
        )
        .await
        .unwrap()
    }

    async fn create_session_for_workspace(
        deployment: &DeploymentImpl,
        workspace_id: Uuid,
    ) -> Session {
        Session::create(
            &deployment.db().pool,
            &CreateSession {
                executor: Some("CLAUDE_CODE".to_string()),
            },
            Uuid::new_v4(),
            workspace_id,
        )
        .await
        .unwrap()
    }

    async fn attach_repo_to_workspace(
        deployment: &DeploymentImpl,
        project_id: Uuid,
        workspace_id: Uuid,
        repo_path: &Path,
        target_branch: &str,
    ) -> Repo {
        let repo = Repo::find_or_create(&deployment.db().pool, repo_path, "Approval Repo")
            .await
            .unwrap();
        ProjectRepo::create(&deployment.db().pool, project_id, repo.id)
            .await
            .unwrap();
        WorkspaceRepo::create_many(
            &deployment.db().pool,
            workspace_id,
            &[CreateWorkspaceRepo {
                repo_id: repo.id,
                target_branch: target_branch.to_string(),
            }],
        )
        .await
        .unwrap();
        repo
    }

    async fn create_running_coding_agent_process(
        deployment: &DeploymentImpl,
        session_id: Uuid,
    ) -> ExecutionProcess {
        ExecutionProcess::create(
            &deployment.db().pool,
            &CreateExecutionProcess {
                session_id,
                executor_action: ExecutorAction::new(
                    ExecutorActionType::CodingAgentInitialRequest(CodingAgentInitialRequest {
                        prompt: "seed".to_string(),
                        executor_profile_id: ExecutorProfileId::new(BaseCodingAgent::ClaudeCode),
                        working_dir: None,
                    }),
                    None,
                ),
                run_reason: ExecutionProcessRunReason::CodingAgent,
            },
            Uuid::new_v4(),
            &[],
        )
        .await
        .unwrap()
    }

    async fn wait_for_event_types(
        publisher: &RecordingOrchestrationEventPublisher,
        minimum: usize,
    ) -> Vec<OrchestrationEventType> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            let events = publisher
                .published()
                .into_iter()
                .map(|(_, envelope)| envelope.event_type)
                .collect::<Vec<_>>();
            if events.len() >= minimum || tokio::time::Instant::now() >= deadline {
                return events;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn approval_route_emits_follow_up_transition_for_answered_question() {
        let _lock = crate::TEST_DB_LOCK.lock().unwrap();
        let publisher = RecordingOrchestrationEventPublisher::default();
        let publisher_handle: OrchestrationEventPublisherHandle = Arc::new(publisher.clone());
        let deployment =
            LocalDeployment::new_with_orchestration_event_publisher(publisher_handle)
                .await
                .unwrap();
        deployment.config().write().await.max_concurrent_agents = 1;

        let project = create_project(&deployment, "approval-route-events").await;
        let task = create_task(&deployment, project.id, "Need answer").await;
        let workspace = create_workspace_for_task(&deployment, task.id, "feature/approval-route").await;
        let session = create_session_for_workspace(&deployment, workspace.id).await;

        let repo_path = std::env::temp_dir()
            .join(format!("vk-approval-route-repo-{}", Uuid::new_v4()));
        deployment
            .git()
            .initialize_repo_with_main_branch(&repo_path)
            .unwrap();
        std::process::Command::new("git")
            .args([
                "-C",
                repo_path.to_str().unwrap(),
                "config",
                "core.fsmonitor",
                "true",
            ])
            .status()
            .unwrap();
        deployment
            .git()
            .create_branch(&repo_path, &workspace.branch, Some("main"))
            .unwrap();
        let _repo = attach_repo_to_workspace(
            &deployment,
            project.id,
            workspace.id,
            &repo_path,
            "main",
        )
        .await;

        let execution_process = create_running_coding_agent_process(&deployment, session.id).await;
        let approval_id = format!("approval-{}", Uuid::new_v4());
        let questions = vec![QuestionData {
            question: "Choose one".to_string(),
            header: None,
            multi_select: false,
            options: vec![QuestionOption {
                label: "A".to_string(),
                description: None,
            }],
        }];
        UserQuestion::create(
            &deployment.db().pool,
            &CreateUserQuestion {
                approval_id: approval_id.clone(),
                execution_process_id: execution_process.id,
                questions: serde_json::to_string(&questions).unwrap(),
            },
            Uuid::new_v4(),
        )
        .await
        .unwrap();

        let status = respond_to_approval(
            State(deployment.clone()),
            AxumPath(approval_id.clone()),
            Json(ApprovalResponse {
                execution_process_id: execution_process.id,
                status: ApprovalStatus::Approved,
                answers: Some(vec![QuestionAnswer {
                    question_index: 0,
                    selected_indices: vec![0],
                    other_text: None,
                }]),
            }),
        )
        .await
        .unwrap()
        .0;

        assert!(matches!(status, ApprovalStatus::Answered { .. }));

        let event_types = wait_for_event_types(&publisher, 2).await;
        assert!(event_types.contains(&OrchestrationEventType::ApprovalResolved));
        assert!(event_types.contains(&OrchestrationEventType::FollowUpTransition));
    }
}
