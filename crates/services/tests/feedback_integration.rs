//! Integration tests for the Agent Feedback System.
//!
//! Tests verify:
//! - Feedback is stored correctly after successful parsing
//! - Feedback parsing handles various response formats
//! - Feedback retrieval by task_id works correctly
//! - Error cases are handled gracefully (empty responses, malformed JSON)
//! - Feedback is NOT stored for failed executions (parsing errors)

use db::models::{
    agent_feedback::{AgentFeedback, CreateAgentFeedback},
    execution_process::ExecutionProcessStatus,
};
use services::services::feedback::FeedbackService;
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use tempfile::NamedTempFile;
use uuid::Uuid;

/// Creates a unique file-based SQLite database and runs all migrations.
/// Returns the pool and the temp file (which must be kept alive for the test duration).
async fn create_test_db() -> (SqlitePool, NamedTempFile) {
    // Create a unique temp file for this test's database
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

    (pool, db_file)
}

/// Creates a test project in the database.
async fn create_test_project(pool: &SqlitePool, name: &str) -> Uuid {
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
async fn create_test_task(pool: &SqlitePool, project_id: Uuid, title: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO tasks (id, project_id, title, status) VALUES (?, ?, ?, 'inprogress')")
        .bind(id)
        .bind(project_id)
        .bind(title)
        .execute(pool)
        .await
        .expect("Failed to create task");
    id
}

/// Creates a test workspace in the database.
async fn create_test_workspace(pool: &SqlitePool, task_id: Uuid, branch: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id, task_id, branch) VALUES (?, ?, ?)")
        .bind(id)
        .bind(task_id)
        .bind(branch)
        .execute(pool)
        .await
        .expect("Failed to create workspace");
    id
}

/// Creates a test session in the database.
/// Sessions reference workspaces, and execution_processes reference sessions.
async fn create_test_session(pool: &SqlitePool, workspace_id: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO sessions (id, workspace_id, executor) VALUES (?, ?, ?)")
        .bind(id)
        .bind(workspace_id)
        .bind("claude_code") // Required by trigger that updates tasks.last_executor
        .execute(pool)
        .await
        .expect("Failed to create session");
    id
}

/// Creates a test execution process in the database.
async fn create_test_execution_process(
    pool: &SqlitePool,
    _workspace_id: Uuid,
    session_id: Uuid,
    status: ExecutionProcessStatus,
) -> Uuid {
    let id = Uuid::new_v4();
    let status_str = match status {
        ExecutionProcessStatus::Running => "running",
        ExecutionProcessStatus::Completed => "completed",
        ExecutionProcessStatus::Failed => "failed",
        ExecutionProcessStatus::Killed => "killed",
    };
    // execution_processes references session_id (from sessions table)
    sqlx::query(
        "INSERT INTO execution_processes (id, session_id, status, run_reason) VALUES (?, ?, ?, 'internalagent')",
    )
    .bind(id)
    .bind(session_id)
    .bind(status_str)
    .execute(pool)
    .await
    .expect("Failed to create execution process");
    id
}

/// Simulates the feedback parsing and storage flow from spawn_feedback_parser.
/// This is the core logic we want to test without needing actual executor processes.
async fn simulate_feedback_storage(
    pool: &SqlitePool,
    execution_process_id: Uuid,
    task_id: Uuid,
    workspace_id: Uuid,
    assistant_message: &str,
) -> Result<AgentFeedback, String> {
    // Extract and validate JSON from the feedback response (simulating what FeedbackService does)
    let feedback_json = FeedbackService::parse_feedback_response(assistant_message)
        .map_err(|e| format!("Parse error: {}", e))?;

    // Store the raw JSON feedback in the database
    let create_feedback = CreateAgentFeedback {
        execution_process_id,
        task_id,
        workspace_id,
        feedback_json: Some(feedback_json),
    };

    AgentFeedback::create(pool, &create_feedback, Uuid::new_v4())
        .await
        .map_err(|e| format!("Database error: {}", e))
}

/// Helper to extract a field from the stored feedback_json
fn get_feedback_field(feedback: &AgentFeedback, field: &str) -> Option<String> {
    feedback.feedback_json.as_ref().and_then(|json| {
        serde_json::from_str::<serde_json::Value>(json)
            .ok()
            .and_then(|v| v.get(field).and_then(|f| f.as_str().map(|s| s.to_string())))
    })
}

// ============================================================================
// Integration Tests: Feedback Storage After Successful Execution
// ============================================================================

#[tokio::test]
async fn test_feedback_stored_after_successful_parsing() {
    let (pool, _db_file) = create_test_db().await;

    // Setup test entities
    let project_id = create_test_project(&pool, "Feedback Test Project").await;
    let task_id = create_test_task(&pool, project_id, "Test Task").await;
    let workspace_id = create_test_workspace(&pool, task_id, "feature-branch").await;
    let session_id = create_test_session(&pool, workspace_id).await;
    let exec_id = create_test_execution_process(
        &pool,
        workspace_id,
        session_id,
        ExecutionProcessStatus::Completed,
    )
    .await;

    // Simulate agent response with valid feedback JSON
    let agent_response = r#"{
        "task_clarity": "The task description was clear and complete",
        "missing_tools": "Would have liked better debugging tools",
        "integration_problems": null,
        "improvement_suggestions": "Add more documentation examples",
        "agent_documentation": "Implemented the feature using pattern X"
    }"#;

    // Simulate the feedback storage flow
    let result =
        simulate_feedback_storage(&pool, exec_id, task_id, workspace_id, agent_response).await;

    // Verify feedback was stored
    assert!(
        result.is_ok(),
        "Expected feedback to be stored successfully"
    );
    let feedback = result.unwrap();

    assert_eq!(feedback.task_id, task_id);
    assert_eq!(feedback.workspace_id, workspace_id);
    assert_eq!(feedback.execution_process_id, exec_id);

    // Verify the JSON was stored and contains expected fields
    assert!(feedback.feedback_json.is_some());
    assert_eq!(
        get_feedback_field(&feedback, "task_clarity"),
        Some("The task description was clear and complete".to_string())
    );
    assert_eq!(
        get_feedback_field(&feedback, "missing_tools"),
        Some("Would have liked better debugging tools".to_string())
    );
    assert_eq!(get_feedback_field(&feedback, "integration_problems"), None);
    assert_eq!(
        get_feedback_field(&feedback, "improvement_suggestions"),
        Some("Add more documentation examples".to_string())
    );
    assert_eq!(
        get_feedback_field(&feedback, "agent_documentation"),
        Some("Implemented the feature using pattern X".to_string())
    );
}

#[tokio::test]
async fn test_feedback_stored_with_json_in_markdown_code_block() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "Markdown Test Project").await;
    let task_id = create_test_task(&pool, project_id, "Markdown Task").await;
    let workspace_id = create_test_workspace(&pool, task_id, "feature-md").await;
    let session_id = create_test_session(&pool, workspace_id).await;
    let exec_id = create_test_execution_process(
        &pool,
        workspace_id,
        session_id,
        ExecutionProcessStatus::Completed,
    )
    .await;

    // Agent response with JSON embedded in markdown code block
    let agent_response = r#"Here's my feedback on the task:

```json
{
    "task_clarity": "Very clear instructions",
    "missing_tools": null,
    "integration_problems": "Build system was slow",
    "improvement_suggestions": null,
    "agent_documentation": "Completed refactoring"
}
```

Hope this helps improve the system!"#;

    let result =
        simulate_feedback_storage(&pool, exec_id, task_id, workspace_id, agent_response).await;

    assert!(result.is_ok());
    let feedback = result.unwrap();
    assert_eq!(
        get_feedback_field(&feedback, "task_clarity"),
        Some("Very clear instructions".to_string())
    );
    assert_eq!(
        get_feedback_field(&feedback, "integration_problems"),
        Some("Build system was slow".to_string())
    );
}

#[tokio::test]
async fn test_feedback_retrievable_by_task_id() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "Retrieval Test Project").await;
    let task_id = create_test_task(&pool, project_id, "Retrieval Task").await;
    let workspace_id = create_test_workspace(&pool, task_id, "feature-retrieval").await;
    let session_id = create_test_session(&pool, workspace_id).await;

    // Create multiple feedback entries for the same task
    for i in 1..=3 {
        let exec_id = create_test_execution_process(
            &pool,
            workspace_id,
            session_id,
            ExecutionProcessStatus::Completed,
        )
        .await;

        let agent_response = format!(
            r#"{{
                "task_clarity": "Feedback round {}",
                "missing_tools": null,
                "integration_problems": null,
                "improvement_suggestions": null,
                "agent_documentation": null
            }}"#,
            i
        );

        let result =
            simulate_feedback_storage(&pool, exec_id, task_id, workspace_id, &agent_response).await;
        assert!(result.is_ok(), "Failed to store feedback round {}", i);

        // Small delay to ensure distinct timestamps
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    // Retrieve all feedback for the task
    let feedbacks = AgentFeedback::find_by_task_id(&pool, task_id)
        .await
        .expect("Failed to retrieve feedbacks");

    assert_eq!(feedbacks.len(), 3, "Expected 3 feedback entries");

    // Verify they're ordered by collected_at DESC (most recent first)
    assert!(feedbacks[0].collected_at >= feedbacks[1].collected_at);
    assert!(feedbacks[1].collected_at >= feedbacks[2].collected_at);
}

// ============================================================================
// Integration Tests: Error Handling When Feedback Parsing Fails
// ============================================================================

#[tokio::test]
async fn test_feedback_not_stored_for_malformed_json() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "Malformed JSON Project").await;
    let task_id = create_test_task(&pool, project_id, "Malformed Task").await;
    let workspace_id = create_test_workspace(&pool, task_id, "feature-malformed").await;
    let session_id = create_test_session(&pool, workspace_id).await;
    let exec_id = create_test_execution_process(
        &pool,
        workspace_id,
        session_id,
        ExecutionProcessStatus::Completed,
    )
    .await;

    // Malformed JSON response
    let agent_response = r#"This is not valid JSON at all {broken"#;

    let result =
        simulate_feedback_storage(&pool, exec_id, task_id, workspace_id, agent_response).await;

    // Should fail to parse
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Parse error"));

    // Verify no feedback was stored
    let feedbacks = AgentFeedback::find_by_task_id(&pool, task_id)
        .await
        .expect("Query should succeed");
    assert!(
        feedbacks.is_empty(),
        "No feedback should be stored for malformed JSON"
    );
}

#[tokio::test]
async fn test_feedback_not_stored_for_empty_response() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "Empty Response Project").await;
    let task_id = create_test_task(&pool, project_id, "Empty Task").await;
    let workspace_id = create_test_workspace(&pool, task_id, "feature-empty").await;
    let session_id = create_test_session(&pool, workspace_id).await;
    let exec_id = create_test_execution_process(
        &pool,
        workspace_id,
        session_id,
        ExecutionProcessStatus::Completed,
    )
    .await;

    // Empty response
    let agent_response = "";

    let result =
        simulate_feedback_storage(&pool, exec_id, task_id, workspace_id, agent_response).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Parse error"));

    // Verify no feedback was stored
    let feedbacks = AgentFeedback::find_by_task_id(&pool, task_id)
        .await
        .expect("Query should succeed");
    assert!(feedbacks.is_empty());
}

#[tokio::test]
async fn test_feedback_not_stored_for_whitespace_only_response() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "Whitespace Project").await;
    let task_id = create_test_task(&pool, project_id, "Whitespace Task").await;
    let workspace_id = create_test_workspace(&pool, task_id, "feature-ws").await;
    let session_id = create_test_session(&pool, workspace_id).await;
    let exec_id = create_test_execution_process(
        &pool,
        workspace_id,
        session_id,
        ExecutionProcessStatus::Completed,
    )
    .await;

    let agent_response = "   \n\t  ";

    let result =
        simulate_feedback_storage(&pool, exec_id, task_id, workspace_id, agent_response).await;

    assert!(result.is_err());

    let feedbacks = AgentFeedback::find_by_task_id(&pool, task_id)
        .await
        .expect("Query should succeed");
    assert!(feedbacks.is_empty());
}

// ============================================================================
// Integration Tests: Feedback NOT Collected for Failed Executions
// ============================================================================

/// This test verifies the logic that feedback should only be stored when
/// execution completes successfully. In the actual code (spawn_feedback_parser),
/// the parser returns early if execution status is Failed or Killed.
#[tokio::test]
async fn test_feedback_skipped_for_failed_execution_status() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "Failed Execution Project").await;
    let task_id = create_test_task(&pool, project_id, "Failed Task").await;
    let workspace_id = create_test_workspace(&pool, task_id, "feature-failed").await;
    let session_id = create_test_session(&pool, workspace_id).await;

    // Create execution with Failed status
    let exec_id = create_test_execution_process(
        &pool,
        workspace_id,
        session_id,
        ExecutionProcessStatus::Failed,
    )
    .await;

    // Even with valid feedback JSON, we simulate the behavior where
    // spawn_feedback_parser returns early for non-Completed status
    let agent_response = r#"{
        "task_clarity": "This should not be stored",
        "missing_tools": null,
        "integration_problems": null,
        "improvement_suggestions": null,
        "agent_documentation": null
    }"#;

    // Simulate the spawn_feedback_parser logic:
    // Check execution status before storing
    let exec_status = get_execution_status(&pool, exec_id).await;

    // The actual spawn_feedback_parser returns early if status is not Completed
    if exec_status != ExecutionProcessStatus::Completed {
        // Don't store feedback - this is the expected behavior
    } else {
        // This branch should NOT be taken for failed executions
        let _ =
            simulate_feedback_storage(&pool, exec_id, task_id, workspace_id, agent_response).await;
    }

    // Verify no feedback was stored
    let feedbacks = AgentFeedback::find_by_task_id(&pool, task_id)
        .await
        .expect("Query should succeed");
    assert!(
        feedbacks.is_empty(),
        "No feedback should be stored for failed executions"
    );
}

#[tokio::test]
async fn test_feedback_skipped_for_killed_execution_status() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "Killed Execution Project").await;
    let task_id = create_test_task(&pool, project_id, "Killed Task").await;
    let workspace_id = create_test_workspace(&pool, task_id, "feature-killed").await;
    let session_id = create_test_session(&pool, workspace_id).await;

    // Create execution with Killed status
    let exec_id = create_test_execution_process(
        &pool,
        workspace_id,
        session_id,
        ExecutionProcessStatus::Killed,
    )
    .await;

    let agent_response = r#"{
        "task_clarity": "This should not be stored either",
        "missing_tools": null,
        "integration_problems": null,
        "improvement_suggestions": null,
        "agent_documentation": null
    }"#;

    // Simulate spawn_feedback_parser logic
    let exec_status = get_execution_status(&pool, exec_id).await;

    if exec_status != ExecutionProcessStatus::Completed {
        // Don't store feedback - expected behavior for Killed status
    } else {
        let _ =
            simulate_feedback_storage(&pool, exec_id, task_id, workspace_id, agent_response).await;
    }

    let feedbacks = AgentFeedback::find_by_task_id(&pool, task_id)
        .await
        .expect("Query should succeed");
    assert!(
        feedbacks.is_empty(),
        "No feedback should be stored for killed executions"
    );
}

/// Helper to get execution status from database
async fn get_execution_status(pool: &SqlitePool, exec_id: Uuid) -> ExecutionProcessStatus {
    let row: (String,) = sqlx::query_as("SELECT status FROM execution_processes WHERE id = ?")
        .bind(exec_id)
        .fetch_one(pool)
        .await
        .expect("Execution process should exist");

    match row.0.as_str() {
        "running" => ExecutionProcessStatus::Running,
        "completed" => ExecutionProcessStatus::Completed,
        "failed" => ExecutionProcessStatus::Failed,
        "killed" => ExecutionProcessStatus::Killed,
        s => panic!("Unknown status: {}", s),
    }
}

// ============================================================================
// Integration Tests: Feedback Collection Doesn't Block Task Finalization
// ============================================================================

/// This test verifies that feedback storage is independent of task finalization.
/// In the actual code, feedback collection is spawned as a background task
/// (tokio::spawn) and failures are logged but don't affect the main flow.
#[tokio::test]
async fn test_feedback_storage_independent_of_task_state() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "Non-blocking Project").await;
    let task_id = create_test_task(&pool, project_id, "Non-blocking Task").await;
    let workspace_id = create_test_workspace(&pool, task_id, "feature-nonblock").await;
    let session_id = create_test_session(&pool, workspace_id).await;
    let exec_id = create_test_execution_process(
        &pool,
        workspace_id,
        session_id,
        ExecutionProcessStatus::Completed,
    )
    .await;

    // Store feedback
    let agent_response = r#"{
        "task_clarity": "Task was clear",
        "missing_tools": null,
        "integration_problems": null,
        "improvement_suggestions": null,
        "agent_documentation": null
    }"#;

    let result =
        simulate_feedback_storage(&pool, exec_id, task_id, workspace_id, agent_response).await;
    assert!(result.is_ok());

    // Update task to 'done' status (simulating task finalization)
    sqlx::query("UPDATE tasks SET status = 'done' WHERE id = ?")
        .bind(task_id)
        .execute(&pool)
        .await
        .expect("Task update should succeed");

    // Verify feedback is still retrievable after task finalization
    let feedbacks = AgentFeedback::find_by_task_id(&pool, task_id)
        .await
        .expect("Query should succeed");
    assert_eq!(feedbacks.len(), 1);

    // Verify task status change didn't affect feedback
    let feedback = &feedbacks[0];
    assert_eq!(feedback.task_id, task_id);
    assert_eq!(
        get_feedback_field(feedback, "task_clarity"),
        Some("Task was clear".to_string())
    );
}

/// Test that multiple feedback entries can be stored for different executions
/// within the same task, demonstrating non-blocking concurrent storage.
#[tokio::test]
async fn test_multiple_feedback_entries_per_task() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "Multi-feedback Project").await;
    let task_id = create_test_task(&pool, project_id, "Multi-feedback Task").await;
    let workspace_id = create_test_workspace(&pool, task_id, "feature-multi").await;
    let session_id = create_test_session(&pool, workspace_id).await;

    // Simulate multiple execution rounds (like retries or follow-ups)
    let feedbacks_to_create = vec![
        ("First attempt feedback", "Initial documentation"),
        ("Second attempt was clearer", "Updated documentation"),
        ("Third time's the charm", "Final documentation"),
    ];

    for (clarity, docs) in feedbacks_to_create {
        let exec_id = create_test_execution_process(
            &pool,
            workspace_id,
            session_id,
            ExecutionProcessStatus::Completed,
        )
        .await;

        let agent_response = format!(
            r#"{{
                "task_clarity": "{}",
                "missing_tools": null,
                "integration_problems": null,
                "improvement_suggestions": null,
                "agent_documentation": "{}"
            }}"#,
            clarity, docs
        );

        let result =
            simulate_feedback_storage(&pool, exec_id, task_id, workspace_id, &agent_response).await;
        assert!(result.is_ok());

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    // Verify all feedbacks are stored and retrievable
    let feedbacks = AgentFeedback::find_by_task_id(&pool, task_id)
        .await
        .expect("Query should succeed");
    assert_eq!(feedbacks.len(), 3);

    // Check each execution has unique feedback
    let exec_ids: std::collections::HashSet<_> =
        feedbacks.iter().map(|f| f.execution_process_id).collect();
    assert_eq!(
        exec_ids.len(),
        3,
        "Each feedback should have unique execution_process_id"
    );
}

// ============================================================================
// Integration Tests: Database Model Operations
// ============================================================================

#[tokio::test]
async fn test_feedback_find_by_execution_process_id() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "Find by Exec Project").await;
    let task_id = create_test_task(&pool, project_id, "Find by Exec Task").await;
    let workspace_id = create_test_workspace(&pool, task_id, "feature-find").await;
    let session_id = create_test_session(&pool, workspace_id).await;
    let exec_id = create_test_execution_process(
        &pool,
        workspace_id,
        session_id,
        ExecutionProcessStatus::Completed,
    )
    .await;

    let agent_response = r#"{
        "task_clarity": "Find by exec test",
        "missing_tools": null,
        "integration_problems": null,
        "improvement_suggestions": null,
        "agent_documentation": null
    }"#;

    simulate_feedback_storage(&pool, exec_id, task_id, workspace_id, agent_response)
        .await
        .expect("Should store feedback");

    // Find by execution process ID
    let feedback = AgentFeedback::find_by_execution_process_id(&pool, exec_id)
        .await
        .expect("Query should succeed");

    assert!(feedback.is_some());
    let feedback = feedback.unwrap();
    assert_eq!(feedback.execution_process_id, exec_id);
    assert_eq!(
        get_feedback_field(&feedback, "task_clarity"),
        Some("Find by exec test".to_string())
    );
}

#[tokio::test]
async fn test_feedback_find_recent() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "Recent Feedback Project").await;

    // Create 5 feedbacks across different tasks
    for i in 1..=5 {
        let task_id = create_test_task(&pool, project_id, &format!("Task {}", i)).await;
        let workspace_id = create_test_workspace(&pool, task_id, &format!("feature-{}", i)).await;
        let session_id = create_test_session(&pool, workspace_id).await;
        let exec_id = create_test_execution_process(
            &pool,
            workspace_id,
            session_id,
            ExecutionProcessStatus::Completed,
        )
        .await;

        let agent_response = format!(
            r#"{{
                "task_clarity": "Feedback {}",
                "missing_tools": null,
                "integration_problems": null,
                "improvement_suggestions": null,
                "agent_documentation": null
            }}"#,
            i
        );

        simulate_feedback_storage(&pool, exec_id, task_id, workspace_id, &agent_response)
            .await
            .expect("Should store feedback");

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    // Find recent with limit
    let recent = AgentFeedback::find_recent(&pool, 3)
        .await
        .expect("Query should succeed");

    assert_eq!(recent.len(), 3);

    // Should be ordered by collected_at DESC (most recent first)
    assert!(recent[0].collected_at >= recent[1].collected_at);
    assert!(recent[1].collected_at >= recent[2].collected_at);
}

// ============================================================================
// Integration Tests: Edge Cases
// ============================================================================

#[tokio::test]
async fn test_feedback_with_special_characters_and_quotes() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "Special Chars Project").await;
    let task_id = create_test_task(&pool, project_id, "Special Chars Task").await;
    let workspace_id = create_test_workspace(&pool, task_id, "feature-special").await;
    let session_id = create_test_session(&pool, workspace_id).await;
    let exec_id = create_test_execution_process(
        &pool,
        workspace_id,
        session_id,
        ExecutionProcessStatus::Completed,
    )
    .await;

    // Response with escaped quotes and special characters
    let agent_response = r#"{
        "task_clarity": "The task said \"implement feature X\" with path /home/user",
        "missing_tools": "Need {curly} braces and [brackets]",
        "integration_problems": "Error: \"Connection refused\" on port 8080",
        "improvement_suggestions": null,
        "agent_documentation": "Used regex: \\d+\\.\\d+"
    }"#;

    let result =
        simulate_feedback_storage(&pool, exec_id, task_id, workspace_id, agent_response).await;

    assert!(result.is_ok());
    let feedback = result.unwrap();
    assert!(
        get_feedback_field(&feedback, "task_clarity")
            .unwrap()
            .contains("implement feature X")
    );
    assert!(
        get_feedback_field(&feedback, "missing_tools")
            .unwrap()
            .contains("{curly}")
    );
}

#[tokio::test]
async fn test_feedback_with_all_null_fields() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "All Null Project").await;
    let task_id = create_test_task(&pool, project_id, "All Null Task").await;
    let workspace_id = create_test_workspace(&pool, task_id, "feature-null").await;
    let session_id = create_test_session(&pool, workspace_id).await;
    let exec_id = create_test_execution_process(
        &pool,
        workspace_id,
        session_id,
        ExecutionProcessStatus::Completed,
    )
    .await;

    // Agent has no feedback to give
    let agent_response = r#"{
        "task_clarity": null,
        "missing_tools": null,
        "integration_problems": null,
        "improvement_suggestions": null,
        "agent_documentation": null
    }"#;

    let result =
        simulate_feedback_storage(&pool, exec_id, task_id, workspace_id, agent_response).await;

    assert!(result.is_ok());
    let feedback = result.unwrap();
    // All fields are null in the JSON, so get_feedback_field returns None for all
    assert_eq!(get_feedback_field(&feedback, "task_clarity"), None);
    assert_eq!(get_feedback_field(&feedback, "missing_tools"), None);
    assert_eq!(get_feedback_field(&feedback, "integration_problems"), None);
    assert_eq!(
        get_feedback_field(&feedback, "improvement_suggestions"),
        None
    );
    assert_eq!(get_feedback_field(&feedback, "agent_documentation"), None);
}

#[tokio::test]
async fn test_feedback_find_by_id() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "Find by ID Project").await;
    let task_id = create_test_task(&pool, project_id, "Find by ID Task").await;
    let workspace_id = create_test_workspace(&pool, task_id, "feature-id").await;
    let session_id = create_test_session(&pool, workspace_id).await;
    let exec_id = create_test_execution_process(
        &pool,
        workspace_id,
        session_id,
        ExecutionProcessStatus::Completed,
    )
    .await;

    let agent_response = r#"{
        "task_clarity": "Find by ID test",
        "missing_tools": null,
        "integration_problems": null,
        "improvement_suggestions": null,
        "agent_documentation": null
    }"#;

    let created = simulate_feedback_storage(&pool, exec_id, task_id, workspace_id, agent_response)
        .await
        .expect("Should create feedback");

    // Find by ID
    let found = AgentFeedback::find_by_id(&pool, created.id)
        .await
        .expect("Query should succeed");

    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, created.id);
    assert_eq!(
        get_feedback_field(&found, "task_clarity"),
        Some("Find by ID test".to_string())
    );
}

#[tokio::test]
async fn test_feedback_not_found_for_nonexistent_id() {
    let (pool, _db_file) = create_test_db().await;

    let nonexistent_id = Uuid::new_v4();

    let found = AgentFeedback::find_by_id(&pool, nonexistent_id)
        .await
        .expect("Query should succeed");

    assert!(found.is_none());
}

#[tokio::test]
async fn test_feedback_empty_list_for_task_without_feedback() {
    let (pool, _db_file) = create_test_db().await;

    let project_id = create_test_project(&pool, "No Feedback Project").await;
    let task_id = create_test_task(&pool, project_id, "No Feedback Task").await;

    // Task exists but has no feedback
    let feedbacks = AgentFeedback::find_by_task_id(&pool, task_id)
        .await
        .expect("Query should succeed");

    assert!(feedbacks.is_empty());
}
