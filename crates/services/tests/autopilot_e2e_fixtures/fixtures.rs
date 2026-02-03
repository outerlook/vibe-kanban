//! Test fixtures for autopilot E2E tests.
//!
//! Provides `MockExecutionController` for capturing execution triggers
//! and controlling execution outcomes without running real LLM agents.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use chrono::Utc;
use db::models::execution_process::{ExecutionProcessRunReason, ExecutionProcessStatus};
use executors::actions::{
    ExecutorAction, ExecutorActionType,
    script::{ScriptContext, ScriptRequest, ScriptRequestLanguage},
};
use futures::FutureExt;
use services::services::domain_events::{ExecutionTrigger, ExecutionTriggerCallback};
use sqlx::SqlitePool;
use uuid::Uuid;

/// Response configuration for mocked execution completions.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MockExecutionResponse {
    /// Whether the execution needs user attention (for ReviewAttention triggers).
    pub needs_attention: Option<bool>,
    /// The final status of the execution process.
    pub status: ExecutionProcessStatus,
}

impl Default for MockExecutionResponse {
    fn default() -> Self {
        Self {
            needs_attention: None,
            status: ExecutionProcessStatus::Completed,
        }
    }
}

/// Controller for capturing and controlling execution triggers in tests.
///
/// This allows tests to:
/// 1. Capture triggers dispatched by handlers (FeedbackCollection, ReviewAttention)
/// 2. Create real ExecutionProcess records in the database
/// 3. Configure mock responses for triggered executions
/// 4. Mark executions as completed with configurable outcomes
pub struct MockExecutionController {
    pool: SqlitePool,
    /// Captured execution triggers.
    captures: Arc<Mutex<Vec<ExecutionTrigger>>>,
    /// Pre-configured responses by task ID.
    responses: Arc<Mutex<HashMap<Uuid, MockExecutionResponse>>>,
}

impl MockExecutionController {
    /// Creates a new MockExecutionController with the given database pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            captures: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Returns an `ExecutionTriggerCallback` that:
    /// 1. Captures triggers to the internal captures vec
    /// 2. Creates ExecutionProcess records in the database
    /// 3. Returns the spawned execution ID
    pub fn callback(&self) -> ExecutionTriggerCallback {
        let pool = self.pool.clone();
        let captures = Arc::clone(&self.captures);

        Arc::new(move |trigger: ExecutionTrigger| {
            let pool = pool.clone();
            let captures = Arc::clone(&captures);

            async move {
                // Capture the trigger
                {
                    let mut caps = captures.lock().unwrap();
                    caps.push(trigger.clone());
                }

                // Extract session_id and run_reason from trigger
                let (session_id, run_reason) = match &trigger {
                    ExecutionTrigger::FeedbackCollection {
                        workspace_id,
                        task_id: _,
                        execution_process_id: _,
                    } => {
                        // Find session for workspace
                        let session_id =
                            find_or_create_session_for_workspace(&pool, *workspace_id).await?;
                        (session_id, ExecutionProcessRunReason::InternalAgent)
                    }
                    ExecutionTrigger::ReviewAttention {
                        task_id,
                        execution_process_id: _,
                    } => {
                        // Find session for task
                        let session_id = find_or_create_session_for_task(&pool, *task_id).await?;
                        (session_id, ExecutionProcessRunReason::InternalAgent)
                    }
                };

                // Create execution process record
                let exec_id = create_execution_process(&pool, session_id, run_reason).await?;

                Ok(exec_id)
            }
            .boxed()
        })
    }

    /// Sets the mock response for review attention triggers for a specific task.
    #[allow(dead_code)]
    pub fn set_review_response(&self, task_id: Uuid, needs_attention: bool) {
        let mut responses = self.responses.lock().unwrap();
        responses.insert(
            task_id,
            MockExecutionResponse {
                needs_attention: Some(needs_attention),
                status: ExecutionProcessStatus::Completed,
            },
        );
    }

    /// Returns all captured triggers.
    pub fn get_captures(&self) -> Vec<ExecutionTrigger> {
        let caps = self.captures.lock().unwrap();
        caps.clone()
    }

    /// Marks an execution as completed in the database.
    #[allow(dead_code)]
    pub async fn complete_execution(&self, exec_id: Uuid) -> Result<(), anyhow::Error> {
        let now = Utc::now();
        sqlx::query(
            r#"UPDATE execution_processes
               SET status = 'completed', completed_at = ?, updated_at = ?
               WHERE id = ?"#,
        )
        .bind(now)
        .bind(now)
        .bind(exec_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Clears all captured triggers.
    #[allow(dead_code)]
    pub fn clear_captures(&self) {
        let mut caps = self.captures.lock().unwrap();
        caps.clear();
    }

    /// Gets the configured response for a task, if any.
    #[allow(dead_code)]
    pub fn get_response(&self, task_id: Uuid) -> Option<MockExecutionResponse> {
        let responses = self.responses.lock().unwrap();
        responses.get(&task_id).cloned()
    }
}

/// Finds an existing session for a workspace, or creates one if none exists.
async fn find_or_create_session_for_workspace(
    pool: &SqlitePool,
    workspace_id: Uuid,
) -> Result<Uuid, anyhow::Error> {
    // Try to find existing session
    let existing: Option<(Uuid,)> = sqlx::query_as(
        r#"SELECT id FROM sessions WHERE workspace_id = ? LIMIT 1"#,
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;

    if let Some((session_id,)) = existing {
        return Ok(session_id);
    }

    // Create new session
    let session_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sessions (id, workspace_id, executor) VALUES (?, ?, ?)",
    )
    .bind(session_id)
    .bind(workspace_id)
    .bind("mock_executor")
    .execute(pool)
    .await?;

    Ok(session_id)
}

/// Finds an existing session for a task (via workspace), or creates one if none exists.
async fn find_or_create_session_for_task(
    pool: &SqlitePool,
    task_id: Uuid,
) -> Result<Uuid, anyhow::Error> {
    // Try to find existing session via workspace
    let existing: Option<(Uuid,)> = sqlx::query_as(
        r#"SELECT s.id
           FROM sessions s
           JOIN workspaces w ON s.workspace_id = w.id
           WHERE w.task_id = ?
           LIMIT 1"#,
    )
    .bind(task_id)
    .fetch_optional(pool)
    .await?;

    if let Some((session_id,)) = existing {
        return Ok(session_id);
    }

    // Need to find or create a workspace first
    let workspace_row: Option<(Uuid,)> = sqlx::query_as(
        r#"SELECT id FROM workspaces WHERE task_id = ? LIMIT 1"#,
    )
    .bind(task_id)
    .fetch_optional(pool)
    .await?;

    let workspace_id = workspace_row
        .map(|(id,)| id)
        .ok_or_else(|| anyhow::anyhow!("No workspace found for task {}", task_id))?;

    find_or_create_session_for_workspace(pool, workspace_id).await
}

/// Creates an execution process record in the database.
async fn create_execution_process(
    pool: &SqlitePool,
    session_id: Uuid,
    run_reason: ExecutionProcessRunReason,
) -> Result<Uuid, anyhow::Error> {
    let exec_id = Uuid::new_v4();
    let now = Utc::now();

    let run_reason_str = match &run_reason {
        ExecutionProcessRunReason::CodingAgent => "codingagent",
        ExecutionProcessRunReason::SetupScript => "setupscript",
        ExecutionProcessRunReason::CleanupScript => "cleanupscript",
        ExecutionProcessRunReason::InternalAgent => "internalagent",
        ExecutionProcessRunReason::DevServer => "devserver",
        ExecutionProcessRunReason::DisposableConversation => "disposableconversation",
    };

    // Create a minimal executor action for the mock process
    let script_request = ScriptRequest {
        script: "echo mock".to_string(),
        language: ScriptRequestLanguage::Bash,
        context: ScriptContext::SetupScript,
        working_dir: None,
    };
    let executor_action =
        ExecutorAction::new(ExecutorActionType::ScriptRequest(script_request), None);
    let executor_action_json = serde_json::to_string(&executor_action)?;

    sqlx::query(
        r#"INSERT INTO execution_processes
           (id, session_id, status, run_reason, executor_action, started_at, created_at, updated_at)
           VALUES (?, ?, 'running', ?, ?, ?, ?, ?)"#,
    )
    .bind(exec_id)
    .bind(session_id)
    .bind(run_reason_str)
    .bind(executor_action_json)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(exec_id)
}
