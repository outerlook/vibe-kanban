use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool, Type};
use strum_macros::{Display, EnumString};
use ts_rs::TS;
use uuid::Uuid;

use super::{task::Task, task_group::TaskGroup};

#[derive(
    Debug, Clone, Copy, Type, Serialize, Deserialize, PartialEq, Eq, TS, EnumString, Display,
)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum WorkflowAssociationScope {
    RepositoryDefault,
    TaskGroupDefault,
    TaskOverride,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct WorkflowAssociation {
    pub id: Uuid,
    pub project_id: Option<Uuid>,
    pub task_group_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
    pub scope: WorkflowAssociationScope,
    pub workflow_id: String,
    pub label: String,
    pub url: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct UpsertWorkflowAssociation {
    pub workflow_id: String,
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ResolvedWorkflowAssociation {
    pub scope: WorkflowAssociationScope,
    pub workflow_id: String,
    pub label: String,
    pub url: String,
    pub is_effective: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct WorkflowAssociationResolution {
    pub effective: Option<ResolvedWorkflowAssociation>,
    pub associations: Vec<ResolvedWorkflowAssociation>,
}

impl WorkflowAssociation {
    pub async fn find_by_project_id(
        pool: &SqlitePool,
        project_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, WorkflowAssociation>(
            r#"SELECT id, project_id, task_group_id, task_id, scope, workflow_id, label, url, created_at, updated_at
               FROM workflow_associations
               WHERE project_id = ?1"#,
        )
        .bind(project_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_task_group_id(
        pool: &SqlitePool,
        task_group_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, WorkflowAssociation>(
            r#"SELECT id, project_id, task_group_id, task_id, scope, workflow_id, label, url, created_at, updated_at
               FROM workflow_associations
               WHERE task_group_id = ?1"#,
        )
        .bind(task_group_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_task_id(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, WorkflowAssociation>(
            r#"SELECT id, project_id, task_group_id, task_id, scope, workflow_id, label, url, created_at, updated_at
               FROM workflow_associations
               WHERE task_id = ?1"#,
        )
        .bind(task_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn upsert_for_project(
        pool: &SqlitePool,
        project_id: Uuid,
        payload: &UpsertWorkflowAssociation,
    ) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();
        sqlx::query_as::<_, WorkflowAssociation>(
            r#"INSERT INTO workflow_associations (
                    id,
                    project_id,
                    task_group_id,
                    task_id,
                    scope,
                    workflow_id,
                    label,
                    url
                ) VALUES (
                    ?1, ?2, NULL, NULL, 'repository_default', ?3, ?4, ?5
                )
                ON CONFLICT(project_id) DO UPDATE SET
                    workflow_id = excluded.workflow_id,
                    label = excluded.label,
                    url = excluded.url,
                    updated_at = datetime('now', 'subsec')
                RETURNING id, project_id, task_group_id, task_id, scope, workflow_id, label, url, created_at, updated_at"#,
        )
        .bind(id)
        .bind(project_id)
        .bind(&payload.workflow_id)
        .bind(&payload.label)
        .bind(&payload.url)
        .fetch_one(pool)
        .await
    }

    pub async fn upsert_for_task_group(
        pool: &SqlitePool,
        task_group_id: Uuid,
        payload: &UpsertWorkflowAssociation,
    ) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();
        sqlx::query_as::<_, WorkflowAssociation>(
            r#"INSERT INTO workflow_associations (
                    id,
                    project_id,
                    task_group_id,
                    task_id,
                    scope,
                    workflow_id,
                    label,
                    url
                ) VALUES (
                    ?1, NULL, ?2, NULL, 'task_group_default', ?3, ?4, ?5
                )
                ON CONFLICT(task_group_id) DO UPDATE SET
                    workflow_id = excluded.workflow_id,
                    label = excluded.label,
                    url = excluded.url,
                    updated_at = datetime('now', 'subsec')
                RETURNING id, project_id, task_group_id, task_id, scope, workflow_id, label, url, created_at, updated_at"#,
        )
        .bind(id)
        .bind(task_group_id)
        .bind(&payload.workflow_id)
        .bind(&payload.label)
        .bind(&payload.url)
        .fetch_one(pool)
        .await
    }

    pub async fn upsert_for_task(
        pool: &SqlitePool,
        task_id: Uuid,
        payload: &UpsertWorkflowAssociation,
    ) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();
        sqlx::query_as::<_, WorkflowAssociation>(
            r#"INSERT INTO workflow_associations (
                    id,
                    project_id,
                    task_group_id,
                    task_id,
                    scope,
                    workflow_id,
                    label,
                    url
                ) VALUES (
                    ?1, NULL, NULL, ?2, 'task_override', ?3, ?4, ?5
                )
                ON CONFLICT(task_id) DO UPDATE SET
                    workflow_id = excluded.workflow_id,
                    label = excluded.label,
                    url = excluded.url,
                    updated_at = datetime('now', 'subsec')
                RETURNING id, project_id, task_group_id, task_id, scope, workflow_id, label, url, created_at, updated_at"#,
        )
        .bind(id)
        .bind(task_id)
        .bind(&payload.workflow_id)
        .bind(&payload.label)
        .bind(&payload.url)
        .fetch_one(pool)
        .await
    }

    pub async fn delete_for_project(
        pool: &SqlitePool,
        project_id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM workflow_associations WHERE project_id = ?1")
            .bind(project_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn delete_for_task_group(
        pool: &SqlitePool,
        task_group_id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM workflow_associations WHERE task_group_id = ?1")
            .bind(task_group_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn delete_for_task(pool: &SqlitePool, task_id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM workflow_associations WHERE task_id = ?1")
            .bind(task_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn resolve_for_task(
        pool: &SqlitePool,
        task: &Task,
    ) -> Result<WorkflowAssociationResolution, sqlx::Error> {
        let mut associations = Vec::new();

        if let Some(association) = Self::find_by_task_id(pool, task.id).await? {
            associations.push(association);
        }

        if let Some(task_group_id) = task.task_group_id
            && let Some(association) = Self::find_by_task_group_id(pool, task_group_id).await?
        {
            associations.push(association);
        }

        if let Some(association) = Self::find_by_project_id(pool, task.project_id).await? {
            associations.push(association);
        }

        Ok(build_resolution(associations))
    }

    pub async fn resolve_for_task_group(
        pool: &SqlitePool,
        task_group: &TaskGroup,
    ) -> Result<WorkflowAssociationResolution, sqlx::Error> {
        let mut associations = Vec::new();

        if let Some(association) = Self::find_by_task_group_id(pool, task_group.id).await? {
            associations.push(association);
        }

        if let Some(association) = Self::find_by_project_id(pool, task_group.project_id).await? {
            associations.push(association);
        }

        Ok(build_resolution(associations))
    }
}

fn build_resolution(associations: Vec<WorkflowAssociation>) -> WorkflowAssociationResolution {
    let associations = associations
        .into_iter()
        .enumerate()
        .map(|(index, association)| ResolvedWorkflowAssociation {
            scope: association.scope,
            workflow_id: association.workflow_id,
            label: association.label,
            url: association.url,
            is_effective: index == 0,
        })
        .collect::<Vec<_>>();

    WorkflowAssociationResolution {
        effective: associations
            .iter()
            .find(|association| association.is_effective)
            .cloned(),
        associations,
    }
}

#[cfg(test)]
mod tests {
    use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

    use super::*;
    use crate::models::{
        project::{CreateProject, Project},
        task::{CreateTask, TaskStatus},
        task_group::TaskGroup,
    };

    async fn setup_pool() -> SqlitePool {
        crate::init_sqlite_vec();

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

    async fn seed_task_scope(pool: &SqlitePool) -> (Uuid, Uuid, Uuid) {
        let project = Project::create(
            pool,
            &CreateProject {
                name: "workflow-association-test".to_string(),
                repositories: Vec::new(),
            },
            Uuid::new_v4(),
        )
        .await
        .unwrap();

        let group = TaskGroup::create(pool, project.id, "Orchestration".to_string(), None, None)
            .await
            .unwrap();

        let task = Task::create(
            pool,
            &CreateTask {
                project_id: project.id,
                title: "Resolve workflow".to_string(),
                description: None,
                status: Some(TaskStatus::Todo),
                parent_workspace_id: None,
                image_ids: None,
                shared_task_id: None,
                task_group_id: Some(group.id),
            },
            Uuid::new_v4(),
        )
        .await
        .unwrap();

        (project.id, group.id, task.id)
    }

    #[tokio::test]
    async fn persists_associations_per_scope() {
        let pool = setup_pool().await;
        let (project_id, group_id, task_id) = seed_task_scope(&pool).await;

        let project_binding = WorkflowAssociation::upsert_for_project(
            &pool,
            project_id,
            &UpsertWorkflowAssociation {
                workflow_id: "wf-repo".to_string(),
                label: "Repo default".to_string(),
                url: "https://workflows.example/repository-default".to_string(),
            },
        )
        .await
        .unwrap();
        let group_binding = WorkflowAssociation::upsert_for_task_group(
            &pool,
            group_id,
            &UpsertWorkflowAssociation {
                workflow_id: "wf-group".to_string(),
                label: "Group default".to_string(),
                url: "https://workflows.example/task-group-default".to_string(),
            },
        )
        .await
        .unwrap();
        let task_binding = WorkflowAssociation::upsert_for_task(
            &pool,
            task_id,
            &UpsertWorkflowAssociation {
                workflow_id: "wf-task".to_string(),
                label: "Task override".to_string(),
                url: "https://workflows.example/task-override".to_string(),
            },
        )
        .await
        .unwrap();

        assert_eq!(
            project_binding.scope,
            WorkflowAssociationScope::RepositoryDefault
        );
        assert_eq!(
            group_binding.scope,
            WorkflowAssociationScope::TaskGroupDefault
        );
        assert_eq!(task_binding.scope, WorkflowAssociationScope::TaskOverride);
        assert_eq!(
            WorkflowAssociation::find_by_project_id(&pool, project_id)
                .await
                .unwrap()
                .unwrap()
                .workflow_id,
            "wf-repo"
        );
        assert_eq!(
            WorkflowAssociation::find_by_task_group_id(&pool, group_id)
                .await
                .unwrap()
                .unwrap()
                .workflow_id,
            "wf-group"
        );
        assert_eq!(
            WorkflowAssociation::find_by_task_id(&pool, task_id)
                .await
                .unwrap()
                .unwrap()
                .workflow_id,
            "wf-task"
        );
    }

    #[tokio::test]
    async fn resolves_precedence_task_then_group_then_repository() {
        let pool = setup_pool().await;
        let (project_id, group_id, task_id) = seed_task_scope(&pool).await;

        WorkflowAssociation::upsert_for_project(
            &pool,
            project_id,
            &UpsertWorkflowAssociation {
                workflow_id: "wf-repo".to_string(),
                label: "Repo default".to_string(),
                url: "https://workflows.example/repository-default".to_string(),
            },
        )
        .await
        .unwrap();
        WorkflowAssociation::upsert_for_task_group(
            &pool,
            group_id,
            &UpsertWorkflowAssociation {
                workflow_id: "wf-group".to_string(),
                label: "Group default".to_string(),
                url: "https://workflows.example/task-group-default".to_string(),
            },
        )
        .await
        .unwrap();
        WorkflowAssociation::upsert_for_task(
            &pool,
            task_id,
            &UpsertWorkflowAssociation {
                workflow_id: "wf-task".to_string(),
                label: "Task override".to_string(),
                url: "https://workflows.example/task-override".to_string(),
            },
        )
        .await
        .unwrap();

        let task = Task::find_by_id(&pool, task_id).await.unwrap().unwrap();
        let resolution = WorkflowAssociation::resolve_for_task(&pool, &task)
            .await
            .unwrap();

        assert_eq!(resolution.effective.unwrap().workflow_id, "wf-task");
        assert_eq!(
            resolution
                .associations
                .iter()
                .map(|association| association.scope)
                .collect::<Vec<_>>(),
            vec![
                WorkflowAssociationScope::TaskOverride,
                WorkflowAssociationScope::TaskGroupDefault,
                WorkflowAssociationScope::RepositoryDefault,
            ]
        );

        WorkflowAssociation::delete_for_task(&pool, task_id)
            .await
            .unwrap();
        let task = Task::find_by_id(&pool, task_id).await.unwrap().unwrap();
        let resolution = WorkflowAssociation::resolve_for_task(&pool, &task)
            .await
            .unwrap();
        assert_eq!(resolution.effective.unwrap().workflow_id, "wf-group");

        WorkflowAssociation::delete_for_task_group(&pool, group_id)
            .await
            .unwrap();
        let task = Task::find_by_id(&pool, task_id).await.unwrap().unwrap();
        let resolution = WorkflowAssociation::resolve_for_task(&pool, &task)
            .await
            .unwrap();
        assert_eq!(resolution.effective.unwrap().workflow_id, "wf-repo");
    }
}
