use executors::logs::NormalizedEntry;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
struct ExecutionProcessNormalizedEntryRow {
    entry_index: i64,
    entry_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionProcessNormalizedEntry {
    pub entry_index: i64,
    pub entry: NormalizedEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionProcessNormalizedEntriesPage {
    pub entries: Vec<ExecutionProcessNormalizedEntry>,
    pub next_before_index: Option<i64>,
    pub has_more: bool,
}

impl ExecutionProcessNormalizedEntry {
    pub async fn upsert(
        pool: &SqlitePool,
        execution_id: Uuid,
        entry_index: i64,
        entry: &NormalizedEntry,
    ) -> Result<(), anyhow::Error> {
        let entry_json = serde_json::to_string(entry)?;
        sqlx::query!(
            r#"INSERT INTO execution_process_normalized_entries (
                    execution_id,
                    entry_index,
                    entry_json
                ) VALUES ($1, $2, $3)
                ON CONFLICT(execution_id, entry_index)
                DO UPDATE SET entry_json = excluded.entry_json"#,
            execution_id,
            entry_index,
            entry_json
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn delete(
        pool: &SqlitePool,
        execution_id: Uuid,
        entry_index: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"DELETE FROM execution_process_normalized_entries
               WHERE execution_id = $1 AND entry_index = $2"#,
            execution_id,
            entry_index
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn count_by_execution_id(
        pool: &SqlitePool,
        execution_id: Uuid,
    ) -> Result<i64, sqlx::Error> {
        let row = sqlx::query!(
            r#"SELECT COUNT(1) as "count!: i64"
               FROM execution_process_normalized_entries
               WHERE execution_id = $1"#,
            execution_id
        )
        .fetch_one(pool)
        .await?;

        Ok(row.count)
    }

    pub async fn fetch_page(
        pool: &SqlitePool,
        execution_id: Uuid,
        before_index: Option<i64>,
        limit: usize,
    ) -> Result<ExecutionProcessNormalizedEntriesPage, anyhow::Error> {
        let limit = limit.clamp(1, 500) as i64;
        let fetch_limit = limit + 1;

        let rows: Vec<ExecutionProcessNormalizedEntryRow> = if let Some(before_index) = before_index
        {
            sqlx::query_as!(
                ExecutionProcessNormalizedEntryRow,
                r#"SELECT
                        entry_index,
                        entry_json
                   FROM execution_process_normalized_entries
                   WHERE execution_id = $1
                     AND entry_index < $2
                   ORDER BY entry_index DESC
                   LIMIT $3"#,
                execution_id,
                before_index,
                fetch_limit
            )
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query_as!(
                ExecutionProcessNormalizedEntryRow,
                r#"SELECT
                        entry_index,
                        entry_json
                   FROM execution_process_normalized_entries
                   WHERE execution_id = $1
                   ORDER BY entry_index DESC
                   LIMIT $2"#,
                execution_id,
                fetch_limit
            )
            .fetch_all(pool)
            .await?
        };

        let has_more = rows.len() as i64 > limit;
        let rows = if has_more {
            rows[..limit as usize].to_vec()
        } else {
            rows
        };

        let mut entries = Vec::with_capacity(rows.len());
        for row in rows {
            let entry = serde_json::from_str(&row.entry_json)?;
            entries.push(ExecutionProcessNormalizedEntry {
                entry_index: row.entry_index,
                entry,
            });
        }

        entries.reverse();
        let next_before_index = if has_more {
            entries.first().map(|entry| entry.entry_index)
        } else {
            None
        };

        Ok(ExecutionProcessNormalizedEntriesPage {
            entries,
            next_before_index,
            has_more,
        })
    }

    pub async fn fetch_all_for_execution(
        pool: &SqlitePool,
        execution_id: Uuid,
    ) -> Result<Vec<ExecutionProcessNormalizedEntry>, anyhow::Error> {
        let rows: Vec<ExecutionProcessNormalizedEntryRow> = sqlx::query_as!(
            ExecutionProcessNormalizedEntryRow,
            r#"SELECT
                    entry_index,
                    entry_json
               FROM execution_process_normalized_entries
               WHERE execution_id = $1
               ORDER BY entry_index ASC"#,
            execution_id
        )
        .fetch_all(pool)
        .await?;

        let mut entries = Vec::with_capacity(rows.len());
        for row in rows {
            let entry = serde_json::from_str(&row.entry_json)?;
            entries.push(ExecutionProcessNormalizedEntry {
                entry_index: row.entry_index,
                entry,
            });
        }

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use executors::{
        actions::{
            ExecutorAction, ExecutorActionType,
            script::{ScriptContext, ScriptRequest, ScriptRequestLanguage},
        },
        logs::{NormalizedEntry, NormalizedEntryType},
    };
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;
    use crate::{
        init_sqlite_vec,
        models::{
            execution_process::{
                CreateExecutionProcess, ExecutionProcess, ExecutionProcessRunReason,
            },
            project::{CreateProject, Project},
            session::{CreateSession, Session},
            task::{CreateTask, Task, TaskStatus},
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

    async fn create_project(pool: &SqlitePool, name: &str) -> Project {
        Project::create(
            pool,
            &CreateProject {
                name: name.to_string(),
                repositories: vec![],
            },
            Uuid::new_v4(),
        )
        .await
        .unwrap()
    }

    async fn create_task(pool: &SqlitePool, project_id: Uuid, title: &str) -> Task {
        Task::create(
            pool,
            &CreateTask {
                project_id,
                title: title.to_string(),
                description: None,
                status: Some(TaskStatus::Todo),
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

    async fn create_workspace(pool: &SqlitePool, task_id: Uuid) -> Workspace {
        Workspace::create(
            pool,
            &CreateWorkspace {
                branch: "feature/normalized-entry-tests".to_string(),
                agent_working_dir: None,
            },
            Uuid::new_v4(),
            task_id,
        )
        .await
        .unwrap()
    }

    async fn create_session(pool: &SqlitePool, workspace_id: Uuid) -> Session {
        Session::create(
            pool,
            &CreateSession {
                executor: Some("CLAUDE_CODE".to_string()),
            },
            Uuid::new_v4(),
            workspace_id,
        )
        .await
        .unwrap()
    }

    async fn create_execution(pool: &SqlitePool, session_id: Uuid) -> ExecutionProcess {
        ExecutionProcess::create(
            pool,
            &CreateExecutionProcess {
                session_id,
                executor_action: ExecutorAction::new(
                    ExecutorActionType::ScriptRequest(ScriptRequest {
                        script: "echo normalized-entry-test".to_string(),
                        language: ScriptRequestLanguage::Bash,
                        context: ScriptContext::SetupScript,
                        working_dir: None,
                    }),
                    None,
                ),
                run_reason: ExecutionProcessRunReason::SetupScript,
            },
            Uuid::new_v4(),
            &[],
        )
        .await
        .unwrap()
    }

    fn assistant_entry(content: &str) -> NormalizedEntry {
        NormalizedEntry {
            timestamp: None,
            entry_type: NormalizedEntryType::AssistantMessage,
            content: content.to_string(),
            metadata: None,
        }
    }

    #[tokio::test]
    async fn fetch_page_returns_stable_oldest_to_newest_order_per_page() {
        let pool = setup_test_pool().await;
        let project = create_project(&pool, "normalized-entry-model").await;
        let task = create_task(&pool, project.id, "Page normalized entries").await;
        let workspace = create_workspace(&pool, task.id).await;
        let session = create_session(&pool, workspace.id).await;
        let execution = create_execution(&pool, session.id).await;

        for index in 0..5 {
            ExecutionProcessNormalizedEntry::upsert(
                &pool,
                execution.id,
                index,
                &assistant_entry(&format!("entry-{index}")),
            )
            .await
            .unwrap();
        }

        let first_page = ExecutionProcessNormalizedEntry::fetch_page(&pool, execution.id, None, 2)
            .await
            .unwrap();
        assert!(first_page.has_more);
        assert_eq!(first_page.next_before_index, Some(3));
        assert_eq!(
            first_page
                .entries
                .iter()
                .map(|entry| entry.entry_index)
                .collect::<Vec<_>>(),
            vec![3, 4]
        );

        let second_page = ExecutionProcessNormalizedEntry::fetch_page(
            &pool,
            execution.id,
            first_page.next_before_index,
            2,
        )
        .await
        .unwrap();
        assert!(second_page.has_more);
        assert_eq!(second_page.next_before_index, Some(1));
        assert_eq!(
            second_page
                .entries
                .iter()
                .map(|entry| entry.entry_index)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );

        let third_page = ExecutionProcessNormalizedEntry::fetch_page(
            &pool,
            execution.id,
            second_page.next_before_index,
            2,
        )
        .await
        .unwrap();
        assert!(!third_page.has_more);
        assert_eq!(third_page.next_before_index, None);
        assert_eq!(
            third_page
                .entries
                .iter()
                .map(|entry| entry.entry_index)
                .collect::<Vec<_>>(),
            vec![0]
        );
    }
}
