use std::time::Duration;

use db::{
    DBService,
    models::{
        merge::{Merge, MergeStatus, PrMerge},
        task::{Task, TaskStatus},
        workspace::{Workspace, WorkspaceError},
    },
};
use serde_json::json;
use sqlx::error::Error as SqlxError;
use thiserror::Error;
use tokio::time::interval;
use tracing::{debug, error, info};

use crate::services::{
    analytics::AnalyticsContext,
    domain_events::{DomainEvent, EventDispatchCallback},
    github::{GitHubRepoInfo, GitHubService, GitHubServiceError},
    share::SharePublisher,
};

#[derive(Debug, Error)]
enum PrMonitorError {
    #[error(transparent)]
    GitHubServiceError(#[from] GitHubServiceError),
    #[error(transparent)]
    WorkspaceError(#[from] WorkspaceError),
    #[error(transparent)]
    Sqlx(#[from] SqlxError),
}

/// Service to monitor GitHub PRs and update task status when they are merged
pub struct PrMonitorService {
    db: DBService,
    poll_interval: Duration,
    analytics: Option<AnalyticsContext>,
    publisher: Option<SharePublisher>,
    event_dispatcher: Option<EventDispatchCallback>,
}

impl PrMonitorService {
    pub async fn spawn(
        db: DBService,
        analytics: Option<AnalyticsContext>,
        publisher: Option<SharePublisher>,
        event_dispatcher: Option<EventDispatchCallback>,
    ) -> tokio::task::JoinHandle<()> {
        let service = Self {
            db,
            poll_interval: Duration::from_secs(60), // Check every minute
            analytics,
            publisher,
            event_dispatcher,
        };
        tokio::spawn(async move {
            service.start().await;
        })
    }

    async fn start(&self) {
        info!(
            "Starting PR monitoring service with interval {:?}",
            self.poll_interval
        );

        let mut interval = interval(self.poll_interval);

        loop {
            interval.tick().await;
            if let Err(e) = self.check_all_open_prs().await {
                error!("Error checking open PRs: {}", e);
            }
        }
    }

    /// Check all open PRs for updates with the provided GitHub token
    async fn check_all_open_prs(&self) -> Result<(), PrMonitorError> {
        let open_prs = Merge::get_open_prs(&self.db.pool).await?;

        if open_prs.is_empty() {
            debug!("No open PRs to check");
            return Ok(());
        }

        info!("Checking {} open PRs", open_prs.len());

        for pr_merge in open_prs {
            if let Err(e) = self.check_pr_status(&pr_merge).await {
                error!(
                    "Error checking PR #{} for workspace {}: {}",
                    pr_merge.pr_info.number, pr_merge.workspace_id, e
                );
            }
        }
        Ok(())
    }

    /// Check the status of a specific PR
    async fn check_pr_status(&self, pr_merge: &PrMerge) -> Result<(), PrMonitorError> {
        // GitHubService now uses gh CLI, no token needed
        let github_service = GitHubService::new()?;
        let repo_info = GitHubRepoInfo::from_remote_url(&pr_merge.pr_info.url)?;

        let pr_status = github_service
            .update_pr_status(&repo_info, pr_merge.pr_info.number)
            .await?;

        debug!(
            "PR #{} status: {:?} (was open)",
            pr_merge.pr_info.number, pr_status.status
        );

        // Update the PR status in the database
        if !matches!(&pr_status.status, MergeStatus::Open) {
            // Update merge status with the latest information from GitHub
            Merge::update_status(
                &self.db.pool,
                pr_merge.id,
                pr_status.status.clone(),
                pr_status.merge_commit_sha,
            )
            .await?;

            if matches!(&pr_status.status, MergeStatus::Merged) {
                self.complete_merged_pr_task(pr_merge).await?;
            }
        }

        Ok(())
    }

    async fn complete_merged_pr_task(&self, pr_merge: &PrMerge) -> Result<(), PrMonitorError> {
        if let Some(workspace) = Workspace::find_by_id(&self.db.pool, pr_merge.workspace_id).await?
            && let Ok(Some(task)) = Task::find_by_id(&self.db.pool, workspace.task_id).await
        {
            info!(
                "PR #{} was merged, updating task {} to done",
                pr_merge.pr_info.number, workspace.task_id
            );

            let previous_status = task.status.clone();
            Task::update_status(&self.db.pool, workspace.task_id, TaskStatus::Done).await?;

            if let Some(dispatcher) = &self.event_dispatcher {
                let mut updated_task = task.clone();
                updated_task.status = TaskStatus::Done;
                dispatcher(DomainEvent::TaskStatusChanged {
                    task: updated_task,
                    previous_status,
                })
                .await;
            }

            if let Some(analytics) = &self.analytics {
                analytics.analytics_service.track_event(
                    &analytics.user_id,
                    "pr_merged",
                    Some(json!({
                        "task_id": workspace.task_id.to_string(),
                        "workspace_id": workspace.id.to_string(),
                        "project_id": task.project_id.to_string(),
                    })),
                );
            }

            if let Some(publisher) = &self.publisher
                && let Err(err) = publisher.update_shared_task_by_id(workspace.task_id).await
            {
                tracing::warn!(
                    ?err,
                    "Failed to propagate shared task update for {}",
                    workspace.task_id
                );
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use db::models::{
        project::{CreateProject, Project},
        repo::Repo,
        task::{CreateTask, Task},
        workspace::{CreateWorkspace, Workspace},
    };
    use tempfile::TempDir;
    use uuid::Uuid;

    use super::*;
    use crate::services::domain_events::{
        EventDispatchCallback, OrchestrationEventMapper, OrchestrationEventPublisher,
        OrchestrationEventType, RecordingOrchestrationEventPublisher, default_topic_namespace,
    };

    fn recording_dispatcher(
        db: &DBService,
        publisher: RecordingOrchestrationEventPublisher,
    ) -> EventDispatchCallback {
        let db = db.clone();
        Arc::new(move |event| {
            let db = db.clone();
            let publisher = publisher.clone();
            Box::pin(async move {
                let mapper = OrchestrationEventMapper::new(db.pool.clone());
                let envelopes = mapper
                    .map_event(&event)
                    .await
                    .expect("map orchestration event");
                for envelope in envelopes {
                    let event_name = serde_json::to_string(&envelope.event_type)
                        .expect("event type serialization cannot fail")
                        .trim_matches('"')
                        .to_string();
                    publisher
                        .publish(
                            format!("{}/{}", default_topic_namespace(), event_name),
                            envelope,
                        )
                        .await
                        .expect("publish orchestration event");
                }
            })
        })
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn pr_monitor_completion_emits_task_status_changed() {
        let _lock = crate::services::TEST_DB_LOCK.lock().expect("lock poisoned");
        let db = DBService::new().await.expect("db service");
        let publisher = RecordingOrchestrationEventPublisher::default();

        let project = Project::create(
            &db.pool,
            &CreateProject {
                name: "pr-monitor-events".to_string(),
                repositories: vec![],
            },
            Uuid::new_v4(),
        )
        .await
        .expect("create project");

        let task = Task::create(
            &db.pool,
            &CreateTask {
                project_id: project.id,
                title: "merge via pr".to_string(),
                description: None,
                status: Some(TaskStatus::InReview),
                parent_workspace_id: None,
                image_ids: None,
                shared_task_id: None,
                task_group_id: None,
            },
            Uuid::new_v4(),
        )
        .await
        .expect("create task");

        let workspace = Workspace::create(
            &db.pool,
            &CreateWorkspace {
                branch: "feature/pr".to_string(),
                agent_working_dir: None,
            },
            Uuid::new_v4(),
            task.id,
        )
        .await
        .expect("create workspace");

        let tempdir = TempDir::new().expect("tempdir");
        let repo_path = tempdir.path().join("repo");
        std::fs::create_dir_all(&repo_path).expect("create repo dir");
        let repo = Repo::find_or_create(&db.pool, &repo_path, "repo")
            .await
            .expect("create repo");
        let pr_merge = Merge::create_pr(
            &db.pool,
            workspace.id,
            repo.id,
            "main",
            42,
            "https://github.com/acme/repo/pull/42",
        )
        .await
        .expect("create pr merge");

        let service = PrMonitorService {
            db: db.clone(),
            poll_interval: Duration::from_secs(60),
            analytics: None,
            publisher: None,
            event_dispatcher: Some(recording_dispatcher(&db, publisher.clone())),
        };

        service
            .complete_merged_pr_task(&pr_merge)
            .await
            .expect("complete merged pr task");

        let updated_task = Task::find_by_id(&db.pool, task.id)
            .await
            .expect("load task")
            .expect("task exists");
        assert_eq!(updated_task.status, TaskStatus::Done);

        let event_types = publisher
            .published()
            .into_iter()
            .map(|(_, envelope)| envelope.event_type)
            .collect::<Vec<_>>();
        assert_eq!(event_types, vec![OrchestrationEventType::TaskStatusChanged]);
    }
}
