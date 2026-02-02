//! Review-to-Merge Flow E2E Tests
//!
//! Tests the middle part of autopilot flow:
//! review execution completes → parse response → needs_attention decision →
//! merge queue enqueue → merge processing → task Done.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use db::models::{
    merge::Merge,
    task::{Task, TaskStatus},
    workspace_repo::{CreateWorkspaceRepo, WorkspaceRepo},
};
use git2::Repository;
use services::services::{
    config::Config,
    git::GitService,
    merge_queue_processor::MergeQueueProcessor,
    merge_queue_store::MergeQueueStore,
    review_attention::ReviewAttentionService,
};
use sqlx::SqlitePool;
use tempfile::TempDir;
use tokio::sync::RwLock;
use utils::msg_store::MsgStore;
use uuid::Uuid;

use super::fixtures::{autopilot_config, autopilot_disabled_config, EntityGraphBuilder, TestDb};

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

/// Test context containing all the pieces needed for review-to-merge tests.
struct TestContext {
    pool: SqlitePool,
    git: GitService,
    merge_queue_store: MergeQueueStore,
    config: Arc<RwLock<Config>>,
    _test_db: TestDb,
}

impl TestContext {
    async fn new() -> Self {
        Self::with_config(autopilot_config()).await
    }

    async fn with_config(config: Arc<RwLock<Config>>) -> Self {
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

    fn processor(&self) -> MergeQueueProcessor {
        MergeQueueProcessor::new(
            self.pool.clone(),
            self.git.clone(),
            self.merge_queue_store.clone(),
            self.config.clone(),
        )
    }
}

/// Creates a repo in the database pointing to the test repository.
async fn create_repo(pool: &SqlitePool, path: &std::path::Path, name: &str) -> Uuid {
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
async fn update_workspace_container_ref(pool: &SqlitePool, workspace_id: Uuid, container_ref: &str) {
    sqlx::query("UPDATE workspaces SET container_ref = ? WHERE id = ?")
        .bind(container_ref)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("Failed to update workspace container_ref");
}

/// Creates a WorkspaceRepo linking workspace to repo with target branch.
async fn create_workspace_repo(
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

/// Helper to add a file and commit in a worktree.
fn add_and_commit(worktree_path: &std::path::Path, filename: &str, content: &str, message: &str) {
    use git2::Repository;

    let repo = Repository::open(worktree_path).expect("Failed to open repo");
    let file_path = worktree_path.join(filename);
    std::fs::write(&file_path, content).expect("Failed to write file");

    let mut index = repo.index().expect("Failed to get index");
    index
        .add_path(std::path::Path::new(filename))
        .expect("Failed to add file");
    index.write().expect("Failed to write index");

    let tree_id = index.write_tree().expect("Failed to write tree");
    let tree = repo.find_tree(tree_id).expect("Failed to find tree");
    let sig = repo.signature().expect("Failed to get signature");
    let parent = repo.head().unwrap().peel_to_commit().unwrap();

    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])
        .expect("Failed to commit");
}

#[tokio::test]
async fn test_review_needs_attention_false_enqueues_merge() {
    // Setup: task InReview, workspace with repo, completed review execution
    let ctx = TestContext::new().await;
    let test_repo = TestRepo::new("review-merge-test");

    // Create project and task in InReview status
    let task_ctx = EntityGraphBuilder::new(ctx.pool.clone())
        .with_project("Test Project")
        .create_task("Task under review", TaskStatus::InReview)
        .await
        .with_workspace("task-branch")
        .await;

    let task_id = task_ctx.task_id();
    let workspace_id = task_ctx.workspace_id();
    let project_id = task_ctx.project_id();

    // Create repo in database
    let repo_id = create_repo(&ctx.pool, &test_repo.path, &test_repo.name).await;

    // Create worktree for task branch and link it to workspace
    let worktree_path = test_repo.create_worktree("task-branch");
    let worktree_parent = worktree_path.parent().unwrap().to_string_lossy().to_string();
    update_workspace_container_ref(&ctx.pool, workspace_id, &worktree_parent).await;

    // Link workspace to repo with target branch = main
    create_workspace_repo(&ctx.pool, workspace_id, repo_id, "main").await;

    // Add a commit on the task branch (simulating work done)
    add_and_commit(
        &worktree_path,
        "feature.txt",
        "new feature content",
        "Add feature",
    );

    // Simulate review response: needs_attention = false
    let review_response = r#"{"needs_attention": false, "reasoning": "All tests pass"}"#;
    let parsed = ReviewAttentionService::parse_review_attention_response(review_response)
        .expect("Should parse review response");

    assert!(!parsed.needs_attention);

    // Update task's needs_attention based on review
    Task::update_needs_attention(&ctx.pool, task_id, Some(parsed.needs_attention))
        .await
        .expect("Failed to update needs_attention");

    // Since needs_attention = false, enqueue to merge queue
    let commit_message = "Merge task-branch: All tests pass";
    ctx.merge_queue_store.enqueue(
        project_id,
        workspace_id,
        repo_id,
        commit_message.to_string(),
    );

    // Verify entry is in queue
    assert_eq!(ctx.merge_queue_store.count_by_project(project_id), 1);

    // Process the merge queue
    let processor = ctx.processor();
    processor
        .process_project_queue(project_id)
        .await
        .expect("Merge queue processing should succeed");

    // Assert: Task status = Done
    let task = Task::find_by_id(&ctx.pool, task_id)
        .await
        .expect("Query should succeed")
        .expect("Task should exist");
    assert_eq!(task.status, TaskStatus::Done);

    // Assert: Merge record created
    let merges = Merge::find_by_workspace_id(&ctx.pool, workspace_id)
        .await
        .expect("Query should succeed");
    assert_eq!(merges.len(), 1);

    // Assert: Queue is empty
    assert_eq!(ctx.merge_queue_store.count_by_project(project_id), 0);

    // Assert: needs_attention was set to false
    let task = Task::find_by_id(&ctx.pool, task_id)
        .await
        .expect("Query should succeed")
        .expect("Task should exist");
    assert_eq!(task.needs_attention, Some(false));
}

#[tokio::test]
async fn test_review_needs_attention_true_stays_inreview() {
    // Setup: task InReview, workspace with repo
    let ctx = TestContext::new().await;

    // Create project and task in InReview status
    let task_ctx = EntityGraphBuilder::new(ctx.pool.clone())
        .with_project("Test Project")
        .create_task("Task needing attention", TaskStatus::InReview)
        .await
        .with_workspace("attention-branch")
        .await;

    let task_id = task_ctx.task_id();
    let workspace_id = task_ctx.workspace_id();
    let project_id = task_ctx.project_id();

    // Simulate review response: needs_attention = true
    let review_response =
        r#"{"needs_attention": true, "reasoning": "Tests are failing for edge cases"}"#;
    let parsed = ReviewAttentionService::parse_review_attention_response(review_response)
        .expect("Should parse review response");

    assert!(parsed.needs_attention);

    // Update task's needs_attention based on review
    Task::update_needs_attention(&ctx.pool, task_id, Some(parsed.needs_attention))
        .await
        .expect("Failed to update needs_attention");

    // Since needs_attention = true, do NOT enqueue to merge queue
    // (In real flow, this decision is made by the handler)

    // Assert: Task stays InReview
    let task = Task::find_by_id(&ctx.pool, task_id)
        .await
        .expect("Query should succeed")
        .expect("Task should exist");
    assert_eq!(task.status, TaskStatus::InReview);

    // Assert: NOT enqueued to merge queue
    assert_eq!(ctx.merge_queue_store.count_by_project(project_id), 0);
    assert!(ctx.merge_queue_store.get(workspace_id).is_none());

    // Assert: needs_attention was set to true
    assert_eq!(task.needs_attention, Some(true));
}

#[tokio::test]
async fn test_merge_conflict_removes_from_queue_continues() {
    // Setup: two tasks in merge queue, first has merge conflict.
    // Use two separate test repos to avoid complexity with worktrees sharing state.
    let ctx = TestContext::new().await;

    // Create two separate test repos for clean isolation
    let test_repo1 = TestRepo::new("conflict-repo");
    let test_repo2 = TestRepo::new("clean-repo");

    // Create project with two tasks
    let task1_ctx = EntityGraphBuilder::new(ctx.pool.clone())
        .with_project("Conflict Project")
        .create_task("Task 1 - will conflict", TaskStatus::InReview)
        .await
        .with_workspace("conflict-branch")
        .await;

    let project_id = task1_ctx.project_id();
    let task1_id = task1_ctx.task_id();
    let workspace1_id = task1_ctx.workspace_id();

    let task2_ctx = task1_ctx
        .builder()
        .create_task("Task 2 - clean merge", TaskStatus::InReview)
        .await
        .with_workspace("clean-branch")
        .await;

    let task2_id = task2_ctx.task_id();
    let workspace2_id = task2_ctx.workspace_id();

    // Create repos in database
    let repo1_id = create_repo(&ctx.pool, &test_repo1.path, &test_repo1.name).await;
    let repo2_id = create_repo(&ctx.pool, &test_repo2.path, &test_repo2.name).await;

    // Create worktrees
    let worktree1_path = test_repo1.create_worktree("conflict-branch");
    let worktree2_path = test_repo2.create_worktree("clean-branch");

    // Set up workspace container refs
    let worktree1_parent = worktree1_path
        .parent()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let worktree2_parent = worktree2_path
        .parent()
        .unwrap()
        .to_string_lossy()
        .to_string();

    update_workspace_container_ref(&ctx.pool, workspace1_id, &worktree1_parent).await;
    update_workspace_container_ref(&ctx.pool, workspace2_id, &worktree2_parent).await;

    // Link workspaces to their repos
    create_workspace_repo(&ctx.pool, workspace1_id, repo1_id, "main").await;
    create_workspace_repo(&ctx.pool, workspace2_id, repo2_id, "main").await;

    // Setup Task 1: Create a conflict with main
    // 1. First commit on task branch
    add_and_commit(
        &worktree1_path,
        "README.md",
        "# Test Repository\n\nConflicting content from task 1",
        "Task 1: Modify README",
    );

    // 2. Add a conflicting commit to main in repo1
    {
        let repo = Repository::open(&test_repo1.path).expect("Failed to open repo");
        let main_ref = repo
            .find_branch("main", git2::BranchType::Local)
            .expect("main branch should exist");
        repo.set_head(main_ref.get().name().unwrap())
            .expect("set head to main");
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
            .expect("checkout main");

        let readme_path = test_repo1.path.join("README.md");
        std::fs::write(
            &readme_path,
            "# Test Repository\n\nDifferent content on main branch",
        )
        .expect("write readme");

        let mut index = repo.index().expect("get index");
        index
            .add_path(Path::new("README.md"))
            .expect("add readme");
        index.write().expect("write index");

        let tree_id = index.write_tree().expect("write tree");
        let tree = repo.find_tree(tree_id).expect("find tree");
        let sig = repo.signature().expect("get sig");
        let parent = repo.head().unwrap().peel_to_commit().unwrap();

        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            "Main: Different README change",
            &tree,
            &[&parent],
        )
        .expect("commit");
    }

    // Setup Task 2: Clean change that won't conflict
    add_and_commit(
        &worktree2_path,
        "new_file.txt",
        "New content",
        "Task 2: Add new file",
    );

    // Enqueue both tasks (task 1 first, then task 2)
    ctx.merge_queue_store.enqueue(
        project_id,
        workspace1_id,
        repo1_id,
        "Merge conflict-branch".to_string(),
    );

    // Small delay to ensure different timestamps
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    ctx.merge_queue_store.enqueue(
        project_id,
        workspace2_id,
        repo2_id,
        "Merge clean-branch".to_string(),
    );

    // Verify both are queued
    assert_eq!(ctx.merge_queue_store.count_by_project(project_id), 2);

    // Process the queue - task 1 should fail with conflict, task 2 should succeed
    let processor = ctx.processor();
    processor
        .process_project_queue(project_id)
        .await
        .expect("Processing should complete without fatal error");

    // Assert: Queue is empty (both processed)
    assert_eq!(ctx.merge_queue_store.count_by_project(project_id), 0);

    // Assert: Task 1 stays InReview (conflict prevented merge)
    let task1 = Task::find_by_id(&ctx.pool, task1_id)
        .await
        .expect("Query should succeed")
        .expect("Task should exist");
    assert_eq!(task1.status, TaskStatus::InReview);

    // Assert: Task 2 is Done (clean merge succeeded)
    let task2 = Task::find_by_id(&ctx.pool, task2_id)
        .await
        .expect("Query should succeed")
        .expect("Task should exist");
    assert_eq!(task2.status, TaskStatus::Done);

    // Assert: Only task 2 has a merge record
    let merges1 = Merge::find_by_workspace_id(&ctx.pool, workspace1_id)
        .await
        .expect("Query should succeed");
    assert_eq!(merges1.len(), 0, "Task 1 should have no merge record");

    let merges2 = Merge::find_by_workspace_id(&ctx.pool, workspace2_id)
        .await
        .expect("Query should succeed");
    assert_eq!(merges2.len(), 1, "Task 2 should have one merge record");
}

#[tokio::test]
async fn test_autopilot_disabled_skips_merge() {
    // Setup: autopilot disabled, review says needs_attention=false
    let ctx = TestContext::with_config(autopilot_disabled_config()).await;
    let test_repo = TestRepo::new("autopilot-disabled-test");

    // Create project and task in InReview status
    let task_ctx = EntityGraphBuilder::new(ctx.pool.clone())
        .with_project("Disabled Autopilot Project")
        .create_task("Task with autopilot disabled", TaskStatus::InReview)
        .await
        .with_workspace("disabled-branch")
        .await;

    let task_id = task_ctx.task_id();
    let workspace_id = task_ctx.workspace_id();
    let project_id = task_ctx.project_id();

    // Create repo in database
    let repo_id = create_repo(&ctx.pool, &test_repo.path, &test_repo.name).await;

    // Create worktree
    let worktree_path = test_repo.create_worktree("disabled-branch");
    let worktree_parent = worktree_path.parent().unwrap().to_string_lossy().to_string();
    update_workspace_container_ref(&ctx.pool, workspace_id, &worktree_parent).await;

    create_workspace_repo(&ctx.pool, workspace_id, repo_id, "main").await;

    // Add a commit on the task branch
    add_and_commit(
        &worktree_path,
        "feature.txt",
        "feature content",
        "Add feature",
    );

    // Simulate review response: needs_attention = false
    let review_response = r#"{"needs_attention": false, "reasoning": "All tests pass"}"#;
    let parsed = ReviewAttentionService::parse_review_attention_response(review_response)
        .expect("Should parse review response");

    assert!(!parsed.needs_attention);

    // Update task's needs_attention
    Task::update_needs_attention(&ctx.pool, task_id, Some(parsed.needs_attention))
        .await
        .expect("Failed to update needs_attention");

    // With autopilot disabled, no merge enqueue should happen
    // This simulates what the handler would check before enqueueing
    let autopilot_enabled = ctx.config.read().await.autopilot_enabled;
    assert!(!autopilot_enabled, "Autopilot should be disabled for this test");

    // Verify: No merge enqueue happens (we simulate the handler decision)
    // In real flow, the handler checks autopilot_enabled before enqueueing
    assert_eq!(ctx.merge_queue_store.count_by_project(project_id), 0);
    assert!(ctx.merge_queue_store.get(workspace_id).is_none());

    // Assert: Task stays InReview
    let task = Task::find_by_id(&ctx.pool, task_id)
        .await
        .expect("Query should succeed")
        .expect("Task should exist");
    assert_eq!(task.status, TaskStatus::InReview);

    // Assert: No merge record
    let merges = Merge::find_by_workspace_id(&ctx.pool, workspace_id)
        .await
        .expect("Query should succeed");
    assert!(merges.is_empty());
}
