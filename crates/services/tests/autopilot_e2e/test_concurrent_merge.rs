//! Concurrent Merge Queue Tests
//!
//! Tests that the merge queue correctly handles multiple entries and processes them in FIFO order.
//!
//! - `test_fifo_merge_queue_ordering`: Verifies entries are merged in enqueue order
//! - `test_conflict_skips_to_next`: Verifies conflicts don't block other tasks

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
};
use sqlx::SqlitePool;
use tempfile::TempDir;
use tokio::sync::RwLock;
use utils::msg_store::MsgStore;
use uuid::Uuid;

use super::fixtures::{autopilot_config, EntityGraphBuilder, TestDb};

/// A test git repository with worktree support.
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

/// Test context containing all the pieces needed for merge queue tests.
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
async fn create_repo(pool: &SqlitePool, path: &Path, name: &str) -> Uuid {
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
fn add_and_commit(worktree_path: &Path, filename: &str, content: &str, message: &str) {
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

/// Test that merge queue processes entries in FIFO order (oldest first).
///
/// Scenario:
/// 1. Enqueue 3 tasks (A, B, C) in order with small delays between
/// 2. Process queue
/// 3. Assert: Merge records created in A, B, C order (check timestamps)
#[tokio::test]
async fn test_fifo_merge_queue_ordering() {
    let ctx = TestContext::new().await;

    // Create 3 separate test repos for clean isolation
    let test_repo_a = TestRepo::new("repo-a");
    let test_repo_b = TestRepo::new("repo-b");
    let test_repo_c = TestRepo::new("repo-c");

    // Create project with 3 tasks
    let task_a_ctx = EntityGraphBuilder::new(ctx.pool.clone())
        .with_project("FIFO Test Project")
        .create_task("Task A", TaskStatus::InReview)
        .await
        .with_workspace("branch-a")
        .await;

    let project_id = task_a_ctx.project_id();
    let task_a_id = task_a_ctx.task_id();
    let workspace_a_id = task_a_ctx.workspace_id();

    let task_b_ctx = task_a_ctx
        .builder()
        .create_task("Task B", TaskStatus::InReview)
        .await
        .with_workspace("branch-b")
        .await;

    let task_b_id = task_b_ctx.task_id();
    let workspace_b_id = task_b_ctx.workspace_id();

    let task_c_ctx = task_b_ctx
        .builder()
        .create_task("Task C", TaskStatus::InReview)
        .await
        .with_workspace("branch-c")
        .await;

    let task_c_id = task_c_ctx.task_id();
    let workspace_c_id = task_c_ctx.workspace_id();

    // Create repos in database
    let repo_a_id = create_repo(&ctx.pool, &test_repo_a.path, &test_repo_a.name).await;
    let repo_b_id = create_repo(&ctx.pool, &test_repo_b.path, &test_repo_b.name).await;
    let repo_c_id = create_repo(&ctx.pool, &test_repo_c.path, &test_repo_c.name).await;

    // Create worktrees
    let worktree_a_path = test_repo_a.create_worktree("branch-a");
    let worktree_b_path = test_repo_b.create_worktree("branch-b");
    let worktree_c_path = test_repo_c.create_worktree("branch-c");

    // Set up workspace container refs
    update_workspace_container_ref(
        &ctx.pool,
        workspace_a_id,
        &worktree_a_path.parent().unwrap().to_string_lossy(),
    )
    .await;
    update_workspace_container_ref(
        &ctx.pool,
        workspace_b_id,
        &worktree_b_path.parent().unwrap().to_string_lossy(),
    )
    .await;
    update_workspace_container_ref(
        &ctx.pool,
        workspace_c_id,
        &worktree_c_path.parent().unwrap().to_string_lossy(),
    )
    .await;

    // Link workspaces to their repos
    create_workspace_repo(&ctx.pool, workspace_a_id, repo_a_id, "main").await;
    create_workspace_repo(&ctx.pool, workspace_b_id, repo_b_id, "main").await;
    create_workspace_repo(&ctx.pool, workspace_c_id, repo_c_id, "main").await;

    // Add commits on each task branch
    add_and_commit(&worktree_a_path, "file_a.txt", "content A", "Task A commit");
    add_and_commit(&worktree_b_path, "file_b.txt", "content B", "Task B commit");
    add_and_commit(&worktree_c_path, "file_c.txt", "content C", "Task C commit");

    // Enqueue tasks in order A, B, C with delays to ensure different timestamps
    ctx.merge_queue_store.enqueue(
        project_id,
        workspace_a_id,
        repo_a_id,
        "Merge branch-a".to_string(),
    );
    tokio::time::sleep(tokio::time::Duration::from_millis(15)).await;

    ctx.merge_queue_store.enqueue(
        project_id,
        workspace_b_id,
        repo_b_id,
        "Merge branch-b".to_string(),
    );
    tokio::time::sleep(tokio::time::Duration::from_millis(15)).await;

    ctx.merge_queue_store.enqueue(
        project_id,
        workspace_c_id,
        repo_c_id,
        "Merge branch-c".to_string(),
    );

    // Verify all 3 are queued
    assert_eq!(ctx.merge_queue_store.count_by_project(project_id), 3);

    // Verify FIFO ordering in list
    let queue_list = ctx.merge_queue_store.list_by_project(project_id);
    assert_eq!(queue_list[0].workspace_id, workspace_a_id);
    assert_eq!(queue_list[1].workspace_id, workspace_b_id);
    assert_eq!(queue_list[2].workspace_id, workspace_c_id);

    // Process the queue
    let processor = ctx.processor();
    processor
        .process_project_queue(project_id)
        .await
        .expect("Merge queue processing should succeed");

    // Assert: Queue is empty
    assert_eq!(ctx.merge_queue_store.count_by_project(project_id), 0);

    // Assert: All tasks are Done
    let task_a = Task::find_by_id(&ctx.pool, task_a_id)
        .await
        .expect("Query should succeed")
        .expect("Task A should exist");
    let task_b = Task::find_by_id(&ctx.pool, task_b_id)
        .await
        .expect("Query should succeed")
        .expect("Task B should exist");
    let task_c = Task::find_by_id(&ctx.pool, task_c_id)
        .await
        .expect("Query should succeed")
        .expect("Task C should exist");

    assert_eq!(task_a.status, TaskStatus::Done, "Task A should be Done");
    assert_eq!(task_b.status, TaskStatus::Done, "Task B should be Done");
    assert_eq!(task_c.status, TaskStatus::Done, "Task C should be Done");

    // Assert: Merge records created in A, B, C order (by created_at timestamp)
    let merges_a = Merge::find_by_workspace_id(&ctx.pool, workspace_a_id)
        .await
        .expect("Query should succeed");
    let merges_b = Merge::find_by_workspace_id(&ctx.pool, workspace_b_id)
        .await
        .expect("Query should succeed");
    let merges_c = Merge::find_by_workspace_id(&ctx.pool, workspace_c_id)
        .await
        .expect("Query should succeed");

    assert_eq!(merges_a.len(), 1, "Task A should have one merge record");
    assert_eq!(merges_b.len(), 1, "Task B should have one merge record");
    assert_eq!(merges_c.len(), 1, "Task C should have one merge record");

    // Verify timestamps follow FIFO order: A < B < C
    let merge_a = &merges_a[0];
    let merge_b = &merges_b[0];
    let merge_c = &merges_c[0];

    // Extract created_at from the Merge enum variants
    let created_at_a = match merge_a {
        Merge::Direct(d) => d.created_at,
        Merge::Pr(p) => p.created_at,
    };
    let created_at_b = match merge_b {
        Merge::Direct(d) => d.created_at,
        Merge::Pr(p) => p.created_at,
    };
    let created_at_c = match merge_c {
        Merge::Direct(d) => d.created_at,
        Merge::Pr(p) => p.created_at,
    };

    assert!(
        created_at_a < created_at_b,
        "Merge A should be created before Merge B"
    );
    assert!(
        created_at_b < created_at_c,
        "Merge B should be created before Merge C"
    );
}

/// Test that conflicts skip to the next task without blocking the queue.
///
/// Scenario:
/// 1. Enqueue A (will conflict), B (clean), C (clean)
/// 2. Create merge conflict for A's branch
/// 3. Process queue
/// 4. Assert: A removed (conflict), B merged successfully, C merged successfully
/// 5. Assert: A's task still InReview (not Done)
#[tokio::test]
async fn test_conflict_skips_to_next() {
    let ctx = TestContext::new().await;

    // Create 3 separate test repos for clean isolation
    let test_repo_a = TestRepo::new("conflict-repo-a");
    let test_repo_b = TestRepo::new("clean-repo-b");
    let test_repo_c = TestRepo::new("clean-repo-c");

    // Create project with 3 tasks
    let task_a_ctx = EntityGraphBuilder::new(ctx.pool.clone())
        .with_project("Conflict Skip Test")
        .create_task("Task A - will conflict", TaskStatus::InReview)
        .await
        .with_workspace("conflict-branch")
        .await;

    let project_id = task_a_ctx.project_id();
    let task_a_id = task_a_ctx.task_id();
    let workspace_a_id = task_a_ctx.workspace_id();

    let task_b_ctx = task_a_ctx
        .builder()
        .create_task("Task B - clean", TaskStatus::InReview)
        .await
        .with_workspace("clean-branch-b")
        .await;

    let task_b_id = task_b_ctx.task_id();
    let workspace_b_id = task_b_ctx.workspace_id();

    let task_c_ctx = task_b_ctx
        .builder()
        .create_task("Task C - clean", TaskStatus::InReview)
        .await
        .with_workspace("clean-branch-c")
        .await;

    let task_c_id = task_c_ctx.task_id();
    let workspace_c_id = task_c_ctx.workspace_id();

    // Create repos in database
    let repo_a_id = create_repo(&ctx.pool, &test_repo_a.path, &test_repo_a.name).await;
    let repo_b_id = create_repo(&ctx.pool, &test_repo_b.path, &test_repo_b.name).await;
    let repo_c_id = create_repo(&ctx.pool, &test_repo_c.path, &test_repo_c.name).await;

    // Create worktrees
    let worktree_a_path = test_repo_a.create_worktree("conflict-branch");
    let worktree_b_path = test_repo_b.create_worktree("clean-branch-b");
    let worktree_c_path = test_repo_c.create_worktree("clean-branch-c");

    // Set up workspace container refs
    update_workspace_container_ref(
        &ctx.pool,
        workspace_a_id,
        &worktree_a_path.parent().unwrap().to_string_lossy(),
    )
    .await;
    update_workspace_container_ref(
        &ctx.pool,
        workspace_b_id,
        &worktree_b_path.parent().unwrap().to_string_lossy(),
    )
    .await;
    update_workspace_container_ref(
        &ctx.pool,
        workspace_c_id,
        &worktree_c_path.parent().unwrap().to_string_lossy(),
    )
    .await;

    // Link workspaces to their repos
    create_workspace_repo(&ctx.pool, workspace_a_id, repo_a_id, "main").await;
    create_workspace_repo(&ctx.pool, workspace_b_id, repo_b_id, "main").await;
    create_workspace_repo(&ctx.pool, workspace_c_id, repo_c_id, "main").await;

    // Setup Task A: Create a conflict with main
    // 1. First commit on task branch
    add_and_commit(
        &worktree_a_path,
        "README.md",
        "# Test Repository\n\nConflicting content from task A",
        "Task A: Modify README",
    );

    // 2. Add a conflicting commit to main in repo A
    {
        let repo = Repository::open(&test_repo_a.path).expect("Failed to open repo");
        let main_ref = repo
            .find_branch("main", git2::BranchType::Local)
            .expect("main branch should exist");
        repo.set_head(main_ref.get().name().unwrap())
            .expect("set head to main");
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
            .expect("checkout main");

        let readme_path = test_repo_a.path.join("README.md");
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

    // Setup Task B and C: Clean changes that won't conflict
    add_and_commit(&worktree_b_path, "file_b.txt", "content B", "Task B: Add file");
    add_and_commit(&worktree_c_path, "file_c.txt", "content C", "Task C: Add file");

    // Enqueue all 3 tasks (A first with conflict, then B and C clean)
    ctx.merge_queue_store.enqueue(
        project_id,
        workspace_a_id,
        repo_a_id,
        "Merge conflict-branch".to_string(),
    );
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    ctx.merge_queue_store.enqueue(
        project_id,
        workspace_b_id,
        repo_b_id,
        "Merge clean-branch-b".to_string(),
    );
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    ctx.merge_queue_store.enqueue(
        project_id,
        workspace_c_id,
        repo_c_id,
        "Merge clean-branch-c".to_string(),
    );

    // Verify all 3 are queued
    assert_eq!(ctx.merge_queue_store.count_by_project(project_id), 3);

    // Process the queue - Task A should fail with conflict, B and C should succeed
    let processor = ctx.processor();
    processor
        .process_project_queue(project_id)
        .await
        .expect("Processing should complete without fatal error");

    // Assert: Queue is empty (all processed - either merged or removed due to conflict)
    assert_eq!(ctx.merge_queue_store.count_by_project(project_id), 0);

    // Assert: Task A stays InReview (conflict prevented merge)
    let task_a = Task::find_by_id(&ctx.pool, task_a_id)
        .await
        .expect("Query should succeed")
        .expect("Task A should exist");
    assert_eq!(
        task_a.status,
        TaskStatus::InReview,
        "Task A should stay InReview due to conflict"
    );

    // Assert: Task B is Done (clean merge succeeded)
    let task_b = Task::find_by_id(&ctx.pool, task_b_id)
        .await
        .expect("Query should succeed")
        .expect("Task B should exist");
    assert_eq!(
        task_b.status,
        TaskStatus::Done,
        "Task B should be Done (clean merge)"
    );

    // Assert: Task C is Done (clean merge succeeded)
    let task_c = Task::find_by_id(&ctx.pool, task_c_id)
        .await
        .expect("Query should succeed")
        .expect("Task C should exist");
    assert_eq!(
        task_c.status,
        TaskStatus::Done,
        "Task C should be Done (clean merge)"
    );

    // Assert: Only Task B and C have merge records
    let merges_a = Merge::find_by_workspace_id(&ctx.pool, workspace_a_id)
        .await
        .expect("Query should succeed");
    let merges_b = Merge::find_by_workspace_id(&ctx.pool, workspace_b_id)
        .await
        .expect("Query should succeed");
    let merges_c = Merge::find_by_workspace_id(&ctx.pool, workspace_c_id)
        .await
        .expect("Query should succeed");

    assert_eq!(
        merges_a.len(),
        0,
        "Task A should have no merge record (conflict)"
    );
    assert_eq!(
        merges_b.len(),
        1,
        "Task B should have one merge record"
    );
    assert_eq!(
        merges_c.len(),
        1,
        "Task C should have one merge record"
    );
}
