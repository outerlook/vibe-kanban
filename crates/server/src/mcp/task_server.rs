use std::{future::Future, str::FromStr};

use db::models::{
    project::Project,
    repo::Repo,
    tag::Tag,
    task::{CreateTask, Task, TaskStatus, TaskWithAttemptStatus, UpdateTask},
    task_dependency::TaskDependency,
    task_group::TaskGroup,
    workspace::{Workspace, WorkspaceContext},
};
use executors::{executors::BaseCodingAgent, profile::ExecutorProfileId};
use regex::Regex;
use reqwest::StatusCode;
use rmcp::{
    ErrorData, ServerHandler,
    handler::server::tool::{Parameters, ToolRouter},
    model::{
        CallToolResult, Content, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo,
    },
    tool, tool_handler, tool_router,
};
use schemars;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json;
use uuid::Uuid;

use crate::routes::{
    containers::ContainerQuery,
    task_attempts::{CreateTaskAttemptBody, WorkspaceRepoInput},
};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateTaskRequest {
    #[schemars(description = "The ID of the project to create the task in. This is required!")]
    pub project_id: Uuid,
    #[schemars(description = "The title of the task")]
    pub title: String,
    #[schemars(description = "Optional description of the task")]
    pub description: Option<String>,
    #[schemars(description = "Optional task group ID to assign this task to")]
    pub task_group_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateTaskWithDepsRequest {
    #[schemars(description = "The ID of the project to create the task in. This is required!")]
    pub project_id: Uuid,
    #[schemars(description = "The title of the task")]
    pub title: String,
    #[schemars(description = "Optional description of the task")]
    pub description: Option<String>,
    #[schemars(description = "Task IDs this task is blocked by")]
    pub depends_on: Option<Vec<Uuid>>,
    #[schemars(description = "Optional task group ID to assign this task to")]
    pub task_group_id: Option<Uuid>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CreateTaskResponse {
    pub task_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TaskDefinition {
    #[schemars(description = "The title of the task")]
    pub title: String,
    #[schemars(description = "Optional description of the task")]
    pub description: Option<String>,
    #[schemars(description = "Reference to another task in this batch by index (0-based)")]
    pub depends_on_indices: Option<Vec<usize>>,
    #[schemars(description = "Optional task group ID to assign this task to")]
    pub task_group_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BulkCreateTasksRequest {
    #[schemars(description = "The ID of the project to create the tasks in")]
    pub project_id: Uuid,
    #[schemars(description = "Tasks to create in order")]
    pub tasks: Vec<TaskDefinition>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct BulkCreateTasksResponse {
    pub task_ids: Vec<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ProjectSummary {
    #[schemars(description = "The unique identifier of the project")]
    pub id: String,
    #[schemars(description = "The name of the project")]
    pub name: String,
    #[schemars(description = "When the project was created")]
    pub created_at: String,
    #[schemars(description = "When the project was last updated")]
    pub updated_at: String,
}

impl ProjectSummary {
    fn from_project(project: Project) -> Self {
        Self {
            id: project.id.to_string(),
            name: project.name,
            created_at: project.created_at.to_rfc3339(),
            updated_at: project.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct McpRepoSummary {
    #[schemars(description = "The unique identifier of the repository")]
    pub id: String,
    #[schemars(description = "The name of the repository")]
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListReposRequest {
    #[schemars(description = "The ID of the project to list repositories from")]
    pub project_id: Uuid,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ListReposResponse {
    pub repos: Vec<McpRepoSummary>,
    pub count: usize,
    pub project_id: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ListProjectsResponse {
    pub projects: Vec<ProjectSummary>,
    pub count: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListTasksRequest {
    #[schemars(description = "The ID of the project to list tasks from")]
    pub project_id: Uuid,
    #[schemars(description = "Optional text search query to filter tasks by title or description")]
    pub query: Option<String>,
    #[schemars(
        description = "Optional status filter: 'todo', 'inprogress', 'inreview', 'done', 'cancelled'"
    )]
    pub status: Option<String>,
    #[schemars(description = "Maximum number of tasks to return (default: 50)")]
    pub limit: Option<i32>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TaskSummary {
    #[schemars(description = "The unique identifier of the task")]
    pub id: String,
    #[schemars(description = "The title of the task")]
    pub title: String,
    #[schemars(description = "Current status of the task")]
    pub status: String,
    #[schemars(description = "When the task was created")]
    pub created_at: String,
    #[schemars(description = "When the task was last updated")]
    pub updated_at: String,
    #[schemars(description = "Whether the task has an in-progress execution attempt")]
    pub has_in_progress_attempt: Option<bool>,
    #[schemars(description = "Whether the last execution attempt failed")]
    pub last_attempt_failed: Option<bool>,
    #[schemars(description = "The task group this task belongs to, if any")]
    pub task_group_id: Option<String>,
}

impl TaskSummary {
    fn from_task_with_status(task: TaskWithAttemptStatus) -> Self {
        Self {
            id: task.id.to_string(),
            title: task.title.to_string(),
            status: task.status.to_string(),
            created_at: task.created_at.to_rfc3339(),
            updated_at: task.updated_at.to_rfc3339(),
            has_in_progress_attempt: Some(task.has_in_progress_attempt),
            last_attempt_failed: Some(task.last_attempt_failed),
            task_group_id: task.task_group_id.map(|id| id.to_string()),
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TaskDetails {
    #[schemars(description = "The unique identifier of the task")]
    pub id: String,
    #[schemars(description = "The title of the task")]
    pub title: String,
    #[schemars(description = "Optional description of the task")]
    pub description: Option<String>,
    #[schemars(description = "Current status of the task")]
    pub status: String,
    #[schemars(description = "When the task was created")]
    pub created_at: String,
    #[schemars(description = "When the task was last updated")]
    pub updated_at: String,
    #[schemars(description = "Whether the task has an in-progress execution attempt")]
    pub has_in_progress_attempt: Option<bool>,
    #[schemars(description = "Whether the last execution attempt failed")]
    pub last_attempt_failed: Option<bool>,
    #[schemars(description = "The task group this task belongs to, if any")]
    pub task_group_id: Option<String>,
}

impl TaskDetails {
    fn from_task(task: Task) -> Self {
        Self {
            id: task.id.to_string(),
            title: task.title,
            description: task.description,
            status: task.status.to_string(),
            created_at: task.created_at.to_rfc3339(),
            updated_at: task.updated_at.to_rfc3339(),
            has_in_progress_attempt: None,
            last_attempt_failed: None,
            task_group_id: task.task_group_id.map(|id| id.to_string()),
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ListTasksResponse {
    pub tasks: Vec<TaskSummary>,
    pub count: usize,
    pub project_id: String,
    pub applied_filters: ListTasksFilters,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ListTasksFilters {
    pub query: Option<String>,
    pub status: Option<String>,
    pub limit: i32,
}

#[derive(Debug, Deserialize)]
struct PaginatedTasksResponse {
    pub tasks: Vec<TaskWithAttemptStatus>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateTaskRequest {
    #[schemars(description = "The ID of the task to update")]
    pub task_id: Uuid,
    #[schemars(description = "New title for the task")]
    pub title: Option<String>,
    #[schemars(description = "New description for the task")]
    pub description: Option<String>,
    #[schemars(description = "New status: 'todo', 'inprogress', 'inreview', 'done', 'cancelled'")]
    pub status: Option<String>,
    #[schemars(
        description = "Task group ID to assign this task to. Pass null to remove from group."
    )]
    pub task_group_id: Option<Uuid>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct UpdateTaskResponse {
    pub task: TaskDetails,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteTaskRequest {
    #[schemars(description = "The ID of the task to delete")]
    pub task_id: Uuid,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct McpWorkspaceRepoInput {
    #[schemars(description = "The repository ID")]
    pub repo_id: Uuid,
    #[schemars(description = "The base branch for this repository")]
    pub base_branch: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StartWorkspaceSessionRequest {
    #[schemars(description = "The ID of the task to start")]
    pub task_id: Uuid,
    #[schemars(
        description = "The coding agent executor to run ('CLAUDE_CODE', 'CODEX', 'GEMINI', 'CURSOR_AGENT', 'OPENCODE')"
    )]
    pub executor: String,
    #[schemars(description = "Optional executor variant, if needed")]
    pub variant: Option<String>,
    #[schemars(description = "Base branch for each repository in the project")]
    pub repos: Vec<McpWorkspaceRepoInput>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct StartWorkspaceSessionResponse {
    pub task_id: String,
    pub workspace_id: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct DeleteTaskResponse {
    pub deleted_task_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetTaskRequest {
    #[schemars(description = "The ID of the task to retrieve")]
    pub task_id: Uuid,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct GetTaskResponse {
    pub task: TaskDetails,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AddTaskDependencyRequest {
    #[schemars(description = "The task to add a dependency for")]
    pub task_id: Uuid,
    #[schemars(description = "The task this one depends on")]
    pub depends_on_id: Uuid,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RemoveTaskDependencyRequest {
    #[schemars(description = "The task to remove a dependency from")]
    pub task_id: Uuid,
    #[schemars(description = "The dependency task to remove")]
    pub depends_on_id: Uuid,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetTaskDependenciesRequest {
    #[schemars(description = "The task to fetch dependencies for")]
    pub task_id: Uuid,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TaskDependencyInfo {
    pub id: String,
    pub task_id: String,
    pub depends_on_id: String,
    pub created_at: String,
}

impl TaskDependencyInfo {
    fn from_dependency(dependency: TaskDependency) -> Self {
        Self {
            id: dependency.id.to_string(),
            task_id: dependency.task_id.to_string(),
            depends_on_id: dependency.depends_on_id.to_string(),
            created_at: dependency.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct RemoveTaskDependencyResponse {
    pub task_id: String,
    pub depends_on_id: String,
    pub removed: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TaskDependencySummary {
    pub blocked_by: Vec<TaskDetails>,
    pub blocking: Vec<TaskDetails>,
    pub is_blocked: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetTaskDependencyTreeRequest {
    #[schemars(description = "The task to fetch the dependency tree for")]
    pub task_id: Uuid,
    #[schemars(description = "Maximum depth to traverse (defaults to server value)")]
    pub max_depth: Option<i32>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TaskDependencyTreeNode {
    pub task: TaskDetails,
    pub dependencies: Vec<TaskDependencyTreeNode>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct GetTaskDependencyTreeResponse {
    pub tree: TaskDependencyTreeNode,
}

#[derive(Debug, Deserialize)]
struct TaskDependencyTreeNodeApi {
    task: Task,
    dependencies: Vec<TaskDependencyTreeNodeApi>,
}

impl TaskDependencyTreeNode {
    fn from_api(node: TaskDependencyTreeNodeApi) -> Self {
        Self {
            task: TaskDetails::from_task(node.task),
            dependencies: node
                .dependencies
                .into_iter()
                .map(TaskDependencyTreeNode::from_api)
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetTaskDependencyContextRequest {
    #[schemars(description = "The task to fetch dependency context for")]
    pub task_id: Uuid,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct GetTaskDependencyContextResponse {
    pub task_id: String,
    pub ancestors: Vec<TaskDetails>,
    pub descendants: Vec<TaskDetails>,
}

#[derive(Debug, Deserialize)]
struct TaskDependencyContextApi {
    ancestors: Vec<Task>,
    descendants: Vec<Task>,
}

// ============================================================================
// Task Group MCP Types
// ============================================================================

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListTaskGroupsRequest {
    #[schemars(description = "The ID of the project to list task groups from")]
    pub project_id: Uuid,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ListTaskGroupsResponse {
    pub task_groups: Vec<TaskGroupSummary>,
    pub count: usize,
    pub project_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateTaskGroupRequest {
    #[schemars(description = "The ID of the project to create the task group in")]
    pub project_id: Uuid,
    #[schemars(description = "The name of the task group")]
    pub name: String,
    #[schemars(description = "Optional description of the task group")]
    pub description: Option<String>,
    #[schemars(description = "Optional base branch for tasks in this group")]
    pub base_branch: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CreateTaskGroupResponse {
    pub task_group: TaskGroupSummary,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetTaskGroupRequest {
    #[schemars(description = "The ID of the task group to retrieve")]
    pub group_id: Uuid,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct GetTaskGroupResponse {
    pub task_group: TaskGroupSummary,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateTaskGroupRequest {
    #[schemars(description = "The ID of the task group to update")]
    pub group_id: Uuid,
    #[schemars(description = "New name for the task group")]
    pub name: Option<String>,
    #[schemars(description = "New description for the task group (set to null to clear)")]
    pub description: Option<String>,
    #[schemars(description = "New base branch for the task group (set to null to clear)")]
    pub base_branch: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct UpdateTaskGroupResponse {
    pub task_group: TaskGroupSummary,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteTaskGroupRequest {
    #[schemars(description = "The ID of the task group to delete")]
    pub group_id: Uuid,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct DeleteTaskGroupResponse {
    pub deleted_group_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BulkAssignTasksToGroupRequest {
    #[schemars(description = "The ID of the task group to assign tasks to")]
    pub group_id: Uuid,
    #[schemars(description = "List of task IDs to assign to the group")]
    pub task_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct BulkAssignTasksToGroupResponse {
    pub group_id: String,
    pub updated_count: u64,
}

// ============================================================================
// Search Similar Tasks MCP Types
// ============================================================================

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchSimilarTasksRequest {
    #[schemars(description = "The ID of the project to search tasks in")]
    pub project_id: Uuid,
    #[schemars(description = "Natural language query to find similar tasks")]
    pub query: String,
    #[schemars(
        description = "Optional status filter: 'todo', 'inprogress', 'inreview', 'done', 'cancelled'"
    )]
    pub status: Option<String>,
    #[schemars(description = "Maximum number of results to return (default: 10, max: 50)")]
    pub limit: Option<i32>,
    #[schemars(
        description = "Use hybrid search combining vector similarity and keyword matching (default: true)"
    )]
    pub hybrid: Option<bool>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SimilarTaskMatch {
    #[schemars(description = "The unique identifier of the task")]
    pub id: String,
    #[schemars(description = "The title of the task")]
    pub title: String,
    #[schemars(description = "The description of the task")]
    pub description: Option<String>,
    #[schemars(description = "Current status of the task")]
    pub status: String,
    #[schemars(description = "When the task was created")]
    pub created_at: String,
    #[schemars(description = "When the task was last updated")]
    pub updated_at: String,
    #[schemars(description = "Whether the task has an in-progress execution attempt")]
    pub has_in_progress_attempt: Option<bool>,
    #[schemars(description = "Whether the last execution attempt failed")]
    pub last_attempt_failed: Option<bool>,
    #[schemars(description = "The task group this task belongs to, if any")]
    pub task_group_id: Option<String>,
    #[schemars(description = "Similarity score (0.0 to 1.0, higher is more similar)")]
    pub similarity_score: f64,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SearchSimilarTasksResponse {
    #[schemars(description = "Tasks matching the query, ranked by similarity")]
    pub matches: Vec<SimilarTaskMatch>,
    #[schemars(description = "Number of matches returned")]
    pub count: usize,
    #[schemars(description = "The project ID that was searched")]
    pub project_id: String,
    #[schemars(description = "The query that was used for searching")]
    pub query: String,
    #[schemars(description = "The search method used: 'hybrid', 'vector', or 'keyword'")]
    pub search_method: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct HealthCheckResponse {
    #[schemars(description = "Overall health status: 'healthy', 'degraded', or 'unhealthy'")]
    pub status: String,
    #[schemars(description = "Status of the API connection: 'ok' or 'failed'")]
    pub api_connection: String,
    #[schemars(description = "Latency in milliseconds for the health check request")]
    pub latency_ms: u64,
    #[schemars(description = "Timestamp of the health check")]
    pub timestamp: String,
    #[schemars(description = "Base URL of the kanban server")]
    pub server_url: String,
}

// ============================================================================
// Feedback MCP Types
// ============================================================================

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetTaskFeedbackRequest {
    #[schemars(description = "The ID of the task to get feedback for")]
    pub task_id: Uuid,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetRecentFeedbackRequest {
    #[schemars(description = "Maximum number of feedback entries to return (default: 10, max: 50)")]
    pub limit: Option<i32>,
}

/// Internal DTO for deserializing feedback from the API.
#[derive(Debug, Deserialize)]
struct ApiFeedbackEntry {
    id: Uuid,
    task_id: Uuid,
    workspace_id: Uuid,
    execution_process_id: Uuid,
    feedback: Option<serde_json::Value>,
    collected_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct FeedbackEntry {
    #[schemars(description = "The unique identifier of the feedback entry")]
    pub id: String,
    #[schemars(description = "The task ID this feedback is associated with")]
    pub task_id: String,
    #[schemars(description = "The workspace ID where the feedback was collected")]
    pub workspace_id: String,
    #[schemars(description = "The execution process ID that generated this feedback")]
    pub execution_process_id: String,
    #[schemars(description = "The feedback data as a JSON object")]
    pub feedback: Option<serde_json::Value>,
    #[schemars(description = "When the feedback was collected")]
    pub collected_at: String,
}

impl From<ApiFeedbackEntry> for FeedbackEntry {
    fn from(f: ApiFeedbackEntry) -> Self {
        FeedbackEntry {
            id: f.id.to_string(),
            task_id: f.task_id.to_string(),
            workspace_id: f.workspace_id.to_string(),
            execution_process_id: f.execution_process_id.to_string(),
            feedback: f.feedback,
            collected_at: f.collected_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct GetTaskFeedbackResponse {
    #[schemars(description = "List of feedback entries for the task")]
    pub feedback: Vec<FeedbackEntry>,
    #[schemars(description = "Number of feedback entries returned")]
    pub count: usize,
    #[schemars(description = "The task ID that was queried")]
    pub task_id: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct GetRecentFeedbackResponse {
    #[schemars(description = "List of recent feedback entries across all tasks")]
    pub feedback: Vec<FeedbackEntry>,
    #[schemars(description = "Number of feedback entries returned")]
    pub count: usize,
    #[schemars(description = "The limit that was applied")]
    pub limit: i32,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TaskGroupSummary {
    #[schemars(description = "The unique identifier of the task group")]
    pub id: String,
    #[schemars(description = "The ID of the project this group belongs to")]
    pub project_id: String,
    #[schemars(description = "The name of the task group")]
    pub name: String,
    #[schemars(description = "Optional description of the task group")]
    pub description: Option<String>,
    #[schemars(description = "The base branch for tasks in this group")]
    pub base_branch: Option<String>,
    #[schemars(description = "When the task group was created")]
    pub created_at: String,
    #[schemars(description = "When the task group was last updated")]
    pub updated_at: String,
}

impl TaskGroupSummary {
    fn from_task_group(group: TaskGroup) -> Self {
        Self {
            id: group.id.to_string(),
            project_id: group.project_id.to_string(),
            name: group.name,
            description: group.description,
            base_branch: group.base_branch,
            created_at: group.created_at.to_rfc3339(),
            updated_at: group.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TaskServer {
    client: reqwest::Client,
    base_url: String,
    tool_router: ToolRouter<TaskServer>,
    context: Option<McpContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
pub struct McpRepoContext {
    #[schemars(description = "The unique identifier of the repository")]
    pub repo_id: Uuid,
    #[schemars(description = "The name of the repository")]
    pub repo_name: String,
    #[schemars(description = "The target branch for this repository in this workspace")]
    pub target_branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
pub struct McpContext {
    pub project_id: Uuid,
    pub task_id: Uuid,
    pub task_title: String,
    pub workspace_id: Uuid,
    pub workspace_branch: String,
    #[schemars(
        description = "Repository info and target branches for each repo in this workspace"
    )]
    pub workspace_repos: Vec<McpRepoContext>,
}

impl TaskServer {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.to_string(),
            tool_router: Self::tool_router(),
            context: None,
        }
    }

    pub async fn init(mut self) -> Self {
        let context = self.fetch_context_at_startup().await;

        if context.is_none() {
            self.tool_router.map.remove("get_context");
            tracing::debug!("VK context not available, get_context tool will not be registered");
        } else {
            tracing::info!("VK context loaded, get_context tool available");
        }

        self.context = context;
        self
    }

    async fn fetch_context_at_startup(&self) -> Option<McpContext> {
        let current_dir = std::env::current_dir().ok()?;
        let canonical_path = current_dir.canonicalize().unwrap_or(current_dir);
        let normalized_path = utils::path::normalize_macos_private_alias(&canonical_path);

        let url = self.url("/api/containers/attempt-context");
        let query = ContainerQuery {
            container_ref: normalized_path.to_string_lossy().to_string(),
        };

        let response = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            self.client.get(&url).query(&query).send(),
        )
        .await
        .ok()?
        .ok()?;

        if !response.status().is_success() {
            return None;
        }

        let api_response: ApiResponseEnvelope<WorkspaceContext> = response.json().await.ok()?;

        if !api_response.success {
            return None;
        }

        let ctx = api_response.data?;

        // Map RepoWithTargetBranch to McpRepoContext
        let workspace_repos: Vec<McpRepoContext> = ctx
            .workspace_repos
            .into_iter()
            .map(|rwb| McpRepoContext {
                repo_id: rwb.repo.id,
                repo_name: rwb.repo.name,
                target_branch: rwb.target_branch,
            })
            .collect();

        Some(McpContext {
            project_id: ctx.project.id,
            task_id: ctx.task.id,
            task_title: ctx.task.title,
            workspace_id: ctx.workspace.id,
            workspace_branch: ctx.workspace.branch,
            workspace_repos,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ApiResponseEnvelope<T> {
    success: bool,
    data: Option<T>,
    message: Option<String>,
}

impl TaskServer {
    fn success<T: Serialize>(data: &T) -> Result<CallToolResult, ErrorData> {
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(data)
                .unwrap_or_else(|_| "Failed to serialize response".to_string()),
        )]))
    }

    fn err_value(v: serde_json::Value) -> Result<CallToolResult, ErrorData> {
        Ok(CallToolResult::error(vec![Content::text(
            serde_json::to_string_pretty(&v)
                .unwrap_or_else(|_| "Failed to serialize error".to_string()),
        )]))
    }

    fn err<S: Into<String>>(msg: S, details: Option<S>) -> Result<CallToolResult, ErrorData> {
        let mut v = serde_json::json!({"success": false, "error": msg.into()});
        if let Some(d) = details {
            v["details"] = serde_json::json!(d.into());
        };
        Self::err_value(v)
    }

    async fn send_json<T: DeserializeOwned>(
        &self,
        rb: reqwest::RequestBuilder,
    ) -> Result<T, CallToolResult> {
        let resp = rb
            .send()
            .await
            .map_err(|e| Self::err("Failed to connect to VK API", Some(&e.to_string())).unwrap())?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(
                Self::err(format!("VK API returned error status: {}", status), None).unwrap(),
            );
        }

        let api_response = resp.json::<ApiResponseEnvelope<T>>().await.map_err(|e| {
            Self::err("Failed to parse VK API response", Some(&e.to_string())).unwrap()
        })?;

        if !api_response.success {
            let msg = api_response.message.as_deref().unwrap_or("Unknown error");
            return Err(Self::err("VK API returned error", Some(msg)).unwrap());
        }

        api_response
            .data
            .ok_or_else(|| Self::err("VK API response missing data field", None).unwrap())
    }

    async fn send_json_no_data(
        &self,
        rb: reqwest::RequestBuilder,
        allow_not_found: bool,
    ) -> Result<bool, CallToolResult> {
        let resp = rb
            .send()
            .await
            .map_err(|e| Self::err("Failed to connect to VK API", Some(&e.to_string())).unwrap())?;

        let status = resp.status();
        if allow_not_found && status == StatusCode::NOT_FOUND {
            return Ok(false);
        }

        if !status.is_success() {
            return Err(
                Self::err(format!("VK API returned error status: {}", status), None).unwrap(),
            );
        }

        let api_response = resp
            .json::<ApiResponseEnvelope<serde_json::Value>>()
            .await
            .map_err(|e| {
                Self::err("Failed to parse VK API response", Some(&e.to_string())).unwrap()
            })?;

        if !api_response.success {
            let msg = api_response.message.as_deref().unwrap_or("Unknown error");
            return Err(Self::err("VK API returned error", Some(msg)).unwrap());
        }

        Ok(true)
    }

    fn url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    /// Expands @tagname references in text by replacing them with tag content.
    /// Returns the original text if expansion fails (e.g., network error).
    /// Unknown tags are left as-is (not expanded, not an error).
    async fn expand_tags(&self, text: &str) -> String {
        // Pattern matches @tagname where tagname is non-whitespace, non-@ characters
        let tag_pattern = match Regex::new(r"@([^\s@]+)") {
            Ok(re) => re,
            Err(_) => return text.to_string(),
        };

        // Find all unique tag names referenced in the text
        let tag_names: Vec<String> = tag_pattern
            .captures_iter(text)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        if tag_names.is_empty() {
            return text.to_string();
        }

        // Fetch all tags from the API
        let url = self.url("/api/tags");
        let tags: Vec<Tag> = match self.client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<ApiResponseEnvelope<Vec<Tag>>>().await {
                    Ok(envelope) if envelope.success => envelope.data.unwrap_or_default(),
                    _ => return text.to_string(),
                }
            }
            _ => return text.to_string(),
        };

        // Build a map of tag_name -> content for quick lookup
        let tag_map: std::collections::HashMap<&str, &str> = tags
            .iter()
            .map(|t| (t.tag_name.as_str(), t.content.as_str()))
            .collect();

        // Replace each @tagname with its content (if found)
        let result = tag_pattern.replace_all(text, |caps: &regex::Captures| {
            let tag_name = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            match tag_map.get(tag_name) {
                Some(content) => (*content).to_string(),
                None => caps.get(0).map(|m| m.as_str()).unwrap_or("").to_string(),
            }
        });

        result.into_owned()
    }

    async fn rollback_created_tasks(&self, tasks: &[Task]) {
        for task in tasks {
            let url = self.url(&format!("/api/tasks/{}", task.id));
            let _ = self.client.delete(&url).send().await;
        }
    }
}

fn detect_dependency_cycle(dependencies: &[Vec<usize>]) -> Option<Vec<usize>> {
    let mut state = vec![0_u8; dependencies.len()];
    let mut stack: Vec<usize> = Vec::new();
    let mut in_stack = vec![false; dependencies.len()];

    fn visit(
        node: usize,
        dependencies: &[Vec<usize>],
        state: &mut [u8],
        stack: &mut Vec<usize>,
        in_stack: &mut [bool],
    ) -> Option<Vec<usize>> {
        state[node] = 1;
        in_stack[node] = true;
        stack.push(node);

        for &dep in &dependencies[node] {
            if state[dep] == 0 {
                if let Some(cycle) = visit(dep, dependencies, state, stack, in_stack) {
                    return Some(cycle);
                }
            } else if in_stack[dep] {
                if let Some(pos) = stack.iter().position(|&v| v == dep) {
                    let mut cycle = stack[pos..].to_vec();
                    cycle.push(dep);
                    return Some(cycle);
                }
                return Some(vec![dep, node, dep]);
            }
        }

        stack.pop();
        in_stack[node] = false;
        state[node] = 2;
        None
    }

    for node in 0..dependencies.len() {
        if state[node] == 0
            && let Some(cycle) = visit(node, dependencies, &mut state, &mut stack, &mut in_stack)
        {
            return Some(cycle);
        }
    }

    None
}

#[tool_router]
impl TaskServer {
    #[tool(
        description = "Return project, task, and workspace metadata for the current workspace session context."
    )]
    async fn get_context(&self) -> Result<CallToolResult, ErrorData> {
        // Context was fetched at startup and cached
        // This tool is only registered if context exists, so unwrap is safe
        let context = self.context.as_ref().expect("VK context should exist");
        TaskServer::success(context)
    }

    #[tool(
        description = "Check the health status of the MCP server and its connection to the kanban backend API."
    )]
    async fn health_check(&self) -> Result<CallToolResult, ErrorData> {
        let start = std::time::Instant::now();
        let health_url = self.url("/health");

        let (status, api_connection) = match self.client.get(&health_url).send().await {
            Ok(resp) if resp.status().is_success() => ("healthy".to_string(), "ok".to_string()),
            Ok(resp) => (
                "unhealthy".to_string(),
                format!("failed: HTTP {}", resp.status()),
            ),
            Err(e) => ("unhealthy".to_string(), format!("failed: {}", e)),
        };

        let latency_ms = start.elapsed().as_millis() as u64;
        let timestamp = chrono::Utc::now().to_rfc3339();

        let response = HealthCheckResponse {
            status,
            api_connection,
            latency_ms,
            timestamp,
            server_url: self.base_url.clone(),
        };

        TaskServer::success(&response)
    }

    #[tool(
        description = "Create a new task/ticket in a project. Always pass the `project_id` of the project you want to create the task in - it is required!"
    )]
    async fn create_task(
        &self,
        Parameters(CreateTaskRequest {
            project_id,
            title,
            description,
            task_group_id,
        }): Parameters<CreateTaskRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        // Expand @tagname references in description
        let expanded_description = match description {
            Some(desc) => Some(self.expand_tags(&desc).await),
            None => None,
        };

        let url = self.url("/api/tasks");

        let task: Task = match self
            .send_json(self.client.post(&url).json(&CreateTask {
                project_id,
                title,
                description: expanded_description,
                status: Some(TaskStatus::Todo),
                parent_workspace_id: None,
                image_ids: None,
                shared_task_id: None,
                task_group_id,
            }))
            .await
        {
            Ok(t) => t,
            Err(e) => return Ok(e),
        };

        TaskServer::success(&CreateTaskResponse {
            task_id: task.id.to_string(),
        })
    }

    #[tool(
        description = "Create multiple tasks at once, optionally with dependencies between them. Use indices to reference other tasks in the same batch."
    )]
    async fn bulk_create_tasks(
        &self,
        Parameters(BulkCreateTasksRequest { project_id, tasks }): Parameters<
            BulkCreateTasksRequest,
        >,
    ) -> Result<CallToolResult, ErrorData> {
        if tasks.is_empty() {
            return Self::err(
                "At least one task must be provided.".to_string(),
                None::<String>,
            );
        }

        let task_count = tasks.len();
        let mut dependency_indices: Vec<Vec<usize>> = Vec::with_capacity(task_count);

        for (index, task) in tasks.iter().enumerate() {
            let mut indices = task.depends_on_indices.clone().unwrap_or_default();
            indices.sort_unstable();
            indices.dedup();
            for &dep_index in &indices {
                if dep_index >= task_count {
                    return Self::err(
                        format!(
                            "depends_on_indices contains out-of-range index {dep_index} for task {index}."
                        ),
                        Some(format!("valid indices: 0..{}", task_count - 1)),
                    );
                }
                if dep_index == index {
                    return Self::err(
                        format!("Task {index} cannot depend on itself."),
                        None::<String>,
                    );
                }
            }
            dependency_indices.push(indices);
        }

        if let Some(cycle) = detect_dependency_cycle(&dependency_indices) {
            let cycle_path = cycle
                .into_iter()
                .map(|idx| idx.to_string())
                .collect::<Vec<_>>()
                .join(" -> ");
            return Self::err(
                "Dependency cycle detected in batch.".to_string(),
                Some(cycle_path),
            );
        }

        let url = self.url("/api/tasks");
        let mut created_tasks: Vec<Task> = Vec::with_capacity(task_count);

        for task in tasks {
            let expanded_description = match task.description {
                Some(desc) => Some(self.expand_tags(&desc).await),
                None => None,
            };

            let created_task: Task = match self
                .send_json(self.client.post(&url).json(&CreateTask {
                    project_id,
                    title: task.title,
                    description: expanded_description,
                    status: Some(TaskStatus::Todo),
                    parent_workspace_id: None,
                    image_ids: None,
                    shared_task_id: None,
                    task_group_id: task.task_group_id,
                }))
                .await
            {
                Ok(t) => t,
                Err(e) => {
                    self.rollback_created_tasks(&created_tasks).await;
                    return Ok(e);
                }
            };
            created_tasks.push(created_task);
        }

        for (task_index, deps) in dependency_indices.iter().enumerate() {
            if deps.is_empty() {
                continue;
            }

            let task_id = created_tasks[task_index].id;
            for &dep_index in deps {
                let depends_on_id = created_tasks[dep_index].id;
                let url = self.url(&format!("/api/tasks/{}/dependencies", task_id));
                let payload = serde_json::json!({ "depends_on_id": depends_on_id });
                let _: TaskDependency =
                    match self.send_json(self.client.post(&url).json(&payload)).await {
                        Ok(dep) => dep,
                        Err(e) => {
                            self.rollback_created_tasks(&created_tasks).await;
                            return Ok(e);
                        }
                    };
            }
        }

        let response = BulkCreateTasksResponse {
            task_ids: created_tasks
                .into_iter()
                .map(|task| task.id.to_string())
                .collect(),
        };

        TaskServer::success(&response)
    }

    #[tool(
        description = "Create a task with dependencies in one call. Specify task IDs this new task depends on."
    )]
    async fn create_task_with_dependencies(
        &self,
        Parameters(CreateTaskWithDepsRequest {
            project_id,
            title,
            description,
            depends_on,
            task_group_id,
        }): Parameters<CreateTaskWithDepsRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let expanded_description = match description {
            Some(desc) => Some(self.expand_tags(&desc).await),
            None => None,
        };

        let url = self.url("/api/tasks");
        let task: Task = match self
            .send_json(self.client.post(&url).json(&CreateTask {
                project_id,
                title,
                description: expanded_description,
                status: None,
                parent_workspace_id: None,
                image_ids: None,
                shared_task_id: None,
                task_group_id,
            }))
            .await
        {
            Ok(t) => t,
            Err(e) => return Ok(e),
        };

        if let Some(depends_on) = depends_on {
            for depends_on_id in depends_on {
                let dep_url = self.url(&format!("/api/tasks/{}/dependencies", task.id));
                let payload = serde_json::json!({ "depends_on_id": depends_on_id });
                if let Err(e) = self
                    .send_json::<TaskDependency>(self.client.post(&dep_url).json(&payload))
                    .await
                {
                    return Ok(e);
                }
            }
        }

        TaskServer::success(&CreateTaskResponse {
            task_id: task.id.to_string(),
        })
    }

    #[tool(description = "List all the available projects")]
    async fn list_projects(&self) -> Result<CallToolResult, ErrorData> {
        let url = self.url("/api/projects");
        let projects: Vec<Project> = match self.send_json(self.client.get(&url)).await {
            Ok(ps) => ps,
            Err(e) => return Ok(e),
        };

        let project_summaries: Vec<ProjectSummary> = projects
            .into_iter()
            .map(ProjectSummary::from_project)
            .collect();

        let response = ListProjectsResponse {
            count: project_summaries.len(),
            projects: project_summaries,
        };

        TaskServer::success(&response)
    }

    #[tool(description = "List all repositories for a project. `project_id` is required!")]
    async fn list_repos(
        &self,
        Parameters(ListReposRequest { project_id }): Parameters<ListReposRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/projects/{}/repositories", project_id));
        let repos: Vec<Repo> = match self.send_json(self.client.get(&url)).await {
            Ok(rs) => rs,
            Err(e) => return Ok(e),
        };

        let repo_summaries: Vec<McpRepoSummary> = repos
            .into_iter()
            .map(|r| McpRepoSummary {
                id: r.id.to_string(),
                name: r.name,
            })
            .collect();

        let response = ListReposResponse {
            count: repo_summaries.len(),
            repos: repo_summaries,
            project_id: project_id.to_string(),
        };

        TaskServer::success(&response)
    }

    #[tool(
        description = "List all the task/tickets in a project with optional filtering and execution status. `project_id` is required!"
    )]
    async fn list_tasks(
        &self,
        Parameters(ListTasksRequest {
            project_id,
            query,
            status,
            limit,
        }): Parameters<ListTasksRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let status_filter = if let Some(ref status_str) = status {
            match TaskStatus::from_str(status_str) {
                Ok(s) => Some(s),
                Err(_) => {
                    return Self::err(
                        "Invalid status filter. Valid values: 'todo', 'inprogress', 'inreview', 'done', 'cancelled'".to_string(),
                        Some(status_str.to_string()),
                    );
                }
            }
        } else {
            None
        };

        let task_limit = limit.unwrap_or(50).clamp(0, 200);
        let mut url = format!(
            "/api/tasks?project_id={}&limit={}&offset=0",
            project_id, task_limit
        );
        if let Some(status) = status_filter.as_ref() {
            url.push_str(&format!("&status={}", status));
        }
        if let Some(ref search_query) = query {
            url.push_str(&format!("&query={}", search_query));
        }

        let page: PaginatedTasksResponse =
            match self.send_json(self.client.get(self.url(&url))).await {
                Ok(t) => t,
                Err(e) => return Ok(e),
            };

        let task_summaries: Vec<TaskSummary> = page
            .tasks
            .into_iter()
            .map(TaskSummary::from_task_with_status)
            .collect();

        let response = ListTasksResponse {
            count: task_summaries.len(),
            tasks: task_summaries,
            project_id: project_id.to_string(),
            applied_filters: ListTasksFilters {
                query: query.clone(),
                status: status.clone(),
                limit: task_limit,
            },
        };

        TaskServer::success(&response)
    }

    #[tool(
        description = "Start working on a task by creating and launching a new workspace session."
    )]
    async fn start_workspace_session(
        &self,
        Parameters(StartWorkspaceSessionRequest {
            task_id,
            executor,
            variant,
            repos,
        }): Parameters<StartWorkspaceSessionRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        if repos.is_empty() {
            return Self::err(
                "At least one repository must be specified.".to_string(),
                None::<String>,
            );
        }

        let executor_trimmed = executor.trim();
        if executor_trimmed.is_empty() {
            return Self::err("Executor must not be empty.".to_string(), None::<String>);
        }

        let normalized_executor = executor_trimmed.replace('-', "_").to_ascii_uppercase();
        let base_executor = match BaseCodingAgent::from_str(&normalized_executor) {
            Ok(exec) => exec,
            Err(_) => {
                return Self::err(
                    format!("Unknown executor '{executor_trimmed}'."),
                    None::<String>,
                );
            }
        };

        let variant = variant.and_then(|v| {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });

        let executor_profile_id = ExecutorProfileId {
            executor: base_executor,
            variant,
        };

        let workspace_repos: Vec<WorkspaceRepoInput> = repos
            .into_iter()
            .map(|r| WorkspaceRepoInput {
                repo_id: r.repo_id,
                target_branch: r.base_branch,
            })
            .collect();

        let payload = CreateTaskAttemptBody {
            task_id,
            executor_profile_id,
            repos: workspace_repos,
        };

        let url = self.url("/api/task-attempts");
        let workspace: Workspace = match self.send_json(self.client.post(&url).json(&payload)).await
        {
            Ok(workspace) => workspace,
            Err(e) => return Ok(e),
        };

        let response = StartWorkspaceSessionResponse {
            task_id: workspace.task_id.to_string(),
            workspace_id: workspace.id.to_string(),
        };

        TaskServer::success(&response)
    }

    #[tool(
        description = "Update an existing task/ticket's title, description, or status. `project_id` and `task_id` are required! `title`, `description`, and `status` are optional."
    )]
    async fn update_task(
        &self,
        Parameters(UpdateTaskRequest {
            task_id,
            title,
            description,
            status,
            task_group_id,
        }): Parameters<UpdateTaskRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let status = if let Some(ref status_str) = status {
            match TaskStatus::from_str(status_str) {
                Ok(s) => Some(s),
                Err(_) => {
                    return Self::err(
                        "Invalid status filter. Valid values: 'todo', 'inprogress', 'inreview', 'done', 'cancelled'".to_string(),
                        Some(status_str.to_string()),
                    );
                }
            }
        } else {
            None
        };

        // Expand @tagname references in description
        let expanded_description = match description {
            Some(desc) => Some(self.expand_tags(&desc).await),
            None => None,
        };

        let payload = UpdateTask {
            title,
            description: expanded_description,
            status,
            parent_workspace_id: None,
            image_ids: None,
            task_group_id,
        };
        let url = self.url(&format!("/api/tasks/{}", task_id));
        let updated_task: Task = match self.send_json(self.client.put(&url).json(&payload)).await {
            Ok(t) => t,
            Err(e) => return Ok(e),
        };

        let details = TaskDetails::from_task(updated_task);
        let response = UpdateTaskResponse { task: details };
        TaskServer::success(&response)
    }

    #[tool(
        description = "Delete a task/ticket from a project. `project_id` and `task_id` are required!"
    )]
    async fn delete_task(
        &self,
        Parameters(DeleteTaskRequest { task_id }): Parameters<DeleteTaskRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/tasks/{}", task_id));
        let deleted = match self.send_json_no_data(self.client.delete(&url), true).await {
            Ok(deleted) => deleted,
            Err(e) => return Ok(e),
        };

        let response = DeleteTaskResponse {
            deleted_task_id: if deleted {
                Some(task_id.to_string())
            } else {
                None
            },
        };

        TaskServer::success(&response)
    }

    #[tool(
        description = "Get detailed information (like task description) about a specific task/ticket. You can use `list_tasks` to find the `task_ids` of all tasks in a project. `project_id` and `task_id` are required!"
    )]
    async fn get_task(
        &self,
        Parameters(GetTaskRequest { task_id }): Parameters<GetTaskRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/tasks/{}", task_id));
        let task: Task = match self.send_json(self.client.get(&url)).await {
            Ok(t) => t,
            Err(e) => return Ok(e),
        };

        let details = TaskDetails::from_task(task);
        let response = GetTaskResponse { task: details };

        TaskServer::success(&response)
    }

    #[tool(description = "Add a dependency between two tasks.")]
    async fn add_task_dependency(
        &self,
        Parameters(AddTaskDependencyRequest {
            task_id,
            depends_on_id,
        }): Parameters<AddTaskDependencyRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/tasks/{}/dependencies", task_id));
        let payload = serde_json::json!({ "depends_on_id": depends_on_id });
        let dependency: TaskDependency =
            match self.send_json(self.client.post(&url).json(&payload)).await {
                Ok(dep) => dep,
                Err(e) => return Ok(e),
            };

        TaskServer::success(&TaskDependencyInfo::from_dependency(dependency))
    }

    #[tool(description = "Remove a dependency between two tasks.")]
    async fn remove_task_dependency(
        &self,
        Parameters(RemoveTaskDependencyRequest {
            task_id,
            depends_on_id,
        }): Parameters<RemoveTaskDependencyRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!(
            "/api/tasks/{}/dependencies/{}",
            task_id, depends_on_id
        ));
        if let Err(e) = self
            .send_json_no_data(self.client.delete(&url), false)
            .await
        {
            return Ok(e);
        }

        let response = RemoveTaskDependencyResponse {
            task_id: task_id.to_string(),
            depends_on_id: depends_on_id.to_string(),
            removed: true,
        };

        TaskServer::success(&response)
    }

    #[tool(
        description = "Get dependency information for a task, including tasks it depends on and tasks it blocks."
    )]
    async fn get_task_dependencies(
        &self,
        Parameters(GetTaskDependenciesRequest { task_id }): Parameters<GetTaskDependenciesRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let blocked_by_url = self.url(&format!("/api/tasks/{}/dependencies", task_id));
        let blocked_by: Vec<Task> = match self.send_json(self.client.get(&blocked_by_url)).await {
            Ok(tasks) => tasks,
            Err(e) => return Ok(e),
        };

        let blocking_url = self.url(&format!(
            "/api/tasks/{}/dependencies?direction=blocking",
            task_id
        ));
        let blocking: Vec<Task> = match self.send_json(self.client.get(&blocking_url)).await {
            Ok(tasks) => tasks,
            Err(e) => return Ok(e),
        };

        let blocked_by_details: Vec<TaskDetails> =
            blocked_by.into_iter().map(TaskDetails::from_task).collect();
        let blocking_details: Vec<TaskDetails> =
            blocking.into_iter().map(TaskDetails::from_task).collect();

        // A task is blocked if any of its dependencies are not done
        let is_blocked = blocked_by_details.iter().any(|dep| dep.status != "done");

        let response = TaskDependencySummary {
            is_blocked,
            blocked_by: blocked_by_details,
            blocking: blocking_details,
        };

        TaskServer::success(&response)
    }

    #[tool(description = "Get a dependency tree for a task.")]
    async fn get_task_dependency_tree(
        &self,
        Parameters(GetTaskDependencyTreeRequest { task_id, max_depth }): Parameters<
            GetTaskDependencyTreeRequest,
        >,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(max_depth) = max_depth
            && max_depth < 0
        {
            return Self::err(
                "max_depth must be non-negative.".to_string(),
                None::<String>,
            );
        }

        let url = self.url(&format!("/api/tasks/{}/dependency-tree", task_id));
        let request = if let Some(max_depth) = max_depth {
            self.client.get(&url).query(&[("max_depth", max_depth)])
        } else {
            self.client.get(&url)
        };

        let tree: TaskDependencyTreeNodeApi = match self.send_json(request).await {
            Ok(tree) => tree,
            Err(e) => return Ok(e),
        };

        let response = GetTaskDependencyTreeResponse {
            tree: TaskDependencyTreeNode::from_api(tree),
        };

        TaskServer::success(&response)
    }

    #[tool(
        description = "Get bidirectional dependency context for a task, including both tasks it depends on (ancestors) and tasks that depend on it (descendants). Use this to understand a task's position in the dependency graph."
    )]
    async fn get_task_dependency_context(
        &self,
        Parameters(GetTaskDependencyContextRequest { task_id }): Parameters<
            GetTaskDependencyContextRequest,
        >,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/tasks/{}/dependency-context", task_id));

        let context: TaskDependencyContextApi = match self.send_json(self.client.get(&url)).await {
            Ok(ctx) => ctx,
            Err(e) => return Ok(e),
        };

        let response = GetTaskDependencyContextResponse {
            task_id: task_id.to_string(),
            ancestors: context
                .ancestors
                .into_iter()
                .map(TaskDetails::from_task)
                .collect(),
            descendants: context
                .descendants
                .into_iter()
                .map(TaskDetails::from_task)
                .collect(),
        };

        TaskServer::success(&response)
    }

    // ========================================================================
    // Task Group Tools
    // ========================================================================

    #[tool(description = "List all task groups in a project. `project_id` is required!")]
    async fn list_task_groups(
        &self,
        Parameters(ListTaskGroupsRequest { project_id }): Parameters<ListTaskGroupsRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/task-groups?project_id={}", project_id));
        let groups: Vec<TaskGroup> = match self.send_json(self.client.get(&url)).await {
            Ok(gs) => gs,
            Err(e) => return Ok(e),
        };

        let group_summaries: Vec<TaskGroupSummary> = groups
            .into_iter()
            .map(|g| {
                let mut summary = TaskGroupSummary::from_task_group(g);
                summary.description = None;
                summary
            })
            .collect();

        let response = ListTaskGroupsResponse {
            count: group_summaries.len(),
            task_groups: group_summaries,
            project_id: project_id.to_string(),
        };

        TaskServer::success(&response)
    }

    #[tool(
        description = "Create a new task group in a project. `project_id` and `name` are required!"
    )]
    async fn create_task_group(
        &self,
        Parameters(CreateTaskGroupRequest {
            project_id,
            name,
            description,
            base_branch,
        }): Parameters<CreateTaskGroupRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url("/api/task-groups");
        let payload = serde_json::json!({
            "project_id": project_id,
            "name": name,
            "description": description,
            "base_branch": base_branch
        });

        let group: TaskGroup = match self.send_json(self.client.post(&url).json(&payload)).await {
            Ok(g) => g,
            Err(e) => return Ok(e),
        };

        let response = CreateTaskGroupResponse {
            task_group: TaskGroupSummary::from_task_group(group),
        };

        TaskServer::success(&response)
    }

    #[tool(description = "Get details of a specific task group. `group_id` is required!")]
    async fn get_task_group(
        &self,
        Parameters(GetTaskGroupRequest { group_id }): Parameters<GetTaskGroupRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/task-groups/{}", group_id));
        let group: TaskGroup = match self.send_json(self.client.get(&url)).await {
            Ok(g) => g,
            Err(e) => return Ok(e),
        };

        let response = GetTaskGroupResponse {
            task_group: TaskGroupSummary::from_task_group(group),
        };

        TaskServer::success(&response)
    }

    #[tool(description = "Update a task group's name, description, or base branch. `group_id` is required!")]
    async fn update_task_group(
        &self,
        Parameters(UpdateTaskGroupRequest {
            group_id,
            name,
            description,
            base_branch,
        }): Parameters<UpdateTaskGroupRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/task-groups/{}", group_id));
        let payload = serde_json::json!({
            "name": name,
            "description": description,
            "base_branch": base_branch
        });

        let group: TaskGroup = match self.send_json(self.client.put(&url).json(&payload)).await {
            Ok(g) => g,
            Err(e) => return Ok(e),
        };

        let response = UpdateTaskGroupResponse {
            task_group: TaskGroupSummary::from_task_group(group),
        };

        TaskServer::success(&response)
    }

    #[tool(description = "Delete a task group. `group_id` is required!")]
    async fn delete_task_group(
        &self,
        Parameters(DeleteTaskGroupRequest { group_id }): Parameters<DeleteTaskGroupRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/task-groups/{}", group_id));
        let deleted = match self.send_json_no_data(self.client.delete(&url), true).await {
            Ok(deleted) => deleted,
            Err(e) => return Ok(e),
        };

        let response = DeleteTaskGroupResponse {
            deleted_group_id: if deleted {
                Some(group_id.to_string())
            } else {
                None
            },
        };

        TaskServer::success(&response)
    }

    #[tool(
        description = "Assign multiple tasks to a task group. `group_id` and `task_ids` are required!"
    )]
    async fn bulk_assign_tasks_to_group(
        &self,
        Parameters(BulkAssignTasksToGroupRequest { group_id, task_ids }): Parameters<
            BulkAssignTasksToGroupRequest,
        >,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/task-groups/{}/assign", group_id));
        let payload = serde_json::json!({ "task_ids": task_ids });

        #[derive(Debug, Deserialize)]
        struct BulkAssignResponse {
            updated_count: u64,
        }

        let result: BulkAssignResponse =
            match self.send_json(self.client.post(&url).json(&payload)).await {
                Ok(r) => r,
                Err(e) => return Ok(e),
            };

        let response = BulkAssignTasksToGroupResponse {
            group_id: group_id.to_string(),
            updated_count: result.updated_count,
        };

        TaskServer::success(&response)
    }

    // ========================================================================
    // Semantic Search Tools
    // ========================================================================

    #[tool(
        description = "Search for tasks using semantic similarity. Finds tasks related in meaning to your query, not just keyword matches. Use this to discover related work, find duplicates, or explore tasks by concept. Examples: 'authentication bugs', 'database performance', 'user onboarding flow'."
    )]
    async fn search_similar_tasks(
        &self,
        Parameters(SearchSimilarTasksRequest {
            project_id,
            query,
            status,
            limit,
            hybrid,
        }): Parameters<SearchSimilarTasksRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        // Validate query is not empty
        let query_trimmed = query.trim();
        if query_trimmed.is_empty() {
            return Self::err("Query cannot be empty".to_string(), None::<String>);
        }

        // Validate status if provided
        let status_filter = if let Some(ref status_str) = status {
            match TaskStatus::from_str(status_str) {
                Ok(s) => Some(s),
                Err(_) => {
                    return Self::err(
                        "Invalid status filter. Valid values: 'todo', 'inprogress', 'inreview', 'done', 'cancelled'".to_string(),
                        Some(status_str.to_string()),
                    );
                }
            }
        } else {
            None
        };

        // Build the request payload
        let payload = serde_json::json!({
            "projectId": project_id,
            "query": query_trimmed,
            "status": status_filter,
            "limit": limit.unwrap_or(10).clamp(1, 50),
            "hybrid": hybrid.unwrap_or(true),
        });

        let url = self.url("/api/tasks/search");

        // Response structure from the API
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ApiSearchResponse {
            matches: Vec<ApiTaskMatch>,
            #[allow(dead_code)]
            count: usize,
            search_method: String,
        }

        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ApiTaskMatch {
            id: Uuid,
            title: String,
            description: Option<String>,
            status: String,
            created_at: chrono::DateTime<chrono::Utc>,
            updated_at: chrono::DateTime<chrono::Utc>,
            has_in_progress_attempt: bool,
            last_attempt_failed: bool,
            task_group_id: Option<Uuid>,
            similarity_score: f64,
        }

        let api_response: ApiSearchResponse =
            match self.send_json(self.client.post(&url).json(&payload)).await {
                Ok(r) => r,
                Err(e) => return Ok(e),
            };

        // Convert to MCP response format
        let matches: Vec<SimilarTaskMatch> = api_response
            .matches
            .into_iter()
            .map(|m| SimilarTaskMatch {
                id: m.id.to_string(),
                title: m.title,
                description: m.description,
                status: m.status,
                created_at: m.created_at.to_rfc3339(),
                updated_at: m.updated_at.to_rfc3339(),
                has_in_progress_attempt: Some(m.has_in_progress_attempt),
                last_attempt_failed: Some(m.last_attempt_failed),
                task_group_id: m.task_group_id.map(|id| id.to_string()),
                similarity_score: m.similarity_score,
            })
            .collect();

        let response = SearchSimilarTasksResponse {
            count: matches.len(),
            matches,
            project_id: project_id.to_string(),
            query: query_trimmed.to_string(),
            search_method: api_response.search_method,
        };

        TaskServer::success(&response)
    }

    // ========================================================================
    // Feedback Tools
    // ========================================================================

    #[tool(
        description = "Get all feedback entries for a specific task. Feedback contains insights from agent executions that can help improve future task handling. Use this to learn from past executions on the same task."
    )]
    async fn get_task_feedback(
        &self,
        Parameters(GetTaskFeedbackRequest { task_id }): Parameters<GetTaskFeedbackRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/feedback/task/{}", task_id));

        let feedback_list: Vec<ApiFeedbackEntry> =
            match self.send_json(self.client.get(&url)).await {
                Ok(f) => f,
                Err(e) => return Ok(e),
            };

        let entries: Vec<FeedbackEntry> = feedback_list.into_iter().map(Into::into).collect();

        let response = GetTaskFeedbackResponse {
            count: entries.len(),
            feedback: entries,
            task_id: task_id.to_string(),
        };

        TaskServer::success(&response)
    }

    #[tool(
        description = "Get recent feedback entries across all tasks. Use this to explore collected insights and learn from past agent executions. Helpful for discovering patterns and improving task handling strategies."
    )]
    async fn get_recent_feedback(
        &self,
        Parameters(GetRecentFeedbackRequest { limit }): Parameters<GetRecentFeedbackRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let limit = limit.unwrap_or(10).clamp(1, 50);
        let url = self.url(&format!("/api/feedback/recent?limit={}", limit));

        let feedback_list: Vec<ApiFeedbackEntry> =
            match self.send_json(self.client.get(&url)).await {
                Ok(f) => f,
                Err(e) => return Ok(e),
            };

        let entries: Vec<FeedbackEntry> = feedback_list.into_iter().map(Into::into).collect();

        let response = GetRecentFeedbackResponse {
            count: entries.len(),
            feedback: entries,
            limit,
        };

        TaskServer::success(&response)
    }
}

#[tool_handler]
impl ServerHandler for TaskServer {
    fn get_info(&self) -> ServerInfo {
        let mut instruction = "A task and project management server. If you need to create or update tickets or tasks then use these tools. Most of them absolutely require that you pass the `project_id` of the project that you are currently working on. You can get project ids by using `list projects`. Call `list_tasks` to fetch the `task_ids` of all the tasks in a project`. TOOLS: 'health_check', 'list_projects', 'list_tasks', 'search_similar_tasks', 'create_task', 'bulk_create_tasks', 'create_task_with_dependencies', 'start_workspace_session', 'get_task', 'update_task', 'delete_task', 'list_repos', 'add_task_dependency', 'remove_task_dependency', 'get_task_dependencies', 'get_task_dependency_tree', 'get_task_dependency_context', 'list_task_groups', 'create_task_group', 'get_task_group', 'update_task_group', 'delete_task_group', 'bulk_assign_tasks_to_group', 'get_task_feedback', 'get_recent_feedback'. Make sure to pass `project_id`, `task_id`, or `group_id` where required. You can use list tools to get the available ids.".to_string();
        if self.context.is_some() {
            let context_instruction = "When working on a task, VK_TASK_ID env var is set. Use 'get_context' to fetch your current task details including task_id. Use 'get_task_dependency_context' with your task_id to see what tasks must complete before yours (ancestors) and what tasks are waiting on you (descendants). This helps understand your position in the workflow.";
            instruction = format!("{} {}", context_instruction, instruction);
        }

        ServerInfo {
            protocol_version: ProtocolVersion::V_2025_03_26,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "vibe-kanban".to_string(),
                version: "1.0.0".to_string(),
            },
            instructions: Some(instruction),
        }
    }
}
