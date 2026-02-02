//! Entry point for autopilot E2E tests.
//!
//! This file exists to enable test discovery while organizing the actual test code
//! in the autopilot_e2e directory.

#[path = "autopilot_e2e/mod.rs"]
mod autopilot_e2e;

pub use autopilot_e2e::fixtures;

// Smoke tests to verify the test infrastructure works
#[cfg(test)]
mod smoke_tests {
    use super::fixtures::{
        autopilot_config, autopilot_disabled_config, create_execution, create_project,
        create_session, create_task, create_workspace, TestDb,
    };
    use db::models::{
        execution_process::{ExecutionProcessRunReason, ExecutionProcessStatus},
        task::TaskStatus,
    };

    /// Verifies that TestDb can be created and migrations run successfully.
    #[tokio::test]
    async fn test_db_creation_and_migrations() {
        let test_db = TestDb::new().await;

        // Verify we can query the database (migrations ran)
        let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects")
            .fetch_one(test_db.pool())
            .await
            .expect("Should be able to query projects table");

        assert_eq!(result.0, 0, "Fresh database should have no projects");
    }

    /// Verifies that autopilot_config returns config with autopilot_enabled=true.
    #[tokio::test]
    async fn test_autopilot_config_has_autopilot_enabled() {
        let config = autopilot_config();
        let config_read = config.read().await;
        assert!(
            config_read.autopilot_enabled,
            "autopilot_config() should return config with autopilot_enabled=true"
        );
    }

    /// Verifies that autopilot_disabled_config returns config with autopilot_enabled=false.
    #[tokio::test]
    async fn test_autopilot_disabled_config() {
        let config = autopilot_disabled_config();
        let config_read = config.read().await;
        assert!(
            !config_read.autopilot_enabled,
            "autopilot_disabled_config() should return config with autopilot_enabled=false"
        );
    }

    /// Verifies entity creation helpers work correctly.
    #[tokio::test]
    async fn test_entity_creation_helpers() {
        let test_db = TestDb::new().await;
        let pool = test_db.pool();

        // Create project
        let project_id = create_project(pool, "Test Project").await;

        // Verify project exists
        let project_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM projects WHERE id = ?")
                .bind(project_id)
                .fetch_one(pool)
                .await
                .expect("Should query project");
        assert_eq!(project_count.0, 1, "Project should be created");

        // Create task
        let task = create_task(pool, project_id, "Test Task", TaskStatus::InProgress).await;
        assert_eq!(task.project_id, project_id);
        assert_eq!(task.title, "Test Task");
        assert_eq!(task.status, TaskStatus::InProgress);

        // Create workspace
        let workspace = create_workspace(pool, task.id, "feature-branch").await;
        assert_eq!(workspace.task_id, task.id);
        assert_eq!(workspace.branch, "feature-branch");

        // Create session
        let session_id = create_session(pool, workspace.id).await;

        // Verify session exists
        let session_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sessions WHERE id = ?")
                .bind(session_id)
                .fetch_one(pool)
                .await
                .expect("Should query session");
        assert_eq!(session_count.0, 1, "Session should be created");

        // Create execution
        let execution = create_execution(
            pool,
            session_id,
            ExecutionProcessStatus::Completed,
            ExecutionProcessRunReason::CodingAgent,
        )
        .await;
        assert_eq!(execution.session_id, Some(session_id));
        assert_eq!(execution.status, ExecutionProcessStatus::Completed);
        assert_eq!(execution.run_reason, ExecutionProcessRunReason::CodingAgent);
    }
}
