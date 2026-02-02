//! Test fixtures for autopilot E2E tests.
//!
//! Provides:
//! - `TestDb`: Isolated SQLite database with migrations applied
//! - `autopilot_config()`: Config with autopilot_enabled=true
//! - `autopilot_disabled_config()`: Config with autopilot_enabled=false
//! - Entity creation helpers for projects, tasks, workspaces, sessions, and executions

use std::sync::Arc;

use db::models::{
    execution_process::{
        ExecutionProcess, ExecutionProcessRunReason, ExecutionProcessStatus, ExecutorActionField,
    },
    task::{Task, TaskStatus},
    workspace::Workspace,
};
use executors::actions::{
    ExecutorAction, ExecutorActionType,
    script::{ScriptContext, ScriptRequest, ScriptRequestLanguage},
};
use services::services::config::Config;
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use tempfile::NamedTempFile;
use tokio::sync::RwLock;
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

/// Creates a test session in the database.
pub async fn create_session(pool: &SqlitePool, workspace_id: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO sessions (id, workspace_id, executor) VALUES (?, ?, ?)")
        .bind(id)
        .bind(workspace_id)
        .bind("claude_code")
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
