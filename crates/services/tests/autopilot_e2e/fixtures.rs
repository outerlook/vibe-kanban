//! Test fixtures for autopilot E2E tests.
//!
//! Provides:
//! - `TestDb`: Isolated SQLite database with migrations applied
//! - `autopilot_config()`: Config with autopilot_enabled=true
//! - `autopilot_disabled_config()`: Config with autopilot_enabled=false
//! - Entity creation helpers for projects, tasks, workspaces, sessions, and executions
//! - `EntityGraphBuilder`: Fluent API for creating complex entity hierarchies
//! - Common test helpers used across multiple test files

use std::sync::Arc;

use db::models::{
    execution_process::{
        ExecutionProcess, ExecutionProcessRunReason, ExecutionProcessStatus, ExecutorActionField,
    },
    task::{Task, TaskStatus},
    workspace::Workspace,
};
use executors::{
    actions::{
        ExecutorAction, ExecutorActionType,
        script::{ScriptContext, ScriptRequest, ScriptRequestLanguage},
    },
    executors::BaseCodingAgent,
    profile::ExecutorProfileId,
};
use services::services::{
    config::Config,
    domain_events::{AutopilotHandler, DispatcherBuilder, DomainEvent, HandlerContext},
};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use tempfile::NamedTempFile;
use tokio::sync::RwLock;
use utils::msg_store::MsgStore;
use uuid::Uuid;

/// An isolated test database with all migrations applied.
///
/// The database is backed by a temporary file that is automatically
/// deleted when the TestDb is dropped.
pub struct TestDb {
    pool: SqlitePool,
    #[allow(dead_code)]
    db_file: NamedTempFile,
}

impl TestDb {
    /// Creates a new isolated test database with all migrations applied.
    pub async fn new() -> Self {
        let db_file = NamedTempFile::new().expect("Failed to create temp file");
        let db_path = db_file.path().to_str().expect("Invalid temp file path");

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite:{}?mode=rwc", db_path))
            .await
            .expect("Failed to create database");

        sqlx::migrate!("../db/migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        Self { pool, db_file }
    }

    /// Returns a reference to the database pool.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

/// Creates a Config with autopilot_enabled=true.
///
/// Use this for tests that verify autopilot behavior.
pub fn autopilot_config() -> Arc<RwLock<Config>> {
    let mut config = Config::default();
    config.autopilot_enabled = true;
    Arc::new(RwLock::new(config))
}

/// Creates a Config with autopilot_enabled=false.
///
/// Use this for negative test cases to verify autopilot doesn't run when disabled.
pub fn autopilot_disabled_config() -> Arc<RwLock<Config>> {
    Arc::new(RwLock::new(Config::default()))
}

/// Creates a test project in the database.
pub async fn create_project(pool: &SqlitePool, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO projects (id, name) VALUES (?, ?)")
        .bind(id)
        .bind(name)
        .execute(pool)
        .await
        .expect("Failed to create project");
    id
}

/// Creates a test task in the database.
pub async fn create_task(
    pool: &SqlitePool,
    project_id: Uuid,
    title: &str,
    status: TaskStatus,
) -> Task {
    let id = Uuid::new_v4();
    let status_str = match status {
        TaskStatus::Todo => "todo",
        TaskStatus::InProgress => "inprogress",
        TaskStatus::InReview => "inreview",
        TaskStatus::Done => "done",
        TaskStatus::Cancelled => "cancelled",
    };
    let now = chrono::Utc::now();

    sqlx::query(
        "INSERT INTO tasks (id, project_id, title, status, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(project_id)
    .bind(title)
    .bind(status_str)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .expect("Failed to create task");

    Task {
        id,
        project_id,
        title: title.to_string(),
        description: None,
        status,
        parent_workspace_id: None,
        shared_task_id: None,
        task_group_id: None,
        created_at: now,
        updated_at: now,
        is_blocked: false,
        has_in_progress_attempt: false,
        last_attempt_failed: false,
        is_queued: false,
        last_executor: String::new(),
        needs_attention: None,
    }
}

/// Creates a test workspace in the database.
pub async fn create_workspace(pool: &SqlitePool, task_id: Uuid, branch: &str) -> Workspace {
    let id = Uuid::new_v4();
    let now = chrono::Utc::now();

    sqlx::query(
        "INSERT INTO workspaces (id, task_id, branch, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(task_id)
    .bind(branch)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .expect("Failed to create workspace");

    Workspace {
        id,
        task_id,
        container_ref: None,
        branch: branch.to_string(),
        agent_working_dir: None,
        setup_completed_at: None,
        created_at: now,
        updated_at: now,
    }
}

/// Creates a test session in the database with proper ExecutorProfileId JSON.
pub async fn create_session(pool: &SqlitePool, workspace_id: Uuid) -> Uuid {
    create_session_with_executor(pool, workspace_id, BaseCodingAgent::ClaudeCode).await
}

/// Creates a test session with a specific executor.
pub async fn create_session_with_executor(
    pool: &SqlitePool,
    workspace_id: Uuid,
    executor: BaseCodingAgent,
) -> Uuid {
    let id = Uuid::new_v4();
    let executor_profile = ExecutorProfileId::new(executor);
    let executor_json =
        serde_json::to_string(&executor_profile).expect("Failed to serialize executor profile");

    sqlx::query("INSERT INTO sessions (id, workspace_id, executor) VALUES (?, ?, ?)")
        .bind(id)
        .bind(workspace_id)
        .bind(&executor_json)
        .execute(pool)
        .await
        .expect("Failed to create session");
    id
}

/// Creates a test execution process in the database.
pub async fn create_execution(
    pool: &SqlitePool,
    session_id: Uuid,
    status: ExecutionProcessStatus,
    run_reason: ExecutionProcessRunReason,
) -> ExecutionProcess {
    let id = Uuid::new_v4();
    let status_str = match &status {
        ExecutionProcessStatus::Running => "running",
        ExecutionProcessStatus::Completed => "completed",
        ExecutionProcessStatus::Failed => "failed",
        ExecutionProcessStatus::Killed => "killed",
    };
    let run_reason_str = match &run_reason {
        ExecutionProcessRunReason::CodingAgent => "codingagent",
        ExecutionProcessRunReason::SetupScript => "setupscript",
        ExecutionProcessRunReason::CleanupScript => "cleanupscript",
        ExecutionProcessRunReason::InternalAgent => "internalagent",
        ExecutionProcessRunReason::DevServer => "devserver",
        ExecutionProcessRunReason::DisposableConversation => "disposableconversation",
    };
    let now = chrono::Utc::now();
    let script_request = ScriptRequest {
        script: "echo test".to_string(),
        language: ScriptRequestLanguage::Bash,
        context: ScriptContext::SetupScript,
        working_dir: None,
    };
    let executor_action =
        ExecutorAction::new(ExecutorActionType::ScriptRequest(script_request), None);
    let executor_action_json =
        serde_json::to_string(&executor_action).expect("serialize executor action");

    sqlx::query(
        "INSERT INTO execution_processes (id, session_id, status, run_reason, executor_action, started_at, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(session_id)
    .bind(status_str)
    .bind(run_reason_str)
    .bind(&executor_action_json)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .expect("Failed to create execution process");

    ExecutionProcess {
        id,
        session_id: Some(session_id),
        conversation_session_id: None,
        status,
        run_reason,
        executor_action: sqlx::types::Json(ExecutorActionField::ExecutorAction(executor_action)),
        exit_code: None,
        dropped: false,
        input_tokens: None,
        output_tokens: None,
        started_at: now,
        completed_at: None,
        created_at: now,
        updated_at: now,
    }
}

/// Creates a task dependency in the database.
pub async fn create_task_dependency(pool: &SqlitePool, task_id: Uuid, depends_on_id: Uuid) {
    let id = Uuid::new_v4();
    let now = chrono::Utc::now();
    sqlx::query(
        "INSERT INTO task_dependencies (id, task_id, depends_on_id, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(id)
    .bind(task_id)
    .bind(depends_on_id)
    .bind(now)
    .execute(pool)
    .await
    .expect("Failed to create task dependency");
}

/// Fluent builder for creating test entity graphs.
///
/// Simplifies creating complex entity hierarchies (project → task → workspace → session → execution)
/// with a chainable API.
///
/// # Example
/// ```ignore
/// let ctx = EntityGraphBuilder::new(pool.clone())
///     .with_project("Test Project")
///     .create_task("Task 1", TaskStatus::Todo).await
///     .with_workspace("feature-branch").await
///     .with_session().await
///     .with_completed_coding_execution().await;
///
/// println!("Task ID: {}", ctx.task_id());
/// println!("Execution ID: {}", ctx.execution_id());
/// ```
pub struct EntityGraphBuilder {
    pool: SqlitePool,
    project_id: Option<Uuid>,
    project_name: Option<String>,
}

impl EntityGraphBuilder {
    /// Creates a new EntityGraphBuilder with the given database pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            project_id: None,
            project_name: None,
        }
    }

    /// Sets the project name for entities created by this builder.
    /// The project is created lazily when the first task is created.
    pub fn with_project(mut self, name: &str) -> Self {
        self.project_name = Some(name.to_string());
        self
    }

    /// Creates a task with the given title and status.
    /// If no project exists yet, creates one using the configured name or "Test Project".
    pub async fn create_task(mut self, title: &str, status: TaskStatus) -> TaskContext {
        let project_id = if let Some(id) = self.project_id {
            id
        } else {
            let name = self.project_name.as_deref().unwrap_or("Test Project");
            let id = create_project(&self.pool, name).await;
            self.project_id = Some(id);
            id
        };

        let task = create_task(&self.pool, project_id, title, status).await;

        TaskContext {
            pool: self.pool.clone(),
            builder_project_id: self.project_id,
            builder_project_name: self.project_name.clone(),
            task,
        }
    }

    /// Returns the project ID if one has been created.
    pub fn project_id(&self) -> Option<Uuid> {
        self.project_id
    }
}

/// Context for a created task, allowing further chaining.
pub struct TaskContext {
    pool: SqlitePool,
    builder_project_id: Option<Uuid>,
    builder_project_name: Option<String>,
    task: Task,
}

impl TaskContext {
    /// Returns the task ID.
    pub fn task_id(&self) -> Uuid {
        self.task.id
    }

    /// Returns the project ID.
    pub fn project_id(&self) -> Uuid {
        self.task.project_id
    }

    /// Returns a reference to the task.
    pub fn task(&self) -> &Task {
        &self.task
    }

    /// Adds a dependency - this task depends on the given task.
    pub async fn with_dependency(self, depends_on_id: Uuid) -> Self {
        create_task_dependency(&self.pool, self.task.id, depends_on_id).await;
        self
    }

    /// Creates a workspace for this task with the given branch name.
    pub async fn with_workspace(self, branch: &str) -> WorkspaceContext {
        let workspace = create_workspace(&self.pool, self.task.id, branch).await;

        WorkspaceContext {
            pool: self.pool,
            builder_project_id: self.builder_project_id,
            builder_project_name: self.builder_project_name,
            task: self.task,
            workspace,
        }
    }

    /// Creates another task under the same project.
    pub async fn and_task(self, title: &str, status: TaskStatus) -> TaskContext {
        let task = create_task(&self.pool, self.task.project_id, title, status).await;

        TaskContext {
            pool: self.pool,
            builder_project_id: self.builder_project_id,
            builder_project_name: self.builder_project_name,
            task,
        }
    }

    /// Returns a builder that can create more tasks under the same project.
    pub fn builder(self) -> EntityGraphBuilder {
        EntityGraphBuilder {
            pool: self.pool,
            project_id: self.builder_project_id,
            project_name: self.builder_project_name,
        }
    }
}

/// Context for a created workspace, allowing further chaining.
pub struct WorkspaceContext {
    pool: SqlitePool,
    builder_project_id: Option<Uuid>,
    builder_project_name: Option<String>,
    task: Task,
    workspace: Workspace,
}

impl WorkspaceContext {
    /// Returns the workspace ID.
    pub fn workspace_id(&self) -> Uuid {
        self.workspace.id
    }

    /// Returns the task ID.
    pub fn task_id(&self) -> Uuid {
        self.task.id
    }

    /// Returns the project ID.
    pub fn project_id(&self) -> Uuid {
        self.task.project_id
    }

    /// Returns a reference to the workspace.
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// Returns a reference to the task.
    pub fn task(&self) -> &Task {
        &self.task
    }

    /// Creates a session for this workspace.
    pub async fn with_session(self) -> SessionContext {
        let session_id = create_session(&self.pool, self.workspace.id).await;

        SessionContext {
            pool: self.pool,
            builder_project_id: self.builder_project_id,
            builder_project_name: self.builder_project_name,
            task: self.task,
            workspace: self.workspace,
            session_id,
            execution: None,
        }
    }

    /// Returns a builder that can create more tasks under the same project.
    pub fn builder(self) -> EntityGraphBuilder {
        EntityGraphBuilder {
            pool: self.pool,
            project_id: self.builder_project_id,
            project_name: self.builder_project_name,
        }
    }
}

/// Context for a created session, allowing further chaining.
pub struct SessionContext {
    pool: SqlitePool,
    builder_project_id: Option<Uuid>,
    builder_project_name: Option<String>,
    task: Task,
    workspace: Workspace,
    session_id: Uuid,
    execution: Option<ExecutionProcess>,
}

impl SessionContext {
    /// Returns the session ID.
    pub fn session_id(&self) -> Uuid {
        self.session_id
    }

    /// Returns the workspace ID.
    pub fn workspace_id(&self) -> Uuid {
        self.workspace.id
    }

    /// Returns the task ID.
    pub fn task_id(&self) -> Uuid {
        self.task.id
    }

    /// Returns the project ID.
    pub fn project_id(&self) -> Uuid {
        self.task.project_id
    }

    /// Returns the execution ID if an execution has been created.
    pub fn execution_id(&self) -> Option<Uuid> {
        self.execution.as_ref().map(|e| e.id)
    }

    /// Returns a reference to the execution if one has been created.
    pub fn execution(&self) -> Option<&ExecutionProcess> {
        self.execution.as_ref()
    }

    /// Returns a reference to the task.
    pub fn task(&self) -> &Task {
        &self.task
    }

    /// Returns a reference to the workspace.
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// Creates a completed coding agent execution for this session.
    pub async fn with_completed_coding_execution(mut self) -> Self {
        let execution = create_execution(
            &self.pool,
            self.session_id,
            ExecutionProcessStatus::Completed,
            ExecutionProcessRunReason::CodingAgent,
        )
        .await;
        self.execution = Some(execution);
        self
    }

    /// Creates an execution with custom status and run reason.
    pub async fn with_execution(
        mut self,
        status: ExecutionProcessStatus,
        run_reason: ExecutionProcessRunReason,
    ) -> Self {
        let execution = create_execution(&self.pool, self.session_id, status, run_reason).await;
        self.execution = Some(execution);
        self
    }

    /// Returns a builder that can create more tasks under the same project.
    pub fn builder(self) -> EntityGraphBuilder {
        EntityGraphBuilder {
            pool: self.pool,
            project_id: self.builder_project_id,
            project_name: self.builder_project_name,
        }
    }
}

// ============================================================================
// Common Test Helpers
// ============================================================================

/// Creates a HandlerContext for testing with the given config.
pub fn test_handler_context(pool: SqlitePool, config: Arc<RwLock<Config>>) -> HandlerContext {
    let db = db::DBService { pool };
    let msg_store = Arc::new(MsgStore::default());
    HandlerContext::new(db, config, msg_store, None)
}

/// Updates task status directly in the database.
pub async fn update_task_status(pool: &SqlitePool, task_id: Uuid, status: TaskStatus) {
    let status_str = task_status_to_str(status);
    sqlx::query("UPDATE tasks SET status = ? WHERE id = ?")
        .bind(status_str)
        .bind(task_id)
        .execute(pool)
        .await
        .expect("Failed to update task status");
}

/// Updates task needs_attention flag directly in the database.
pub async fn update_task_needs_attention(pool: &SqlitePool, task_id: Uuid, needs_attention: bool) {
    sqlx::query("UPDATE tasks SET needs_attention = ? WHERE id = ?")
        .bind(needs_attention)
        .bind(task_id)
        .execute(pool)
        .await
        .expect("Failed to update task needs_attention");
}

/// Gets is_blocked value for a task.
pub async fn get_task_is_blocked(pool: &SqlitePool, task_id: Uuid) -> bool {
    sqlx::query_scalar("SELECT is_blocked FROM tasks WHERE id = ?")
        .bind(task_id)
        .fetch_one(pool)
        .await
        .expect("Failed to fetch is_blocked")
}

/// Converts TaskStatus to its string representation.
pub fn task_status_to_str(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Todo => "todo",
        TaskStatus::InProgress => "inprogress",
        TaskStatus::InReview => "inreview",
        TaskStatus::Done => "done",
        TaskStatus::Cancelled => "cancelled",
    }
}

/// Creates a Task struct for dispatching events.
pub fn make_task_for_event(
    task_id: Uuid,
    project_id: Uuid,
    title: &str,
    status: TaskStatus,
) -> Task {
    Task {
        id: task_id,
        project_id,
        title: title.to_string(),
        description: None,
        status,
        parent_workspace_id: None,
        shared_task_id: None,
        task_group_id: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        is_blocked: false,
        has_in_progress_attempt: false,
        last_attempt_failed: false,
        is_queued: false,
        last_executor: String::new(),
        needs_attention: None,
    }
}

/// Dispatches TaskStatusChanged event and waits for handler completion.
pub async fn dispatch_task_done(
    pool: &SqlitePool,
    task_id: Uuid,
    project_id: Uuid,
    title: &str,
    previous_status: TaskStatus,
) {
    let config = autopilot_config();
    let ctx = test_handler_context(pool.clone(), config);

    let dispatcher = DispatcherBuilder::new()
        .with_handler(AutopilotHandler::new())
        .with_context(ctx)
        .build();

    let task = make_task_for_event(task_id, project_id, title, TaskStatus::Done);

    dispatcher
        .dispatch(DomainEvent::TaskStatusChanged {
            task,
            previous_status,
        })
        .await;

    // Give spawned handler time to complete (AutopilotHandler is Spawned mode)
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
}

// ============================================================================
// Git Repository Fixtures
// ============================================================================

pub mod git_fixtures {
    //! Git repository fixtures for tests that need real git operations.

    use std::path::{Path, PathBuf};

    use db::models::workspace_repo::{CreateWorkspaceRepo, WorkspaceRepo};
    use git2::Repository;
    use services::services::{
        config::Config,
        git::GitService,
        merge_queue_processor::MergeQueueProcessor,
        merge_queue_store::MergeQueueStore,
    };
    use sqlx::SqlitePool;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::RwLock;
    use utils::msg_store::MsgStore;
    use uuid::Uuid;

    use super::{autopilot_config, TestDb};

    /// A test git repository with worktree support.
    ///
    /// Creates an isolated git repository in a temporary directory with an initial commit.
    /// The repository is automatically cleaned up when dropped.
    pub struct TestRepo {
        pub path: PathBuf,
        pub name: String,
        _dir: TempDir,
    }

    impl TestRepo {
        /// Creates a new test repository with the given name.
        pub fn new(name: &str) -> Self {
            let dir = TempDir::new().expect("Failed to create temp dir");
            let path = dir.path().join(name);

            let repo = Repository::init(&path).expect("Failed to init git repo");

            // Configure git user
            {
                let mut config = repo.config().expect("Failed to get repo config");
                config
                    .set_str("user.name", "Test User")
                    .expect("Failed to set user.name");
                config
                    .set_str("user.email", "test@example.com")
                    .expect("Failed to set user.email");
            }

            // Create initial commit
            let sig = repo.signature().expect("Failed to get signature");
            let readme_path = path.join("README.md");
            std::fs::write(&readme_path, "# Test Repository\n").expect("Failed to write README.md");

            let mut index = repo.index().expect("Failed to get index");
            index
                .add_path(Path::new("README.md"))
                .expect("Failed to add README.md to index");
            index.write().expect("Failed to write index");

            let tree_id = index.write_tree().expect("Failed to write tree");
            let tree = repo.find_tree(tree_id).expect("Failed to find tree");

            repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
                .expect("Failed to create initial commit");

            repo.set_head("refs/heads/master")
                .expect("Failed to set HEAD");

            let mut master = repo
                .find_branch("master", git2::BranchType::Local)
                .expect("Failed to find master branch");
            master.rename("main", false).expect("Failed to rename to main");

            repo.set_head("refs/heads/main")
                .expect("Failed to set HEAD to main");

            Self {
                path,
                name: name.to_string(),
                _dir: dir,
            }
        }

        /// Creates a new branch at HEAD.
        pub fn create_branch(&self, branch: &str) {
            let repo = Repository::open(&self.path).expect("Failed to open repository");
            let head = repo
                .head()
                .expect("Failed to get HEAD")
                .peel_to_commit()
                .expect("Failed to peel to commit");
            repo.branch(branch, &head, false)
                .expect("Failed to create branch");
        }

        /// Creates a git worktree for the given branch.
        pub fn create_worktree(&self, branch: &str) -> PathBuf {
            let repo = Repository::open(&self.path).expect("Failed to open repository");

            if repo.find_branch(branch, git2::BranchType::Local).is_err() {
                self.create_branch(branch);
            }

            let branch_ref = repo
                .find_branch(branch, git2::BranchType::Local)
                .expect("Failed to find branch")
                .into_reference();

            let worktree_path = self
                .path
                .parent()
                .expect("Repo should have parent")
                .join(format!("{}-worktree-{}", self.name, branch));

            repo.worktree(
                branch,
                &worktree_path,
                Some(&git2::WorktreeAddOptions::new().reference(Some(&branch_ref))),
            )
            .expect("Failed to create worktree");

            worktree_path
        }
    }

    /// Test context containing all the pieces needed for merge-related tests.
    pub struct MergeTestContext {
        pub pool: SqlitePool,
        pub git: GitService,
        pub merge_queue_store: MergeQueueStore,
        pub config: Arc<RwLock<Config>>,
        _test_db: TestDb,
    }

    impl MergeTestContext {
        pub async fn new() -> Self {
            Self::with_config(autopilot_config()).await
        }

        pub async fn with_config(config: Arc<RwLock<Config>>) -> Self {
            let test_db = TestDb::new().await;
            let pool = test_db.pool().clone();
            let git = GitService::new();
            let msg_store = Arc::new(MsgStore::new());
            let merge_queue_store = MergeQueueStore::new(msg_store);

            Self {
                pool,
                git,
                merge_queue_store,
                config,
                _test_db: test_db,
            }
        }

        pub fn processor(&self) -> MergeQueueProcessor {
            MergeQueueProcessor::new(
                self.pool.clone(),
                self.git.clone(),
                self.merge_queue_store.clone(),
                self.config.clone(),
            )
        }
    }

    /// Creates a repo in the database pointing to the test repository.
    pub async fn create_repo(pool: &SqlitePool, path: &Path, name: &str) -> Uuid {
        let id = Uuid::new_v4();
        let path_str = path.to_string_lossy().to_string();
        let now = chrono::Utc::now();

        sqlx::query(
            "INSERT INTO repos (id, path, name, display_name, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(&path_str)
        .bind(name)
        .bind(name)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .expect("Failed to create repo");

        id
    }

    /// Updates a workspace's container_ref to point to a worktree directory.
    pub async fn update_workspace_container_ref(
        pool: &SqlitePool,
        workspace_id: Uuid,
        container_ref: &str,
    ) {
        sqlx::query("UPDATE workspaces SET container_ref = ? WHERE id = ?")
            .bind(container_ref)
            .bind(workspace_id)
            .execute(pool)
            .await
            .expect("Failed to update workspace container_ref");
    }

    /// Creates a WorkspaceRepo linking workspace to repo with target branch.
    pub async fn create_workspace_repo(
        pool: &SqlitePool,
        workspace_id: Uuid,
        repo_id: Uuid,
        target_branch: &str,
    ) {
        WorkspaceRepo::create_many(
            pool,
            workspace_id,
            &[CreateWorkspaceRepo {
                repo_id,
                target_branch: target_branch.to_string(),
            }],
        )
        .await
        .expect("Failed to create workspace repo");
    }

    /// Adds a file and commits it in a worktree.
    pub fn add_and_commit(worktree_path: &Path, filename: &str, content: &str, message: &str) {
        let repo = Repository::open(worktree_path).expect("Failed to open repo");
        let file_path = worktree_path.join(filename);
        std::fs::write(&file_path, content).expect("Failed to write file");

        let mut index = repo.index().expect("Failed to get index");
        index
            .add_path(Path::new(filename))
            .expect("Failed to add file");
        index.write().expect("Failed to write index");

        let tree_id = index.write_tree().expect("Failed to write tree");
        let tree = repo.find_tree(tree_id).expect("Failed to find tree");
        let sig = repo.signature().expect("Failed to get signature");
        let parent = repo.head().unwrap().peel_to_commit().unwrap();

        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])
            .expect("Failed to commit");
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_repo_creates_valid_git_repo() {
            let repo = TestRepo::new("test-repo");
            assert!(repo.path.exists());

            let git_repo = Repository::open(&repo.path).expect("Should open as git repo");
            let head = git_repo.head().expect("Should have HEAD");
            assert!(head.is_branch());
        }

        #[test]
        fn test_repo_creates_worktree() {
            let repo = TestRepo::new("worktree-test");
            let worktree_path = repo.create_worktree("feature-branch");

            assert!(worktree_path.exists());
            let worktree_repo =
                Repository::open(&worktree_path).expect("Should open worktree as repo");
            let head = worktree_repo.head().expect("Should have HEAD");
            assert!(head.name().unwrap().contains("feature-branch"));
        }
    }
}
