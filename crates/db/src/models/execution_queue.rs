use chrono::{DateTime, Utc};
use executors::{actions::ExecutorAction, profile::ExecutorProfileId};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

/// Represents an entry in the execution queue.
/// Presence in this table means the workspace is waiting to execute.
/// When execution starts, the row is deleted.
///
/// For initial workspace starts: session_id and executor_action are None.
/// For follow-up executions: session_id and executor_action are populated.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExecutionQueue {
    pub id: Uuid,
    pub workspace_id: Uuid,
    #[ts(type = "ExecutorProfileId")]
    pub executor_profile_id: sqlx::types::Json<ExecutorProfileId>,
    pub queued_at: DateTime<Utc>,
    /// Session ID for follow-up executions (None for initial workspace starts)
    pub session_id: Option<Uuid>,
    /// Serialized ExecutorAction for follow-up executions (None for initial workspace starts)
    pub executor_action: Option<String>,
}

impl ExecutionQueue {
    /// Check if this is a follow-up queue entry
    pub fn is_follow_up(&self) -> bool {
        self.session_id.is_some() && self.executor_action.is_some()
    }

    /// Parse the executor action (for follow-up entries)
    pub fn parsed_executor_action(&self) -> Option<ExecutorAction> {
        self.executor_action
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
    }

    /// Insert a new queue entry for initial workspace start (waiting to run)
    pub async fn create(
        pool: &SqlitePool,
        workspace_id: Uuid,
        executor_profile_id: &ExecutorProfileId,
    ) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();
        let executor_profile_json = serde_json::to_string(executor_profile_id)
            .map_err(|e| sqlx::Error::Encode(Box::new(e)))?;

        sqlx::query!(
            r#"INSERT INTO execution_queue (id, workspace_id, executor_profile_id)
               VALUES (?, ?, ?)"#,
            id,
            workspace_id,
            executor_profile_json
        )
        .execute(pool)
        .await?;

        Self::find_by_id(pool, id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    /// Insert a new queue entry for follow-up execution
    pub async fn create_follow_up(
        pool: &SqlitePool,
        workspace_id: Uuid,
        session_id: Uuid,
        executor_action: &ExecutorAction,
    ) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();

        let executor_profile_id = match executor_action.executor_profile_id() {
            Some(executor_profile_id) => executor_profile_id.clone(),
            None => {
                // Scripts don't have a profile, use a default
                ExecutorProfileId {
                    executor: executors::executors::BaseCodingAgent::ClaudeCode,
                    variant: None,
                }
            }
        };

        let executor_profile_json = serde_json::to_string(&executor_profile_id)
            .map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
        let executor_action_json =
            serde_json::to_string(executor_action).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;

        sqlx::query!(
            r#"INSERT INTO execution_queue (id, workspace_id, executor_profile_id, session_id, executor_action)
               VALUES (?, ?, ?, ?, ?)"#,
            id,
            workspace_id,
            executor_profile_json,
            session_id,
            executor_action_json
        )
        .execute(pool)
        .await?;

        Self::find_by_id(pool, id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    /// Find a queue entry by ID
    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            ExecutionQueue,
            r#"SELECT
                id AS "id!: Uuid",
                workspace_id AS "workspace_id!: Uuid",
                executor_profile_id AS "executor_profile_id!: sqlx::types::Json<ExecutorProfileId>",
                queued_at AS "queued_at!: DateTime<Utc>",
                session_id AS "session_id: Uuid",
                executor_action AS "executor_action: String"
            FROM execution_queue
            WHERE id = ?"#,
            id
        )
        .fetch_optional(pool)
        .await
    }

    /// Pop the oldest waiting entry (SELECT + DELETE) - FIFO by queued_at
    /// Returns None if queue is empty
    pub async fn pop_next(pool: &SqlitePool) -> Result<Option<Self>, sqlx::Error> {
        // Get the oldest entry
        let entry = sqlx::query_as!(
            ExecutionQueue,
            r#"SELECT
                id AS "id!: Uuid",
                workspace_id AS "workspace_id!: Uuid",
                executor_profile_id AS "executor_profile_id!: sqlx::types::Json<ExecutorProfileId>",
                queued_at AS "queued_at!: DateTime<Utc>",
                session_id AS "session_id: Uuid",
                executor_action AS "executor_action: String"
            FROM execution_queue
            ORDER BY queued_at ASC
            LIMIT 1"#
        )
        .fetch_optional(pool)
        .await?;

        // If found, delete it
        if let Some(ref e) = entry {
            sqlx::query!("DELETE FROM execution_queue WHERE id = ?", e.id)
                .execute(pool)
                .await?;
        }

        Ok(entry)
    }

    /// Check if a workspace has a pending queue entry
    pub async fn find_by_workspace(
        pool: &SqlitePool,
        workspace_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            ExecutionQueue,
            r#"SELECT
                id AS "id!: Uuid",
                workspace_id AS "workspace_id!: Uuid",
                executor_profile_id AS "executor_profile_id!: sqlx::types::Json<ExecutorProfileId>",
                queued_at AS "queued_at!: DateTime<Utc>",
                session_id AS "session_id: Uuid",
                executor_action AS "executor_action: String"
            FROM execution_queue
            WHERE workspace_id = ?"#,
            workspace_id
        )
        .fetch_optional(pool)
        .await
    }

    /// Cancel/remove a workspace's entry from the queue
    pub async fn delete_by_workspace(
        pool: &SqlitePool,
        workspace_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "DELETE FROM execution_queue WHERE workspace_id = ?",
            workspace_id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Get total count of entries in the queue
    pub async fn count(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
        let result =
            sqlx::query_scalar!(r#"SELECT COUNT(*) AS "count!: i64" FROM execution_queue"#)
                .fetch_one(pool)
                .await?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use executors::{
        actions::{
            ExecutorActionType, StructuredOutputContract,
            coding_agent_follow_up::CodingAgentFollowUpRequest,
        },
        executors::BaseCodingAgent,
        profile::ExecutorProfileId,
    };
    use serde_json::json;
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;
    use crate::{
        init_sqlite_vec,
        models::{
            project::{CreateProject, Project},
            session::{CreateSession, Session},
            task::Task,
            workspace::{CreateWorkspace, Workspace},
        },
    };

    async fn setup_test_pool() -> SqlitePool {
        init_sqlite_vec();

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();

        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        pool
    }

    fn structured_output_contract() -> StructuredOutputContract {
        StructuredOutputContract {
            schema: serde_json::from_value(json!({
                "type": "object",
                "required": ["decision"],
                "properties": {
                    "decision": { "type": "string" }
                },
                "additionalProperties": false
            }))
            .unwrap(),
        }
    }

    async fn create_workspace_context(pool: &SqlitePool) -> (Project, Task, Workspace, Session) {
        let project = Project::create(
            pool,
            &CreateProject {
                name: "execution-queue-structured-output".to_string(),
                repositories: vec![],
            },
            Uuid::new_v4(),
        )
        .await
        .unwrap();
        let task_id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO tasks (
                    id, project_id, title, description, status, parent_workspace_id,
                    shared_task_id, task_group_id, last_executor
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(task_id)
        .bind(project.id)
        .bind("Task")
        .bind(None::<String>)
        .bind("todo")
        .bind(None::<Uuid>)
        .bind(None::<Uuid>)
        .bind(None::<Uuid>)
        .bind("")
        .execute(pool)
        .await
        .unwrap();
        let task = Task::find_by_id(pool, task_id).await.unwrap().unwrap();
        let workspace = Workspace::create(
            pool,
            &CreateWorkspace {
                branch: "feature/queue-structured-output".to_string(),
                agent_working_dir: Some("repo".to_string()),
            },
            Uuid::new_v4(),
            task.id,
        )
        .await
        .unwrap();
        let session = Session::create(
            pool,
            &CreateSession {
                executor: Some("claude_code".to_string()),
            },
            Uuid::new_v4(),
            workspace.id,
        )
        .await
        .unwrap();

        (project, task, workspace, session)
    }

    #[tokio::test]
    async fn follow_up_queue_round_trip_preserves_structured_output_contract() {
        let pool = setup_test_pool().await;
        let (_, _, workspace, session) = create_workspace_context(&pool).await;
        let executor_action = ExecutorAction::new(
            ExecutorActionType::CodingAgentFollowUpRequest(CodingAgentFollowUpRequest {
                prompt: "Continue with structured output".to_string(),
                session_id: "agent-session-456".to_string(),
                structured_output: Some(structured_output_contract()),
                executor_profile_id: ExecutorProfileId::new(BaseCodingAgent::ClaudeCode),
                working_dir: Some("repo".to_string()),
            }),
            None,
        );

        let queued =
            ExecutionQueue::create_follow_up(&pool, workspace.id, session.id, &executor_action)
                .await
                .unwrap();

        assert_eq!(
            queued.parsed_executor_action().as_ref(),
            Some(&executor_action)
        );
    }
}
