use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use anyhow::{Context, Result};
use db::{
    init_sqlite_vec,
    models::{
        execution_process::{
            CreateExecutionProcess, ExecutionProcess, ExecutionProcessRunReason,
            ExecutionProcessStatus,
        },
        project::{CreateProject, Project},
        session::{CreateSession, Session},
        tag::{CreateTag, Tag},
        task::{CreateTask, Task, TaskStatus},
        task_dependency::TaskDependency,
        task_group::TaskGroup,
        workspace::{CreateWorkspace, Workspace},
    },
};
use executors::actions::{
    script::{ScriptContext, ScriptRequest, ScriptRequestLanguage},
    ExecutorAction, ExecutorActionType,
};
use fake::{Fake, faker::lorem::en::{Paragraph, Sentence}};
use rand::{Rng, seq::SliceRandom};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use uuid::Uuid;

const PROJECT_NAMES: &[&str] = &[
    "E-commerce Platform",
    "AI Task Manager",
    "Mobile Banking App",
];

const TASK_GROUP_NAMES: &[(&str, &str)] = &[
    ("Sprint 1", "Sprint 2"),
    ("Authentication Feature", "Payments Feature"),
    ("Discovery", "Polish"),
];

const TAGS: &[(&str, &str)] = &[
    ("urgent", "Requires immediate attention"),
    ("bug", "Unexpected behavior or defect"),
    ("feature", "New product functionality"),
];

const TASK_TEMPLATES: &[(&str, &str)] = &[
    ("Implement OAuth login flow", "Add Google & GitHub authentication"),
    ("Design user registration form", "Create responsive signup UI"),
    ("Add password reset functionality", "Email-based password recovery"),
    ("Create admin dashboard", "Analytics and user management views"),
    ("Implement search functionality", "Full-text search with filters"),
    ("Add file upload feature", "Support images and documents up to 10MB"),
    ("Set up CI/CD pipeline", "GitHub Actions for automated testing"),
    ("Implement rate limiting", "API throttling to prevent abuse"),
    ("Add notification system", "Real-time alerts via WebSocket"),
    ("Create API documentation", "OpenAPI spec with examples"),
    ("Refactor billing module", "Simplify invoice generation logic"),
    ("Improve onboarding flow", "Guided tour with contextual hints"),
    ("Build audit log view", "Track admin actions and export CSV"),
    ("Optimize database queries", "Reduce dashboard load time"),
    ("Add multi-factor authentication", "SMS and authenticator support"),
    ("Implement dark mode toggle", "Persist user theme preference"),
    ("Create localization framework", "Support English and Spanish"),
    ("Add team permissions", "Role-based access control"),
    ("Set up feature flags", "Gradual rollout controls"),
    ("Implement subscription plans", "Monthly and annual billing tiers"),
    ("Design landing page", "Hero section with value props"),
    ("Create reporting exports", "CSV and PDF generation"),
    ("Integrate payment gateway", "Stripe checkout and webhooks"),
    ("Add in-app feedback widget", "Capture user sentiment"),
];

#[tokio::main]
async fn main() -> Result<()> {
    let db_path = PathBuf::from("dev_assets_seed/dev.db");
    reset_database_file(&db_path)?;

    let pool = build_pool(&db_path).await?;
    seed_data(&pool).await?;

    Ok(())
}

fn reset_database_file(db_path: &Path) -> Result<()> {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }

    if db_path.exists() {
        fs::remove_file(db_path)
            .with_context(|| format!("Failed to remove {}", db_path.display()))?;
    }

    Ok(())
}

async fn build_pool(db_path: &Path) -> Result<SqlitePool> {
    init_sqlite_vec();

    let database_url = format!("sqlite://{}", db_path.display());
    let connect_options = SqliteConnectOptions::from_str(&database_url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(30))
        .synchronous(SqliteSynchronous::Normal);

    let pool = SqlitePoolOptions::new()
        .max_connections(20)
        .min_connections(1)
        .idle_timeout(Duration::from_secs(300))
        .acquire_timeout(Duration::from_secs(30))
        .connect_with(connect_options)
        .await?;

    sqlx::migrate!("../db/migrations").run(&pool).await?;
    sqlx::query("PRAGMA optimize").execute(&pool).await?;

    Ok(pool)
}

async fn seed_data(pool: &SqlitePool) -> Result<()> {
    let tags = create_tags(pool).await?;
    let projects = create_projects(pool).await?;
    let task_groups = create_task_groups(pool, &projects).await?;

    let tasks = create_tasks(pool, &projects, &task_groups).await?;
    let dependencies = create_dependencies(pool, &tasks, 5..=8).await?;

    let workspaces = create_workspaces_with_sessions(pool, &tasks, 2).await?;
    let execution_processes = create_execution_processes(pool, &workspaces, 3).await?;

    for task in &tasks {
        Task::update_materialized_status(pool, task.id).await?;
    }

    let mut todo = 0;
    let mut inprogress = 0;
    let mut inreview = 0;
    let mut done = 0;
    let mut cancelled = 0;

    for task in &tasks {
        match task.status {
            TaskStatus::Todo => todo += 1,
            TaskStatus::InProgress => inprogress += 1,
            TaskStatus::InReview => inreview += 1,
            TaskStatus::Done => done += 1,
            TaskStatus::Cancelled => cancelled += 1,
        }
    }

    println!("Seed database created at dev_assets_seed/dev.db");
    println!("Projects: {}", projects.len());
    println!("Task groups: {}", task_groups.len());
    println!(
        "Tasks: {} (todo {}, inprogress {}, inreview {}, done {}, cancelled {})",
        tasks.len(),
        todo,
        inprogress,
        inreview,
        done,
        cancelled,
    );
    println!("Dependencies: {}", dependencies);
    println!("Workspaces: {}", workspaces.len());
    println!("Execution processes: {}", execution_processes);
    println!("Tags: {}", tags.len());

    Ok(())
}

async fn create_tags(pool: &SqlitePool) -> Result<Vec<Tag>> {
    let mut tags = Vec::new();
    for (name, content) in TAGS {
        let tag = Tag::create(
            pool,
            &CreateTag {
                tag_name: (*name).to_string(),
                content: (*content).to_string(),
            },
        )
        .await?;
        tags.push(tag);
    }
    Ok(tags)
}

async fn create_projects(pool: &SqlitePool) -> Result<Vec<Project>> {
    let mut projects = Vec::new();
    for name in PROJECT_NAMES {
        let project = Project::create(
            pool,
            &CreateProject {
                name: (*name).to_string(),
                repositories: Vec::new(),
            },
            Uuid::new_v4(),
        )
        .await?;
        projects.push(project);
    }
    Ok(projects)
}

async fn create_task_groups(
    pool: &SqlitePool,
    projects: &[Project],
) -> Result<Vec<TaskGroup>> {
    let mut groups = Vec::new();
    for (index, project) in projects.iter().enumerate() {
        let name_pair = TASK_GROUP_NAMES
            .get(index)
            .unwrap_or(&("Sprint 1", "Sprint 2"));
        let first = TaskGroup::create(pool, project.id, name_pair.0.to_string(), None).await?;
        let second = TaskGroup::create(pool, project.id, name_pair.1.to_string(), None).await?;
        groups.push(first);
        groups.push(second);
    }
    Ok(groups)
}

async fn create_tasks(
    pool: &SqlitePool,
    projects: &[Project],
    task_groups: &[TaskGroup],
) -> Result<Vec<Task>> {
    let mut rng = rand::thread_rng();
    let mut tasks = Vec::new();
    let mut status_buckets = Vec::new();

    status_buckets.extend(std::iter::repeat(TaskStatus::Todo).take(15));
    status_buckets.extend(std::iter::repeat(TaskStatus::InProgress).take(8));
    status_buckets.extend(std::iter::repeat(TaskStatus::InReview).take(6));
    status_buckets.extend(std::iter::repeat(TaskStatus::Done).take(18));
    status_buckets.extend(std::iter::repeat(TaskStatus::Cancelled).take(3));
    status_buckets.shuffle(&mut rng);

    let templates: Vec<(&str, &str)> = TASK_TEMPLATES.to_vec();

    for (idx, status) in status_buckets.into_iter().enumerate() {
        let project = &projects[idx % projects.len()];
        let group_choices: Vec<&TaskGroup> = task_groups
            .iter()
            .filter(|group| group.project_id == project.id)
            .collect();
        let group = group_choices
            .choose(&mut rng)
            .context("No task groups available for project")?;

        let (title, description) = if idx < templates.len() {
            (templates[idx].0.to_string(), templates[idx].1.to_string())
        } else {
            let title: String = Sentence(3..6).fake();
            let description: String = Paragraph(1..2).fake();
            (title, description)
        };

        let task = Task::create(
            pool,
            &CreateTask {
                project_id: project.id,
                title,
                description: Some(description),
                status: Some(status),
                parent_workspace_id: None,
                image_ids: None,
                shared_task_id: None,
                task_group_id: Some(group.id),
            },
            Uuid::new_v4(),
        )
        .await?;
        tasks.push(task);
    }

    Ok(tasks)
}

async fn create_dependencies(
    pool: &SqlitePool,
    tasks: &[Task],
    range: std::ops::RangeInclusive<usize>,
) -> Result<usize> {
    let mut rng = rand::thread_rng();
    let target = rng.gen_range(range);
    let mut created = HashSet::new();
    let mut attempts = 0;

    let mut tasks_by_project: HashMap<Uuid, Vec<&Task>> = HashMap::new();
    for task in tasks {
        tasks_by_project.entry(task.project_id).or_default().push(task);
    }

    while created.len() < target && attempts < 200 {
        attempts += 1;

        let project_tasks = tasks_by_project
            .values()
            .filter(|items| items.len() > 1)
            .collect::<Vec<_>>();
        let Some(project_tasks) = project_tasks.choose(&mut rng) else {
            break;
        };

        let blocked_task = project_tasks.choose(&mut rng).context("Missing task")?;
        let candidate_dependencies: Vec<&Task> = project_tasks
            .iter()
            .copied()
            .filter(|task| task.id != blocked_task.id && task.status != TaskStatus::Done)
            .collect();
        let Some(depends_on) = candidate_dependencies.choose(&mut rng) else {
            continue;
        };

        let pair = (blocked_task.id, depends_on.id);
        if created.contains(&pair) {
            continue;
        }

        if TaskDependency::create(pool, blocked_task.id, depends_on.id)
            .await
            .is_ok()
        {
            created.insert(pair);
        }
    }

    Ok(created.len())
}

async fn create_workspaces_with_sessions(
    pool: &SqlitePool,
    tasks: &[Task],
    count: usize,
) -> Result<Vec<Workspace>> {
    let in_progress: Vec<&Task> = tasks
        .iter()
        .filter(|task| task.status == TaskStatus::InProgress)
        .collect();
    let mut workspaces = Vec::new();

    for task in in_progress.into_iter().take(count) {
        let branch = format!("feature/{}", slugify(&task.title));
        let workspace = Workspace::create(
            pool,
            &CreateWorkspace {
                branch,
                agent_working_dir: None,
            },
            Uuid::new_v4(),
            task.id,
        )
        .await?;
        Session::create(
            pool,
            &CreateSession {
                executor: Some("claude".to_string()),
            },
            Uuid::new_v4(),
            workspace.id,
        )
        .await?;
        workspaces.push(workspace);
    }

    Ok(workspaces)
}

async fn create_execution_processes(
    pool: &SqlitePool,
    workspaces: &[Workspace],
    count: usize,
) -> Result<usize> {
    if workspaces.is_empty() {
        return Ok(0);
    }

    let mut created = 0;

    for index in 0..count {
        let workspace = workspaces
            .get(index % workspaces.len())
            .context("Workspace not found")?;
        let sessions = Session::find_by_workspace_id(pool, workspace.id).await?;
        let session = sessions
            .first()
            .context("Session not found for workspace")?;

        let action = ExecutorAction::new(
            ExecutorActionType::ScriptRequest(ScriptRequest {
                script: "pnpm install".to_string(),
                language: ScriptRequestLanguage::Bash,
                context: ScriptContext::SetupScript,
                working_dir: None,
            }),
            None,
        );

        let process = ExecutionProcess::create(
            pool,
            &CreateExecutionProcess {
                session_id: session.id,
                executor_action: action,
                run_reason: ExecutionProcessRunReason::SetupScript,
            },
            Uuid::new_v4(),
            &[],
        )
        .await?;

        ExecutionProcess::update_completion(
            pool,
            process.id,
            ExecutionProcessStatus::Completed,
            Some(0),
        )
        .await?;

        created += 1;
    }

    Ok(created)
}

fn slugify(value: &str) -> String {
    let mut output = String::new();
    let mut previous_dash = false;

    for ch in value.chars() {
        let normalized = if ch.is_ascii_alphanumeric() {
            Some(ch.to_ascii_lowercase())
        } else if ch == ' ' || ch == '-' || ch == '_' {
            Some('-')
        } else {
            None
        };

        match normalized {
            Some('-') => {
                if !output.is_empty() && !previous_dash {
                    output.push('-');
                }
                previous_dash = true;
            }
            Some(c) => {
                output.push(c);
                previous_dash = false;
            }
            None => {}
        }
    }

    if output.ends_with('-') {
        output.pop();
    }

    if output.is_empty() {
        "task".to_string()
    } else {
        output
    }
}
