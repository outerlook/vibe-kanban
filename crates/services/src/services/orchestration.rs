use chrono::{DateTime, Utc};
use db::models::{
    agent_feedback::AgentFeedback,
    conversation_message::{ConversationMessage, ConversationMessageError},
    conversation_session::ConversationSession,
    execution_process::{ExecutionProcess, ExecutionProcessRunReason, ExecutionProcessStatus},
    execution_process_repo_state::ExecutionProcessRepoState,
    execution_queue::ExecutionQueue,
    image::Image,
    review_attention::ReviewAttention,
    session::Session,
    task::{Task, TaskStatus, TaskWithAttemptStatus},
    task_dependency::TaskDependency,
    task_group::{TaskGroup, TaskGroupWithStats, TaskStatusCounts},
    user_question::{UserQuestion, UserQuestionStatus},
    workspace::Workspace,
};
use executors::actions::{ExecutorAction, ExecutorActionType};
use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;
use ts_rs::TS;
use uuid::Uuid;

use super::{
    approvals::Approvals,
    autopilot,
    feedback::{FeedbackHydrationSummary, FeedbackService},
    merge_queue_store::{MergeQueueEntry, MergeQueueStore},
    review_attention::{ReviewAttentionHydrationSummary, ReviewAttentionService},
};

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct OrchestrationTaskContextDto {
    pub task: TaskSnapshotDto,
    pub images: Vec<ImageSnapshotDto>,
    pub latest_workspace: Option<WorkspaceSnapshotDto>,
    pub latest_session: Option<SessionSnapshotDto>,
    pub latest_coding_execution: Option<ExecutionSnapshotDto>,
    pub current_execution_visibility: Option<ExecutionVisibilityDto>,
    pub pending_tool_approvals: Vec<ToolApprovalSnapshotDto>,
    pub pending_questions: Vec<UserQuestionSnapshotDto>,
    pub dependency_context: TaskDependencyContextDto,
    pub latest_review_attention: Option<ReviewAttentionHydrationSummary>,
    pub latest_feedback: Option<FeedbackHydrationSummary>,
    pub queue_state: TaskQueueStateDto,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct OrchestrationTaskGroupContextDto {
    pub task_group: TaskGroupSnapshotDto,
    pub stats: TaskGroupStatsDto,
    pub tasks: Vec<TaskListItemDto>,
    pub dependency_context: TaskGroupDependencyContextDto,
    pub queue_state: TaskGroupQueueStateDto,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct OrchestrationConversationContextDto {
    pub conversation: ConversationSnapshotDto,
    pub transcript: ConversationTranscriptDto,
    pub executions: Vec<ExecutionSnapshotDto>,
    pub current_execution_visibility: ExecutionVisibilityDto,
    pub latest_agent_session_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct OrchestrationExecutionContextDto {
    pub execution: ExecutionSnapshotDto,
    pub scope: ExecutionScopeDto,
    pub repo_states: Vec<ExecutionRepoStateSnapshotDto>,
    pub current_execution_visibility: ExecutionVisibilityDto,
    pub pending_tool_approvals: Vec<ToolApprovalSnapshotDto>,
    pub pending_questions: Vec<UserQuestionSnapshotDto>,
    pub review_attention: Option<ReviewAttentionHydrationSummary>,
    pub feedback: Option<FeedbackHydrationSummary>,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct OrchestrationApprovalContextDto {
    pub approval: ApprovalContextSnapshotDto,
    pub execution: Option<ExecutionSnapshotDto>,
    pub task: Option<TaskSnapshotDto>,
    pub workspace: Option<WorkspaceSnapshotDto>,
    pub session: Option<SessionSnapshotDto>,
    pub current_execution_visibility: Option<ExecutionVisibilityDto>,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct ApprovalContextSnapshotDto {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub execution_process_id: String,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_input: Option<Value>,
    pub questions: Vec<QuestionSnapshotDto>,
    pub answers: Vec<QuestionAnswerSnapshotDto>,
    pub created_at: Option<String>,
    pub timeout_at: Option<String>,
    pub answered_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct TaskSnapshotDto {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub parent_workspace_id: Option<String>,
    pub shared_task_id: Option<String>,
    pub task_group_id: Option<String>,
    pub is_blocked: bool,
    pub has_in_progress_attempt: bool,
    pub last_attempt_failed: bool,
    pub is_queued: bool,
    pub last_executor: String,
    pub needs_attention: Option<bool>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct TaskListItemDto {
    pub id: String,
    pub title: String,
    pub status: String,
    pub task_group_id: Option<String>,
    pub is_blocked: bool,
    pub has_in_progress_attempt: bool,
    pub is_queued: bool,
    pub needs_attention: Option<bool>,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct ImageSnapshotDto {
    pub id: String,
    pub file_path: String,
    pub original_name: String,
    pub mime_type: Option<String>,
    pub size_bytes: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct WorkspaceSnapshotDto {
    pub id: String,
    pub task_id: String,
    pub branch: String,
    pub agent_working_dir: Option<String>,
    pub setup_completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct SessionSnapshotDto {
    pub id: String,
    pub workspace_id: String,
    pub executor: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct ExecutionSnapshotDto {
    pub id: String,
    pub session_id: Option<String>,
    pub conversation_session_id: Option<String>,
    pub run_reason: String,
    pub status: String,
    pub action_type: String,
    pub executor_profile: Option<String>,
    pub exit_code: Option<i64>,
    pub dropped: bool,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct ExecutionVisibilityDto {
    pub latest_execution_id: Option<String>,
    pub latest_visible_execution_id: Option<String>,
    pub running_execution_ids: Vec<String>,
    pub hidden_execution_ids: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct ToolApprovalSnapshotDto {
    pub id: String,
    pub execution_process_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub tool_input: Value,
    pub created_at: String,
    pub timeout_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct UserQuestionSnapshotDto {
    pub id: String,
    pub execution_process_id: String,
    pub status: String,
    pub created_at: String,
    pub answered_at: Option<String>,
    pub questions: Vec<QuestionSnapshotDto>,
    pub answers: Vec<QuestionAnswerSnapshotDto>,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct QuestionSnapshotDto {
    pub header: Option<String>,
    pub question: String,
    pub multi_select: bool,
    pub options: Vec<QuestionOptionSnapshotDto>,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct QuestionOptionSnapshotDto {
    pub label: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct QuestionAnswerSnapshotDto {
    pub question_index: usize,
    pub selected_indices: Vec<usize>,
    pub other_text: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct TaskDependencyContextDto {
    pub blocked_by: Vec<TaskListItemDto>,
    pub blocking: Vec<TaskListItemDto>,
    pub ready_dependents: Vec<TaskListItemDto>,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct TaskQueueStateDto {
    pub execution_queue: Option<ExecutionQueueSnapshotDto>,
    pub merge_queue: Option<MergeQueueEntrySnapshotDto>,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct ExecutionQueueSnapshotDto {
    pub id: String,
    pub workspace_id: String,
    pub executor_profile: String,
    pub queued_at: String,
    pub session_id: Option<String>,
    pub is_follow_up: bool,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct MergeQueueEntrySnapshotDto {
    pub id: String,
    pub project_id: String,
    pub workspace_id: String,
    pub repo_id: String,
    pub status: String,
    pub commit_message: String,
    pub queued_at: String,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct TaskGroupSnapshotDto {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub description: Option<String>,
    pub base_branch: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct TaskGroupStatsDto {
    pub todo: i64,
    pub in_progress: i64,
    pub in_review: i64,
    pub done: i64,
    pub cancelled: i64,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct TaskGroupDependencyContextDto {
    pub blocked_tasks: Vec<TaskListItemDto>,
    pub ready_tasks: Vec<TaskListItemDto>,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct TaskGroupQueueStateDto {
    pub queued_tasks: Vec<TaskListItemDto>,
    pub merge_queue_entries: Vec<MergeQueueEntrySnapshotDto>,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct ConversationSnapshotDto {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub status: String,
    pub executor: Option<String>,
    pub worktree_path: Option<String>,
    pub worktree_branch: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct ConversationTranscriptDto {
    pub messages: Vec<ConversationMessageSnapshotDto>,
    pub images: Vec<ImageSnapshotDto>,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct ConversationMessageSnapshotDto {
    pub id: String,
    pub execution_process_id: Option<String>,
    pub role: String,
    pub content: String,
    pub metadata: Option<Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct ExecutionScopeDto {
    pub task: Option<TaskSnapshotDto>,
    pub workspace: Option<WorkspaceSnapshotDto>,
    pub session: Option<SessionSnapshotDto>,
    pub conversation: Option<ConversationSnapshotDto>,
}

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
pub struct ExecutionRepoStateSnapshotDto {
    pub id: String,
    pub execution_process_id: String,
    pub repo_id: String,
    pub before_head_commit: Option<String>,
    pub after_head_commit: Option<String>,
    pub merge_commit: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub struct OrchestrationService;

impl OrchestrationService {
    pub async fn build_task_context(
        pool: &SqlitePool,
        approvals: &Approvals,
        merge_queue_store: &MergeQueueStore,
        task: Task,
    ) -> Result<OrchestrationTaskContextDto, sqlx::Error> {
        let images = Image::find_by_task_id(pool, task.id).await?;
        let latest_workspace = Workspace::find_latest_by_task_id(pool, task.id).await?;
        let latest_session = match &latest_workspace {
            Some(workspace) => Session::find_latest_by_workspace_id(pool, workspace.id).await?,
            None => None,
        };
        let latest_coding_execution = match &latest_workspace {
            Some(workspace) => {
                ExecutionProcess::find_latest_by_workspace_and_run_reason(
                    pool,
                    workspace.id,
                    &ExecutionProcessRunReason::CodingAgent,
                )
                .await?
            }
            None => None,
        };
        let session_processes = match &latest_session {
            Some(session) => ExecutionProcess::find_by_session_id(pool, session.id, true).await?,
            None => Vec::new(),
        };

        let pending_tool_approvals = collect_tool_approvals(approvals, &session_processes);
        let pending_questions = collect_pending_questions(pool, &session_processes).await?;
        let blocked_by = TaskDependency::find_blocked_by(pool, task.id).await?;
        let blocking = TaskDependency::find_blocking(pool, task.id).await?;
        let ready_dependents = if task.status == TaskStatus::Done {
            autopilot::find_unblocked_dependents(pool, task.id).await?
        } else {
            Vec::new()
        };
        let latest_review_attention = ReviewAttention::find_latest_by_task_id(pool, task.id)
            .await?
            .map(|entry| ReviewAttentionService::summarize_record(&entry));
        let latest_feedback = match &latest_workspace {
            Some(workspace) => AgentFeedback::find_by_workspace_id(pool, workspace.id)
                .await?
                .into_iter()
                .next()
                .map(|entry| FeedbackService::summarize_record(&entry)),
            None => None,
        };
        let execution_queue = match &latest_workspace {
            Some(workspace) => ExecutionQueue::find_by_workspace(pool, workspace.id).await?,
            None => None,
        };
        let queue_state = TaskQueueStateDto {
            execution_queue: execution_queue.map(ExecutionQueueSnapshotDto::from),
            merge_queue: latest_workspace
                .as_ref()
                .and_then(|workspace| merge_queue_store.get(workspace.id))
                .map(MergeQueueEntrySnapshotDto::from),
        };

        Ok(OrchestrationTaskContextDto {
            task: TaskSnapshotDto::from(task),
            images: images.into_iter().map(ImageSnapshotDto::from).collect(),
            latest_workspace: latest_workspace.map(WorkspaceSnapshotDto::from),
            latest_session: latest_session.map(SessionSnapshotDto::from),
            latest_coding_execution: latest_coding_execution.map(ExecutionSnapshotDto::from),
            current_execution_visibility: (!session_processes.is_empty())
                .then(|| ExecutionVisibilityDto::from_processes(&session_processes)),
            pending_tool_approvals,
            pending_questions,
            dependency_context: TaskDependencyContextDto {
                blocked_by: blocked_by.into_iter().map(TaskListItemDto::from).collect(),
                blocking: blocking.into_iter().map(TaskListItemDto::from).collect(),
                ready_dependents: ready_dependents
                    .into_iter()
                    .map(TaskListItemDto::from)
                    .collect(),
            },
            latest_review_attention,
            latest_feedback,
            queue_state,
        })
    }

    pub async fn build_task_group_context(
        pool: &SqlitePool,
        merge_queue_store: &MergeQueueStore,
        task_group: TaskGroup,
    ) -> Result<OrchestrationTaskGroupContextDto, sqlx::Error> {
        let stats = TaskGroup::get_stats_for_project(pool, task_group.project_id)
            .await?
            .into_iter()
            .find(|entry| entry.group.id == task_group.id)
            .unwrap_or(TaskGroupWithStats {
                group: task_group.clone(),
                task_counts: TaskStatusCounts::default(),
            });
        let tasks = Task::find_by_project_id_with_attempt_status(pool, task_group.project_id)
            .await?
            .into_iter()
            .filter(|task| task.task.task_group_id == Some(task_group.id))
            .collect::<Vec<_>>();
        let blocked_tasks = tasks
            .iter()
            .filter(|task| task.task.is_blocked)
            .cloned()
            .map(TaskListItemDto::from)
            .collect();
        let ready_tasks = tasks
            .iter()
            .filter(|task| {
                task.task.status == TaskStatus::Todo
                    && !task.task.is_blocked
                    && !task.task.has_in_progress_attempt
                    && !task.task.is_queued
            })
            .cloned()
            .map(TaskListItemDto::from)
            .collect();
        let queued_tasks = tasks
            .iter()
            .filter(|task| task.task.is_queued)
            .cloned()
            .map(TaskListItemDto::from)
            .collect();
        let workspace_ids = Workspace::fetch_ids_by_task_group(pool, task_group.id).await?;
        let merge_queue_entries = merge_queue_store
            .get_all()
            .into_iter()
            .filter(|entry| workspace_ids.contains(&entry.workspace_id))
            .map(MergeQueueEntrySnapshotDto::from)
            .collect();

        Ok(OrchestrationTaskGroupContextDto {
            task_group: TaskGroupSnapshotDto::from(task_group),
            stats: TaskGroupStatsDto::from(stats.task_counts),
            tasks: tasks.into_iter().map(TaskListItemDto::from).collect(),
            dependency_context: TaskGroupDependencyContextDto {
                blocked_tasks,
                ready_tasks,
            },
            queue_state: TaskGroupQueueStateDto {
                queued_tasks,
                merge_queue_entries,
            },
        })
    }

    pub async fn build_conversation_context(
        pool: &SqlitePool,
        conversation: ConversationSession,
    ) -> Result<OrchestrationConversationContextDto, sqlx::Error> {
        let messages = ConversationMessage::find_by_conversation_session_id(pool, conversation.id)
            .await
            .map_err(|error| match error {
                ConversationMessageError::Database(error) => error,
                _ => sqlx::Error::Protocol(error.to_string()),
            })?;
        let images = Image::find_by_conversation_session_id(pool, conversation.id).await?;
        let executions =
            ExecutionProcess::find_by_conversation_session_id(pool, conversation.id, true).await?;
        let latest_agent_session_id =
            ExecutionProcess::find_latest_conversation_agent_session_id(pool, conversation.id)
                .await?;

        Ok(OrchestrationConversationContextDto {
            conversation: ConversationSnapshotDto::from(conversation),
            transcript: ConversationTranscriptDto {
                messages: messages
                    .into_iter()
                    .map(ConversationMessageSnapshotDto::from)
                    .collect(),
                images: images.into_iter().map(ImageSnapshotDto::from).collect(),
            },
            executions: executions
                .iter()
                .cloned()
                .map(ExecutionSnapshotDto::from)
                .collect(),
            current_execution_visibility: ExecutionVisibilityDto::from_processes(&executions),
            latest_agent_session_id,
        })
    }

    pub async fn build_execution_context(
        pool: &SqlitePool,
        approvals: &Approvals,
        execution: ExecutionProcess,
    ) -> Result<OrchestrationExecutionContextDto, sqlx::Error> {
        let repo_states =
            ExecutionProcessRepoState::find_by_execution_process_id(pool, execution.id).await?;
        let pending_tool_approvals =
            collect_tool_approvals(approvals, std::slice::from_ref(&execution));
        let pending_questions =
            collect_pending_questions(pool, std::slice::from_ref(&execution)).await?;
        let review_attention = ReviewAttention::find_by_execution_process_id(pool, execution.id)
            .await?
            .map(|entry| ReviewAttentionService::summarize_record(&entry));
        let feedback = AgentFeedback::find_by_execution_process_id(pool, execution.id)
            .await?
            .map(|entry| FeedbackService::summarize_record(&entry));

        let (scope, visibility) = if let Some(session_id) = execution.session_id {
            let session = Session::find_by_id(pool, session_id).await?;
            let processes = ExecutionProcess::find_by_session_id(pool, session_id, true).await?;
            let (task, workspace, session) =
                match ExecutionProcess::load_context(pool, execution.id).await {
                    Ok(ctx) => (
                        Some(TaskSnapshotDto::from(ctx.task)),
                        Some(WorkspaceSnapshotDto::from(ctx.workspace)),
                        Some(SessionSnapshotDto::from(ctx.session)),
                    ),
                    Err(_) => (None, None, session.map(SessionSnapshotDto::from)),
                };
            (
                ExecutionScopeDto {
                    task,
                    workspace,
                    session,
                    conversation: None,
                },
                ExecutionVisibilityDto::from_processes(&processes),
            )
        } else if let Some(conversation_session_id) = execution.conversation_session_id {
            let conversation = ConversationSession::find_by_id(pool, conversation_session_id)
                .await
                .ok()
                .flatten()
                .map(ConversationSnapshotDto::from);
            let processes = ExecutionProcess::find_by_conversation_session_id(
                pool,
                conversation_session_id,
                true,
            )
            .await?;
            (
                ExecutionScopeDto {
                    task: None,
                    workspace: None,
                    session: None,
                    conversation,
                },
                ExecutionVisibilityDto::from_processes(&processes),
            )
        } else {
            (
                ExecutionScopeDto {
                    task: None,
                    workspace: None,
                    session: None,
                    conversation: None,
                },
                ExecutionVisibilityDto::from_processes(std::slice::from_ref(&execution)),
            )
        };

        Ok(OrchestrationExecutionContextDto {
            execution: ExecutionSnapshotDto::from(execution),
            scope,
            repo_states: repo_states
                .into_iter()
                .map(ExecutionRepoStateSnapshotDto::from)
                .collect(),
            current_execution_visibility: visibility,
            pending_tool_approvals,
            pending_questions,
            review_attention,
            feedback,
        })
    }

    pub async fn build_approval_context(
        pool: &SqlitePool,
        approvals: &Approvals,
        approval_id: &str,
    ) -> Result<Option<OrchestrationApprovalContextDto>, sqlx::Error> {
        if let Some(question) = UserQuestion::get_by_approval_id(pool, approval_id).await? {
            let execution =
                ExecutionProcess::find_by_id(pool, question.execution_process_id).await?;
            let (task, workspace, session, visibility) = match &execution {
                Some(execution) if execution.session_id.is_some() => {
                    if let Ok(ctx) = ExecutionProcess::load_context(pool, execution.id).await {
                        let processes =
                            ExecutionProcess::find_by_session_id(pool, ctx.session.id, true)
                                .await?;
                        (
                            Some(TaskSnapshotDto::from(ctx.task)),
                            Some(WorkspaceSnapshotDto::from(ctx.workspace)),
                            Some(SessionSnapshotDto::from(ctx.session)),
                            Some(ExecutionVisibilityDto::from_processes(&processes)),
                        )
                    } else {
                        (None, None, None, None)
                    }
                }
                _ => (None, None, None, None),
            };

            return Ok(Some(OrchestrationApprovalContextDto {
                approval: ApprovalContextSnapshotDto::from_user_question(
                    approvals,
                    &question,
                    execution.as_ref(),
                ),
                execution: execution.map(ExecutionSnapshotDto::from),
                task,
                workspace,
                session,
                current_execution_visibility: visibility,
            }));
        }

        let Some(request) = approvals.find_request(approval_id) else {
            return Ok(None);
        };
        let execution = ExecutionProcess::find_by_id(pool, request.execution_process_id).await?;
        let (task, workspace, session, visibility) = match &execution {
            Some(execution) if execution.session_id.is_some() => {
                if let Ok(ctx) = ExecutionProcess::load_context(pool, execution.id).await {
                    let processes =
                        ExecutionProcess::find_by_session_id(pool, ctx.session.id, true).await?;
                    (
                        Some(TaskSnapshotDto::from(ctx.task)),
                        Some(WorkspaceSnapshotDto::from(ctx.workspace)),
                        Some(SessionSnapshotDto::from(ctx.session)),
                        Some(ExecutionVisibilityDto::from_processes(&processes)),
                    )
                } else {
                    (None, None, None, None)
                }
            }
            _ => (None, None, None, None),
        };

        Ok(Some(OrchestrationApprovalContextDto {
            approval: ApprovalContextSnapshotDto::from_request(
                request,
                approvals
                    .find_status(approval_id)
                    .unwrap_or(utils::approvals::ApprovalStatus::Pending),
            ),
            execution: execution.map(ExecutionSnapshotDto::from),
            task,
            workspace,
            session,
            current_execution_visibility: visibility,
        }))
    }
}

fn collect_tool_approvals(
    approvals: &Approvals,
    executions: &[ExecutionProcess],
) -> Vec<ToolApprovalSnapshotDto> {
    executions
        .iter()
        .flat_map(|execution| approvals.list_pending_requests_for_execution_process(execution.id))
        .filter_map(|request| match request.request_type {
            utils::approvals::ApprovalRequestType::ToolApproval {
                tool_name,
                tool_input,
            } => Some(ToolApprovalSnapshotDto {
                id: request.id,
                execution_process_id: normalize_uuid(request.execution_process_id),
                tool_call_id: request.tool_call_id,
                tool_name,
                tool_input,
                created_at: normalize_ts(request.created_at),
                timeout_at: request.timeout_at.map(normalize_ts),
            }),
            utils::approvals::ApprovalRequestType::UserQuestion { .. } => None,
        })
        .collect()
}

async fn collect_pending_questions(
    pool: &SqlitePool,
    executions: &[ExecutionProcess],
) -> Result<Vec<UserQuestionSnapshotDto>, sqlx::Error> {
    let mut questions = Vec::new();
    for execution in executions {
        let execution_questions = UserQuestion::get_by_execution_process_id(pool, execution.id)
            .await?
            .into_iter()
            .filter(|question| question.status_enum() == UserQuestionStatus::Pending)
            .map(UserQuestionSnapshotDto::from)
            .collect::<Vec<_>>();
        questions.extend(execution_questions);
    }
    Ok(questions)
}

fn normalize_uuid(value: Uuid) -> String {
    value.to_string()
}

fn normalize_ts(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}

fn enum_name<T>(value: &T) -> String
where
    T: Serialize,
{
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_default()
}

fn execution_action_type(action: &ExecutorAction) -> String {
    match &action.typ {
        ExecutorActionType::CodingAgentInitialRequest(_) => "coding_agent_initial".to_string(),
        ExecutorActionType::CodingAgentFollowUpRequest(_) => "coding_agent_follow_up".to_string(),
        ExecutorActionType::ScriptRequest(_) => "script".to_string(),
    }
}

fn execution_profile(action: &ExecutorAction) -> Option<String> {
    match &action.typ {
        ExecutorActionType::CodingAgentInitialRequest(request) => {
            Some(request.executor_profile_id.to_string())
        }
        ExecutorActionType::CodingAgentFollowUpRequest(request) => {
            Some(request.executor_profile_id.to_string())
        }
        ExecutorActionType::ScriptRequest(_) => None,
    }
}

fn approval_status_name(status: &utils::approvals::ApprovalStatus) -> String {
    match status {
        utils::approvals::ApprovalStatus::Pending => "pending",
        utils::approvals::ApprovalStatus::Approved => "approved",
        utils::approvals::ApprovalStatus::Denied { .. } => "denied",
        utils::approvals::ApprovalStatus::Answered { .. } => "answered",
        utils::approvals::ApprovalStatus::TimedOut => "timed_out",
    }
    .to_string()
}

impl ExecutionVisibilityDto {
    pub fn from_processes(processes: &[ExecutionProcess]) -> Self {
        let latest_execution_id = processes.last().map(|process| normalize_uuid(process.id));
        let latest_visible_execution_id = processes
            .iter()
            .rev()
            .find(|process| !process.dropped)
            .map(|process| normalize_uuid(process.id));

        Self {
            latest_execution_id,
            latest_visible_execution_id,
            running_execution_ids: processes
                .iter()
                .filter(|process| process.status == ExecutionProcessStatus::Running)
                .map(|process| normalize_uuid(process.id))
                .collect(),
            hidden_execution_ids: processes
                .iter()
                .filter(|process| process.dropped)
                .map(|process| normalize_uuid(process.id))
                .collect(),
        }
    }
}

impl ApprovalContextSnapshotDto {
    fn from_request(
        request: utils::approvals::ApprovalRequest,
        status: utils::approvals::ApprovalStatus,
    ) -> Self {
        match request.request_type {
            utils::approvals::ApprovalRequestType::ToolApproval {
                tool_name,
                tool_input,
            } => Self {
                id: request.id,
                kind: "tool_approval".to_string(),
                status: approval_status_name(&status),
                execution_process_id: normalize_uuid(request.execution_process_id),
                tool_call_id: Some(request.tool_call_id),
                tool_name: Some(tool_name),
                tool_input: Some(tool_input),
                questions: Vec::new(),
                answers: Vec::new(),
                created_at: Some(normalize_ts(request.created_at)),
                timeout_at: request.timeout_at.map(normalize_ts),
                answered_at: None,
            },
            utils::approvals::ApprovalRequestType::UserQuestion { questions } => Self {
                id: request.id,
                kind: "user_question".to_string(),
                status: approval_status_name(&status),
                execution_process_id: normalize_uuid(request.execution_process_id),
                tool_call_id: Some(request.tool_call_id),
                tool_name: None,
                tool_input: None,
                questions: questions
                    .into_iter()
                    .map(QuestionSnapshotDto::from)
                    .collect(),
                answers: Vec::new(),
                created_at: Some(normalize_ts(request.created_at)),
                timeout_at: request.timeout_at.map(normalize_ts),
                answered_at: None,
            },
        }
    }

    fn from_user_question(
        approvals: &Approvals,
        question: &UserQuestion,
        execution: Option<&ExecutionProcess>,
    ) -> Self {
        let request = approvals.find_request(&question.approval_id);
        let status = request
            .as_ref()
            .map(|_| utils::approvals::ApprovalStatus::Pending)
            .or_else(|| approvals.find_status(&question.approval_id))
            .unwrap_or_else(|| match question.status_enum() {
                UserQuestionStatus::Pending => utils::approvals::ApprovalStatus::Pending,
                UserQuestionStatus::Answered => utils::approvals::ApprovalStatus::Answered {
                    answers: parse_answers(&question.answers),
                },
                UserQuestionStatus::Expired => utils::approvals::ApprovalStatus::TimedOut,
            });

        Self {
            id: question.approval_id.clone(),
            kind: "user_question".to_string(),
            status: approval_status_name(&status),
            execution_process_id: normalize_uuid(
                execution
                    .map(|execution| execution.id)
                    .unwrap_or(question.execution_process_id),
            ),
            tool_call_id: request.as_ref().map(|request| request.tool_call_id.clone()),
            tool_name: None,
            tool_input: None,
            questions: parse_questions(&question.questions),
            answers: parse_answers(&question.answers)
                .into_iter()
                .map(QuestionAnswerSnapshotDto::from)
                .collect(),
            created_at: request
                .map(|request| normalize_ts(request.created_at))
                .or_else(|| Some(normalize_ts(question.created_at))),
            timeout_at: None,
            answered_at: question.answered_at.map(normalize_ts),
        }
    }
}

fn parse_questions(raw: &str) -> Vec<QuestionSnapshotDto> {
    serde_json::from_str::<Vec<utils::approvals::QuestionData>>(raw)
        .unwrap_or_default()
        .into_iter()
        .map(QuestionSnapshotDto::from)
        .collect()
}

fn parse_answers(raw: &Option<String>) -> Vec<utils::approvals::QuestionAnswer> {
    raw.as_deref()
        .and_then(|raw| serde_json::from_str::<Vec<utils::approvals::QuestionAnswer>>(raw).ok())
        .unwrap_or_default()
}

impl From<Task> for TaskSnapshotDto {
    fn from(task: Task) -> Self {
        Self {
            id: normalize_uuid(task.id),
            project_id: normalize_uuid(task.project_id),
            title: task.title,
            description: task.description,
            status: enum_name(&task.status),
            parent_workspace_id: task.parent_workspace_id.map(normalize_uuid),
            shared_task_id: task.shared_task_id.map(normalize_uuid),
            task_group_id: task.task_group_id.map(normalize_uuid),
            is_blocked: task.is_blocked,
            has_in_progress_attempt: task.has_in_progress_attempt,
            last_attempt_failed: task.last_attempt_failed,
            is_queued: task.is_queued,
            last_executor: task.last_executor,
            needs_attention: task.needs_attention,
            created_at: normalize_ts(task.created_at),
            updated_at: normalize_ts(task.updated_at),
        }
    }
}

impl From<Task> for TaskListItemDto {
    fn from(task: Task) -> Self {
        Self {
            id: normalize_uuid(task.id),
            title: task.title,
            status: enum_name(&task.status),
            task_group_id: task.task_group_id.map(normalize_uuid),
            is_blocked: task.is_blocked,
            has_in_progress_attempt: task.has_in_progress_attempt,
            is_queued: task.is_queued,
            needs_attention: task.needs_attention,
        }
    }
}

impl From<TaskWithAttemptStatus> for TaskListItemDto {
    fn from(task: TaskWithAttemptStatus) -> Self {
        TaskListItemDto::from(task.task)
    }
}

impl From<Image> for ImageSnapshotDto {
    fn from(image: Image) -> Self {
        Self {
            id: normalize_uuid(image.id),
            file_path: format!("{}/{}", utils::path::VIBE_IMAGES_DIR, image.file_path),
            original_name: image.original_name,
            mime_type: image.mime_type,
            size_bytes: image.size_bytes,
            created_at: normalize_ts(image.created_at),
        }
    }
}

impl From<Workspace> for WorkspaceSnapshotDto {
    fn from(workspace: Workspace) -> Self {
        Self {
            id: normalize_uuid(workspace.id),
            task_id: normalize_uuid(workspace.task_id),
            branch: workspace.branch,
            agent_working_dir: workspace.agent_working_dir,
            setup_completed_at: workspace.setup_completed_at.map(normalize_ts),
            created_at: normalize_ts(workspace.created_at),
            updated_at: normalize_ts(workspace.updated_at),
        }
    }
}

impl From<Session> for SessionSnapshotDto {
    fn from(session: Session) -> Self {
        Self {
            id: normalize_uuid(session.id),
            workspace_id: normalize_uuid(session.workspace_id),
            executor: session.executor,
            created_at: normalize_ts(session.created_at),
            updated_at: normalize_ts(session.updated_at),
        }
    }
}

impl From<ExecutionProcess> for ExecutionSnapshotDto {
    fn from(execution: ExecutionProcess) -> Self {
        let action = execution.executor_action().ok();
        Self {
            id: normalize_uuid(execution.id),
            session_id: execution.session_id.map(normalize_uuid),
            conversation_session_id: execution.conversation_session_id.map(normalize_uuid),
            run_reason: enum_name(&execution.run_reason),
            status: enum_name(&execution.status),
            action_type: action
                .map(execution_action_type)
                .unwrap_or_else(|| "unknown".to_string()),
            executor_profile: action.and_then(execution_profile),
            exit_code: execution.exit_code,
            dropped: execution.dropped,
            input_tokens: execution.input_tokens,
            output_tokens: execution.output_tokens,
            started_at: normalize_ts(execution.started_at),
            completed_at: execution.completed_at.map(normalize_ts),
            created_at: normalize_ts(execution.created_at),
            updated_at: normalize_ts(execution.updated_at),
        }
    }
}

impl From<ExecutionQueue> for ExecutionQueueSnapshotDto {
    fn from(queue: ExecutionQueue) -> Self {
        Self {
            id: normalize_uuid(queue.id),
            workspace_id: normalize_uuid(queue.workspace_id),
            executor_profile: queue.executor_profile_id.0.to_string(),
            queued_at: normalize_ts(queue.queued_at),
            session_id: queue.session_id.map(normalize_uuid),
            is_follow_up: queue.is_follow_up(),
        }
    }
}

impl From<MergeQueueEntry> for MergeQueueEntrySnapshotDto {
    fn from(entry: MergeQueueEntry) -> Self {
        Self {
            id: normalize_uuid(entry.id),
            project_id: normalize_uuid(entry.project_id),
            workspace_id: normalize_uuid(entry.workspace_id),
            repo_id: normalize_uuid(entry.repo_id),
            status: enum_name(&entry.status),
            commit_message: entry.commit_message,
            queued_at: normalize_ts(entry.queued_at),
        }
    }
}

impl From<TaskGroup> for TaskGroupSnapshotDto {
    fn from(task_group: TaskGroup) -> Self {
        Self {
            id: normalize_uuid(task_group.id),
            project_id: normalize_uuid(task_group.project_id),
            name: task_group.name,
            description: task_group.description,
            base_branch: task_group.base_branch,
            created_at: normalize_ts(task_group.created_at),
            updated_at: normalize_ts(task_group.updated_at),
        }
    }
}

impl From<TaskStatusCounts> for TaskGroupStatsDto {
    fn from(counts: TaskStatusCounts) -> Self {
        Self {
            todo: counts.todo,
            in_progress: counts.inprogress,
            in_review: counts.inreview,
            done: counts.done,
            cancelled: counts.cancelled,
        }
    }
}

impl From<ConversationSession> for ConversationSnapshotDto {
    fn from(conversation: ConversationSession) -> Self {
        Self {
            id: normalize_uuid(conversation.id),
            project_id: normalize_uuid(conversation.project_id),
            title: conversation.title,
            status: enum_name(&conversation.status),
            executor: conversation.executor,
            worktree_path: conversation.worktree_path,
            worktree_branch: conversation.worktree_branch,
            created_at: normalize_ts(conversation.created_at),
            updated_at: normalize_ts(conversation.updated_at),
        }
    }
}

impl From<ConversationMessage> for ConversationMessageSnapshotDto {
    fn from(message: ConversationMessage) -> Self {
        Self {
            id: normalize_uuid(message.id),
            execution_process_id: message.execution_process_id.map(normalize_uuid),
            role: enum_name(&message.role),
            content: message.content,
            metadata: message
                .metadata
                .as_deref()
                .and_then(|value| serde_json::from_str::<Value>(value).ok()),
            created_at: normalize_ts(message.created_at),
        }
    }
}

impl From<ExecutionProcessRepoState> for ExecutionRepoStateSnapshotDto {
    fn from(state: ExecutionProcessRepoState) -> Self {
        Self {
            id: normalize_uuid(state.id),
            execution_process_id: normalize_uuid(state.execution_process_id),
            repo_id: normalize_uuid(state.repo_id),
            before_head_commit: state.before_head_commit,
            after_head_commit: state.after_head_commit,
            merge_commit: state.merge_commit,
            created_at: normalize_ts(state.created_at),
            updated_at: normalize_ts(state.updated_at),
        }
    }
}

impl From<utils::approvals::QuestionData> for QuestionSnapshotDto {
    fn from(question: utils::approvals::QuestionData) -> Self {
        Self {
            header: question.header,
            question: question.question,
            multi_select: question.multi_select,
            options: question
                .options
                .into_iter()
                .map(QuestionOptionSnapshotDto::from)
                .collect(),
        }
    }
}

impl From<utils::approvals::QuestionOption> for QuestionOptionSnapshotDto {
    fn from(option: utils::approvals::QuestionOption) -> Self {
        Self {
            label: option.label,
            description: option.description,
        }
    }
}

impl From<utils::approvals::QuestionAnswer> for QuestionAnswerSnapshotDto {
    fn from(answer: utils::approvals::QuestionAnswer) -> Self {
        Self {
            question_index: answer.question_index,
            selected_indices: answer.selected_indices,
            other_text: answer.other_text,
        }
    }
}

impl From<UserQuestion> for UserQuestionSnapshotDto {
    fn from(question: UserQuestion) -> Self {
        Self {
            id: question.approval_id,
            execution_process_id: normalize_uuid(question.execution_process_id),
            status: question.status,
            created_at: normalize_ts(question.created_at),
            answered_at: question.answered_at.map(normalize_ts),
            questions: parse_questions(&question.questions),
            answers: parse_answers(&question.answers)
                .into_iter()
                .map(QuestionAnswerSnapshotDto::from)
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn execution_visibility_normalizes_hidden_and_running_ids() {
        let base = Utc.with_ymd_and_hms(2026, 2, 1, 10, 0, 0).unwrap();
        let make = |status, dropped, minutes| ExecutionProcess {
            id: Uuid::new_v4(),
            session_id: Some(Uuid::new_v4()),
            conversation_session_id: None,
            run_reason: ExecutionProcessRunReason::CodingAgent,
            executor_action: sqlx::types::Json(
                db::models::execution_process::ExecutorActionField::Other(Value::Null),
            ),
            status,
            exit_code: None,
            dropped,
            input_tokens: None,
            output_tokens: None,
            started_at: base + chrono::Duration::minutes(minutes),
            completed_at: None,
            created_at: base + chrono::Duration::minutes(minutes),
            updated_at: base + chrono::Duration::minutes(minutes),
        };

        let first = make(ExecutionProcessStatus::Completed, false, 0);
        let second = make(ExecutionProcessStatus::Running, true, 1);
        let third = make(ExecutionProcessStatus::Running, false, 2);

        let visibility =
            ExecutionVisibilityDto::from_processes(&[first.clone(), second.clone(), third.clone()]);

        assert_eq!(visibility.latest_execution_id, Some(third.id.to_string()));
        assert_eq!(
            visibility.latest_visible_execution_id,
            Some(third.id.to_string())
        );
        assert_eq!(visibility.running_execution_ids.len(), 2);
        assert_eq!(visibility.hidden_execution_ids, vec![second.id.to_string()]);
    }

    #[test]
    fn task_snapshot_normalizes_ids_and_timestamps() {
        let created_at = Utc.with_ymd_and_hms(2026, 2, 1, 10, 0, 0).unwrap();
        let updated_at = Utc.with_ymd_and_hms(2026, 2, 1, 11, 0, 0).unwrap();
        let task = Task {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            title: "hydrate me".to_string(),
            description: Some("compact task".to_string()),
            status: TaskStatus::InReview,
            parent_workspace_id: Some(Uuid::new_v4()),
            shared_task_id: None,
            task_group_id: Some(Uuid::new_v4()),
            created_at,
            updated_at,
            is_blocked: false,
            has_in_progress_attempt: true,
            last_attempt_failed: false,
            is_queued: true,
            last_executor: "CLAUDE_CODE".to_string(),
            needs_attention: Some(true),
        };

        let snapshot = TaskSnapshotDto::from(task.clone());

        assert_eq!(snapshot.id, task.id.to_string());
        assert_eq!(snapshot.project_id, task.project_id.to_string());
        assert_eq!(snapshot.status, "inreview");
        assert_eq!(snapshot.created_at, created_at.to_rfc3339());
        assert_eq!(snapshot.updated_at, updated_at.to_rfc3339());
    }
}
