//! Autopilot handler for auto-queueing unblocked dependent tasks.
//!
//! When a task is marked as Done, this handler finds dependent tasks that
//! become unblocked and queues them for execution (if autopilot is enabled).
//! If a task doesn't have a workspace, one is automatically created using
//! the task group's base branch.

use async_trait::async_trait;
use db::models::{
    execution_queue::ExecutionQueue,
    project_repo::ProjectRepo,
    session::Session,
    task::{Task, TaskStatus},
    task_group::TaskGroup,
    workspace::{CreateWorkspace, Workspace},
    workspace_repo::{CreateWorkspaceRepo, WorkspaceRepo},
};
use executors::profile::ExecutorProfileId;
use tracing::{debug, error, info, warn};
use utils::text::{git_branch_id, short_uuid};
use uuid::Uuid;

use crate::services::{
    autopilot,
    domain_events::{
        DomainEvent, EventHandler, ExecutionMode, ExecutionTrigger, HandlerContext, HandlerError,
    },
};

/// Handler that auto-queues unblocked dependent tasks when autopilot is enabled.
pub struct AutopilotHandler;

impl AutopilotHandler {
    pub fn new() -> Self {
        Self
    }

    /// Get the executor profile ID from the latest session of a workspace.
    async fn get_executor_profile_for_workspace(
        &self,
        ctx: &HandlerContext,
        workspace_id: Uuid,
    ) -> Option<ExecutorProfileId> {
        let session = Session::find_latest_by_workspace_id(&ctx.db.pool, workspace_id)
            .await
            .ok()??;

        let executor_str = session.executor.as_ref()?;
        serde_json::from_str(executor_str).ok()
    }

    /// Generate a git branch name for a workspace.
    fn generate_branch_name(
        &self,
        workspace_id: &Uuid,
        task_title: &str,
        git_branch_prefix: &str,
    ) -> String {
        let task_title_id = git_branch_id(task_title);
        if git_branch_prefix.is_empty() {
            format!("{}-{}", short_uuid(workspace_id), task_title_id)
        } else {
            format!("{}/{}-{}", git_branch_prefix, short_uuid(workspace_id), task_title_id)
        }
    }

    /// Create a workspace for a task that doesn't have one.
    ///
    /// Returns None if:
    /// - Task has no task_group_id
    /// - Task group doesn't exist or has no base_branch
    /// - Project has no repos
    /// - Any database operation fails
    async fn create_workspace_for_task(
        &self,
        ctx: &HandlerContext,
        task: &Task,
    ) -> Option<Workspace> {
        // 1. Get task_group with base_branch
        let task_group_id = task.task_group_id?;
        let task_group = match TaskGroup::find_by_id(&ctx.db.pool, task_group_id).await {
            Ok(Some(group)) => group,
            Ok(None) => {
                warn!(
                    task_id = %task.id,
                    task_group_id = %task_group_id,
                    "Skipping auto-create workspace: task group not found"
                );
                return None;
            }
            Err(e) => {
                error!(
                    task_id = %task.id,
                    task_group_id = %task_group_id,
                    error = %e,
                    "Failed to fetch task group for workspace creation"
                );
                return None;
            }
        };

        let base_branch = match &task_group.base_branch {
            Some(branch) => branch.clone(),
            None => {
                warn!(
                    task_id = %task.id,
                    task_group_id = %task_group_id,
                    "Skipping auto-create workspace: task group has no base_branch"
                );
                return None;
            }
        };

        // 2. Get project repos
        let project = match task.parent_project(&ctx.db.pool).await {
            Ok(Some(p)) => p,
            Ok(None) => {
                warn!(
                    task_id = %task.id,
                    project_id = %task.project_id,
                    "Skipping auto-create workspace: project not found"
                );
                return None;
            }
            Err(e) => {
                error!(
                    task_id = %task.id,
                    project_id = %task.project_id,
                    error = %e,
                    "Failed to fetch project for workspace creation"
                );
                return None;
            }
        };

        let repos = match ProjectRepo::find_repos_for_project(&ctx.db.pool, project.id).await {
            Ok(r) => r,
            Err(e) => {
                error!(
                    task_id = %task.id,
                    project_id = %project.id,
                    error = %e,
                    "Failed to fetch repos for workspace creation"
                );
                return None;
            }
        };

        if repos.is_empty() {
            warn!(
                task_id = %task.id,
                project_id = %project.id,
                "Skipping auto-create workspace: project has no repos"
            );
            return None;
        }

        // 3. Generate branch name
        let workspace_id = Uuid::new_v4();
        let git_branch_prefix = ctx.config.read().await.git_branch_prefix.clone();
        let branch_name = self.generate_branch_name(&workspace_id, &task.title, &git_branch_prefix);

        // 4. Create workspace
        let workspace = match Workspace::create(
            &ctx.db.pool,
            &CreateWorkspace {
                branch: branch_name,
                agent_working_dir: project.default_agent_working_dir.clone(),
            },
            workspace_id,
            task.id,
        )
        .await
        {
            Ok(ws) => ws,
            Err(e) => {
                error!(
                    task_id = %task.id,
                    workspace_id = %workspace_id,
                    error = %e,
                    "Failed to create workspace"
                );
                return None;
            }
        };

        // 5. Create workspace repos
        let workspace_repos: Vec<CreateWorkspaceRepo> = repos
            .iter()
            .map(|r| CreateWorkspaceRepo {
                repo_id: r.id,
                target_branch: base_branch.clone(),
            })
            .collect();

        if let Err(e) = WorkspaceRepo::create_many(&ctx.db.pool, workspace.id, &workspace_repos).await {
            error!(
                task_id = %task.id,
                workspace_id = %workspace.id,
                error = %e,
                "Failed to create workspace repos"
            );
            // Workspace was created but repos failed - this is a partial state.
            // Still return the workspace since it exists, but log the error.
            // The workspace setup will fail anyway without repos.
            return None;
        }

        info!(
            task_id = %task.id,
            workspace_id = %workspace.id,
            branch = %workspace.branch,
            base_branch = %base_branch,
            "Auto-created workspace for task"
        );
        Some(workspace)
    }
}

impl Default for AutopilotHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventHandler for AutopilotHandler {
    fn name(&self) -> &'static str {
        "autopilot"
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Spawned
    }

    fn handles(&self, event: &DomainEvent) -> bool {
        matches!(
            event,
            DomainEvent::TaskStatusChanged { task, .. } if task.status == TaskStatus::Done
        )
    }

    async fn handle(&self, event: DomainEvent, ctx: &HandlerContext) -> Result<(), HandlerError> {
        let DomainEvent::TaskStatusChanged { task, .. } = event else {
            return Ok(());
        };

        let completed_task_id = task.id;

        // Check if autopilot is enabled
        let autopilot_enabled = ctx.config.read().await.autopilot_enabled;
        if !autopilot_enabled {
            debug!(
                task_id = %completed_task_id,
                "Autopilot disabled, skipping auto-dequeue of dependents"
            );
            return Ok(());
        }

        // Find unblocked dependent tasks
        let unblocked_tasks = autopilot::find_unblocked_dependents(&ctx.db.pool, completed_task_id)
            .await
            .map_err(HandlerError::Database)?;

        if unblocked_tasks.is_empty() {
            debug!(
                task_id = %completed_task_id,
                "No unblocked dependent tasks to auto-dequeue"
            );
            return Ok(());
        }

        info!(
            completed_task_id = %completed_task_id,
            unblocked_count = unblocked_tasks.len(),
            "Auto-dequeueing unblocked dependent tasks"
        );

        let mut enqueued_count = 0;

        // Get the default executor profile from config (used for new workspaces or as fallback)
        let default_executor_profile = ctx.config.read().await.executor_profile.clone();

        for unblocked_task in unblocked_tasks {
            // Find the latest workspace for this task, or create one if it doesn't exist
            let (workspace, is_new_workspace) =
                match Workspace::find_latest_by_task_id(&ctx.db.pool, unblocked_task.id).await {
                    Ok(Some(ws)) => (ws, false),
                    Ok(None) => {
                        // Try to auto-create a workspace
                        match self.create_workspace_for_task(ctx, &unblocked_task).await {
                            Some(ws) => (ws, true),
                            None => {
                                debug!(
                                    task_id = %unblocked_task.id,
                                    "Skipping auto-dequeue: could not create workspace for task"
                                );
                                continue;
                            }
                        }
                    }
                    Err(e) => {
                        error!(
                            task_id = %unblocked_task.id,
                            error = %e,
                            "Failed to find workspace for unblocked task"
                        );
                        continue;
                    }
                };

            // Get the executor profile: from last session if available, otherwise use default from config
            let executor_profile_id = if is_new_workspace {
                // New workspace has no session, use default executor from config
                default_executor_profile.clone()
            } else {
                // Existing workspace: try to get executor from last session, fallback to config default
                self.get_executor_profile_for_workspace(ctx, workspace.id)
                    .await
                    .unwrap_or_else(|| {
                        debug!(
                            task_id = %unblocked_task.id,
                            workspace_id = %workspace.id,
                            "No session found for workspace, using default executor from config"
                        );
                        default_executor_profile.clone()
                    })
            };

            // Create execution queue entry
            match ExecutionQueue::create(&ctx.db.pool, workspace.id, &executor_profile_id).await {
                Ok(_) => {
                    info!(
                        task_id = %unblocked_task.id,
                        workspace_id = %workspace.id,
                        executor = %executor_profile_id,
                        "Auto-dequeued unblocked dependent task"
                    );
                    enqueued_count += 1;
                }
                Err(e) => {
                    error!(
                        task_id = %unblocked_task.id,
                        workspace_id = %workspace.id,
                        error = %e,
                        "Failed to create execution queue entry for unblocked task"
                    );
                }
            }
        }

        if enqueued_count > 0 {
            info!(
                completed_task_id = %completed_task_id,
                enqueued_count = enqueued_count,
                "Auto-dequeued unblocked dependent tasks"
            );

            // Trigger queue processing to start the newly queued workspaces
            if let Some(ref trigger_callback) = ctx.execution_trigger {
                if let Err(e) = trigger_callback(ExecutionTrigger::ProcessQueue).await {
                    warn!(
                        error = %e,
                        "Failed to trigger queue processing after auto-dequeue"
                    );
                }
            }
        }

        Ok(())
    }
}
