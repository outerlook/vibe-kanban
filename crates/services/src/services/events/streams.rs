use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use db::models::{
    execution_process::ExecutionProcess,
    gantt::GanttTask,
    notification::Notification,
    project::Project,
    scratch::Scratch,
    session::Session,
    task::{Task, TaskWithAttemptStatus},
    workspace::{Workspace, WorkspaceWithSession},
};
use futures::StreamExt;
use moka::future::Cache;
use once_cell::sync::Lazy;
use serde_json::json;
use sqlx::SqlitePool;
use tokio_stream::wrappers::{BroadcastStream, errors::BroadcastStreamRecvError};
use utils::log_msg::LogMsg;
use uuid::Uuid;

use super::{
    EventService,
    patches::execution_process_patch,
    types::{EventError, EventPatch, RecordTypes},
};

static TASK_PROJECT_CACHE: Lazy<Cache<Uuid, Uuid>> = Lazy::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(300))
        .max_capacity(1000)
        .build()
});
static TASK_PROJECT_CACHE_HITS: AtomicU64 = AtomicU64::new(0);
static TASK_PROJECT_CACHE_MISSES: AtomicU64 = AtomicU64::new(0);
static TASK_PROJECT_CACHE_LOOKUPS: AtomicU64 = AtomicU64::new(0);

async fn cache_project_for_task(task_id: Uuid, project_id: Uuid) {
    TASK_PROJECT_CACHE.insert(task_id, project_id).await;
}

async fn invalidate_task_cache(task_id: Uuid) {
    TASK_PROJECT_CACHE.invalidate(&task_id).await;
}

async fn get_project_for_task(db: &SqlitePool, task_id: Uuid) -> Option<Uuid> {
    if let Some(project_id) = TASK_PROJECT_CACHE.get(&task_id).await {
        record_task_cache_result(true);
        return Some(project_id);
    }

    record_task_cache_result(false);
    if let Ok(Some(task)) = Task::find_by_id(db, task_id).await {
        let project_id = task.project_id;
        TASK_PROJECT_CACHE.insert(task_id, project_id).await;
        return Some(project_id);
    }

    None
}

fn task_id_from_path(path: &str) -> Option<Uuid> {
    let suffix = path.strip_prefix("/tasks/")?;
    let id_str = suffix.split('/').next()?;
    Uuid::parse_str(id_str).ok()
}

fn record_task_cache_result(hit: bool) {
    if hit {
        TASK_PROJECT_CACHE_HITS.fetch_add(1, Ordering::Relaxed);
    } else {
        TASK_PROJECT_CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
    }

    let lookups = TASK_PROJECT_CACHE_LOOKUPS.fetch_add(1, Ordering::Relaxed) + 1;
    if lookups.is_multiple_of(1000) {
        let hits = TASK_PROJECT_CACHE_HITS.load(Ordering::Relaxed);
        let misses = TASK_PROJECT_CACHE_MISSES.load(Ordering::Relaxed);
        let total = hits + misses;
        if total > 0 {
            let hit_rate = hits as f64 / total as f64;
            tracing::debug!(
                hits = hits,
                misses = misses,
                hit_rate = hit_rate,
                "task project cache stats"
            );
        }
    }
}

/// Look up task_id from session_id via session -> workspace -> task
async fn get_task_id_for_execution_process(
    db: &SqlitePool,
    session_id: Option<Uuid>,
) -> Option<Uuid> {
    let session_id = session_id?;
    if let Ok(Some(session)) = Session::find_by_id(db, session_id).await
        && let Ok(Some(workspace)) = Workspace::find_by_id(db, session.workspace_id).await
    {
        return Some(workspace.task_id);
    }
    None
}

impl EventService {
    /// Stream raw task messages for a specific project with optional snapshot
    pub async fn stream_tasks_raw(
        &self,
        project_id: Uuid,
        include_snapshot: bool,
    ) -> Result<futures::stream::BoxStream<'static, Result<LogMsg, std::io::Error>>, EventError>
    {
        async fn build_tasks_snapshot(
            db_pool: &SqlitePool,
            project_id: Uuid,
        ) -> Result<LogMsg, sqlx::Error> {
            let tasks =
                Task::find_by_project_id_with_attempt_status(db_pool, project_id).await?;

            for task in &tasks {
                cache_project_for_task(task.id, task.project_id).await;
            }

            let tasks_map: serde_json::Map<String, serde_json::Value> = tasks
                .into_iter()
                .map(|task| (task.id.to_string(), serde_json::to_value(task).unwrap()))
                .collect();

            let patch = json!([{
                "op": "replace",
                "path": "/tasks",
                "value": tasks_map
            }]);

            Ok(LogMsg::JsonPatch(serde_json::from_value(patch).unwrap()))
        }

        // Clone necessary data for the async filter
        let db_pool = self.db.pool.clone();

        // Get filtered event stream
        let filtered_stream =
            BroadcastStream::new(self.msg_store.get_receiver()).filter_map(move |msg_result| {
                let db_pool = db_pool.clone();
                async move {
                    match msg_result {
                        Ok(LogMsg::JsonPatch(patch)) => {
                            // Filter events based on project_id
                            if let Some(patch_op) = patch.0.first() {
                                // Check if this is a direct task patch (new format)
                                if patch_op.path().starts_with("/tasks/") {
                                    match patch_op {
                                        json_patch::PatchOperation::Add(op) => {
                                            // Parse task data directly from value
                                            if let Ok(task) =
                                                serde_json::from_value::<TaskWithAttemptStatus>(
                                                    op.value.clone(),
                                                )
                                            {
                                                cache_project_for_task(task.id, task.project_id)
                                                    .await;
                                                if task.project_id == project_id {
                                                    return Some(Ok(LogMsg::JsonPatch(patch)));
                                                }
                                            }
                                        }
                                        json_patch::PatchOperation::Replace(op) => {
                                            // Parse task data directly from value
                                            if let Ok(task) =
                                                serde_json::from_value::<TaskWithAttemptStatus>(
                                                    op.value.clone(),
                                                )
                                            {
                                                cache_project_for_task(task.id, task.project_id)
                                                    .await;
                                                if task.project_id == project_id {
                                                    return Some(Ok(LogMsg::JsonPatch(patch)));
                                                }
                                            }
                                        }
                                        json_patch::PatchOperation::Remove(_) => {
                                            if let Some(task_id) =
                                                task_id_from_path(patch_op.path())
                                            {
                                                invalidate_task_cache(task_id).await;
                                            }
                                            // For remove operations, we need to check project membership differently
                                            // We could cache this information or let it pass through for now
                                            // Since we don't have the task data, we'll allow all removals
                                            // and let the client handle filtering
                                            return Some(Ok(LogMsg::JsonPatch(patch)));
                                        }
                                        _ => {}
                                    }
                                } else if let Ok(event_patch_value) = serde_json::to_value(patch_op)
                                    && let Ok(event_patch) =
                                        serde_json::from_value::<EventPatch>(event_patch_value)
                                {
                                    // Handle old EventPatch format for non-task records
                                    match &event_patch.value.record {
                                        RecordTypes::Task(task) => {
                                            cache_project_for_task(task.id, task.project_id).await;
                                            if task.project_id == project_id {
                                                return Some(Ok(LogMsg::JsonPatch(patch)));
                                            }
                                        }
                                        RecordTypes::DeletedTask {
                                            project_id: deleted_project_id,
                                            task_id,
                                            ..
                                        } => {
                                            if let Some(task_id) = task_id {
                                                invalidate_task_cache(*task_id).await;
                                            }
                                            if let Some(deleted_project_id) = deleted_project_id
                                                && *deleted_project_id == project_id
                                            {
                                                return Some(Ok(LogMsg::JsonPatch(patch)));
                                            }
                                        }
                                        RecordTypes::Workspace(workspace) => {
                                            // Check if this workspace belongs to a task in our project
                                            if let Some(task_project_id) =
                                                get_project_for_task(&db_pool, workspace.task_id)
                                                    .await
                                                && task_project_id == project_id
                                            {
                                                return Some(Ok(LogMsg::JsonPatch(patch)));
                                            }
                                        }
                                        RecordTypes::DeletedWorkspace {
                                            task_id: Some(deleted_task_id),
                                            ..
                                        } => {
                                            // Check if deleted workspace belonged to a task in our project
                                            if let Some(task_project_id) =
                                                get_project_for_task(&db_pool, *deleted_task_id)
                                                    .await
                                                && task_project_id == project_id
                                            {
                                                return Some(Ok(LogMsg::JsonPatch(patch)));
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            None
                        }
                        Ok(other) => Some(Ok(other)), // Pass through non-patch messages
                        Err(BroadcastStreamRecvError::Lagged(skipped)) => {
                            tracing::warn!(
                                skipped = skipped,
                                project_id = %project_id,
                                "tasks stream lagged; resyncing snapshot"
                            );

                            match build_tasks_snapshot(&db_pool, project_id).await {
                                Ok(snapshot) => Some(Ok(snapshot)),
                                Err(err) => {
                                    tracing::error!(
                                        error = %err,
                                        "failed to resync tasks after lag"
                                    );
                                    Some(Err(std::io::Error::other(format!(
                                        "failed to resync tasks after lag: {err}"
                                    ))))
                                }
                            }
                        }
                    }
                }
            });

        if !include_snapshot {
            return Ok(filtered_stream.boxed());
        }

        // Get initial snapshot of tasks
        let initial_msg = build_tasks_snapshot(&self.db.pool, project_id).await?;

        // Start with initial snapshot, then live updates
        let initial_stream = futures::stream::once(async move { Ok(initial_msg) });
        let combined_stream = initial_stream.chain(filtered_stream).boxed();

        Ok(combined_stream)
    }

    /// Stream raw project messages with initial snapshot
    pub async fn stream_projects_raw(
        &self,
    ) -> Result<futures::stream::BoxStream<'static, Result<LogMsg, std::io::Error>>, EventError>
    {
        fn build_projects_snapshot(projects: Vec<Project>) -> LogMsg {
            // Convert projects array to object keyed by project ID
            let projects_map: serde_json::Map<String, serde_json::Value> = projects
                .into_iter()
                .map(|project| {
                    (
                        project.id.to_string(),
                        serde_json::to_value(project).unwrap(),
                    )
                })
                .collect();

            let patch = json!([
                {
                    "op": "replace",
                    "path": "/projects",
                    "value": projects_map
                }
            ]);

            LogMsg::JsonPatch(serde_json::from_value(patch).unwrap())
        }

        // Get initial snapshot of projects
        let projects = Project::find_all(&self.db.pool).await?;
        let initial_msg = build_projects_snapshot(projects);

        let db_pool = self.db.pool.clone();

        // Get filtered event stream (projects only)
        let filtered_stream =
            BroadcastStream::new(self.msg_store.get_receiver()).filter_map(move |msg_result| {
                let db_pool = db_pool.clone();
                async move {
                    match msg_result {
                        Ok(LogMsg::JsonPatch(patch)) => {
                            if let Some(patch_op) = patch.0.first()
                                && patch_op.path().starts_with("/projects")
                            {
                                return Some(Ok(LogMsg::JsonPatch(patch)));
                            }
                            None
                        }
                        Ok(other) => Some(Ok(other)), // Pass through non-patch messages
                        Err(BroadcastStreamRecvError::Lagged(skipped)) => {
                            tracing::warn!(
                                skipped = skipped,
                                "projects stream lagged; resyncing snapshot"
                            );

                            match Project::find_all(&db_pool).await {
                                Ok(projects) => Some(Ok(build_projects_snapshot(projects))),
                                Err(err) => {
                                    tracing::error!(
                                        error = %err,
                                        "failed to resync projects after lag"
                                    );
                                    Some(Err(std::io::Error::other(format!(
                                        "failed to resync projects after lag: {err}"
                                    ))))
                                }
                            }
                        }
                    }
                }
            });

        // Start with initial snapshot, then live updates
        let initial_stream = futures::stream::once(async move { Ok(initial_msg) });
        let combined_stream = initial_stream.chain(filtered_stream).boxed();

        Ok(combined_stream)
    }

    /// Stream execution processes for a specific workspace with initial snapshot (raw LogMsg format for WebSocket)
    pub async fn stream_execution_processes_for_workspace_raw(
        &self,
        workspace_id: Uuid,
        show_soft_deleted: bool,
    ) -> Result<futures::stream::BoxStream<'static, Result<LogMsg, std::io::Error>>, EventError>
    {
        async fn build_execution_processes_snapshot(
            db_pool: &SqlitePool,
            workspace_id: Uuid,
            show_soft_deleted: bool,
        ) -> Result<LogMsg, sqlx::Error> {
            // Get all sessions for this workspace
            let sessions = Session::find_by_workspace_id(db_pool, workspace_id).await?;

            // Collect all execution processes across all sessions
            let mut all_processes = Vec::new();
            for session in &sessions {
                let processes =
                    ExecutionProcess::find_by_session_id(db_pool, session.id, show_soft_deleted)
                        .await?;
                all_processes.extend(processes);
            }

            // Convert processes array to object keyed by process ID
            let processes_map: serde_json::Map<String, serde_json::Value> = all_processes
                .into_iter()
                .map(|process| {
                    (
                        process.id.to_string(),
                        serde_json::to_value(process).unwrap(),
                    )
                })
                .collect();

            let patch = json!([{
                "op": "replace",
                "path": "/execution_processes",
                "value": processes_map
            }]);

            Ok(LogMsg::JsonPatch(serde_json::from_value(patch).unwrap()))
        }

        // Get all sessions for this workspace
        let sessions = Session::find_by_workspace_id(&self.db.pool, workspace_id).await?;

        // Collect session IDs for filtering
        let session_ids: Vec<Uuid> = sessions.iter().map(|s| s.id).collect();

        // Collect all execution processes across all sessions for initial snapshot
        let mut all_processes = Vec::new();
        for session in &sessions {
            let processes =
                ExecutionProcess::find_by_session_id(&self.db.pool, session.id, show_soft_deleted)
                    .await?;
            all_processes.extend(processes);
        }

        // Convert processes array to object keyed by process ID
        let processes_map: serde_json::Map<String, serde_json::Value> = all_processes
            .into_iter()
            .map(|process| {
                (
                    process.id.to_string(),
                    serde_json::to_value(process).unwrap(),
                )
            })
            .collect();

        let initial_patch = json!([{
            "op": "replace",
            "path": "/execution_processes",
            "value": processes_map
        }]);
        let initial_msg = LogMsg::JsonPatch(serde_json::from_value(initial_patch).unwrap());

        let db_pool = self.db.pool.clone();

        // Get filtered event stream
        let filtered_stream =
            BroadcastStream::new(self.msg_store.get_receiver()).filter_map(move |msg_result| {
                let session_ids = session_ids.clone();
                let db_pool = db_pool.clone();
                async move {
                    match msg_result {
                        Ok(LogMsg::JsonPatch(patch)) => {
                            // Filter events based on session_id (must belong to one of the workspace's sessions)
                            if let Some(patch_op) = patch.0.first() {
                                // Check if this is a modern execution process patch
                                if patch_op.path().starts_with("/execution_processes/") {
                                    match patch_op {
                                        json_patch::PatchOperation::Add(op) => {
                                            // Parse execution process data directly from value
                                            if let Ok(process) =
                                                serde_json::from_value::<ExecutionProcess>(
                                                    op.value.clone(),
                                                )
                                                && process
                                                    .session_id
                                                    .is_some_and(|sid| session_ids.contains(&sid))
                                            {
                                                if !show_soft_deleted && process.dropped {
                                                    let remove_patch =
                                                        execution_process_patch::remove(process.id);
                                                    return Some(Ok(LogMsg::JsonPatch(
                                                        remove_patch,
                                                    )));
                                                }
                                                return Some(Ok(LogMsg::JsonPatch(patch)));
                                            }
                                        }
                                        json_patch::PatchOperation::Replace(op) => {
                                            // Parse execution process data directly from value
                                            if let Ok(process) =
                                                serde_json::from_value::<ExecutionProcess>(
                                                    op.value.clone(),
                                                )
                                                && process
                                                    .session_id
                                                    .is_some_and(|sid| session_ids.contains(&sid))
                                            {
                                                if !show_soft_deleted && process.dropped {
                                                    let remove_patch =
                                                        execution_process_patch::remove(process.id);
                                                    return Some(Ok(LogMsg::JsonPatch(
                                                        remove_patch,
                                                    )));
                                                }
                                                return Some(Ok(LogMsg::JsonPatch(patch)));
                                            }
                                        }
                                        json_patch::PatchOperation::Remove(_) => {
                                            // For remove operations, we can't verify session_id
                                            // so we allow all removals and let the client handle filtering
                                            return Some(Ok(LogMsg::JsonPatch(patch)));
                                        }
                                        _ => {}
                                    }
                                }
                                // Fallback to legacy EventPatch format for backward compatibility
                                else if let Ok(event_patch_value) = serde_json::to_value(patch_op)
                                    && let Ok(event_patch) =
                                        serde_json::from_value::<EventPatch>(event_patch_value)
                                {
                                    match &event_patch.value.record {
                                        RecordTypes::ExecutionProcess(process) => {
                                            if process
                                                .session_id
                                                .is_some_and(|sid| session_ids.contains(&sid))
                                            {
                                                if !show_soft_deleted && process.dropped {
                                                    let remove_patch =
                                                        execution_process_patch::remove(process.id);
                                                    return Some(Ok(LogMsg::JsonPatch(
                                                        remove_patch,
                                                    )));
                                                }
                                                return Some(Ok(LogMsg::JsonPatch(patch)));
                                            }
                                        }
                                        RecordTypes::DeletedExecutionProcess {
                                            session_id: Some(deleted_session_id),
                                            ..
                                        } => {
                                            if session_ids.contains(deleted_session_id) {
                                                return Some(Ok(LogMsg::JsonPatch(patch)));
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            None
                        }
                        Ok(other) => Some(Ok(other)), // Pass through non-patch messages
                        Err(BroadcastStreamRecvError::Lagged(skipped)) => {
                            tracing::warn!(
                                skipped = skipped,
                                workspace_id = %workspace_id,
                                "execution processes stream lagged; resyncing snapshot"
                            );

                            match build_execution_processes_snapshot(
                                &db_pool,
                                workspace_id,
                                show_soft_deleted,
                            )
                            .await
                            {
                                Ok(snapshot) => Some(Ok(snapshot)),
                                Err(err) => {
                                    tracing::error!(
                                        error = %err,
                                        "failed to resync execution processes after lag"
                                    );
                                    Some(Err(std::io::Error::other(format!(
                                        "failed to resync execution processes after lag: {err}"
                                    ))))
                                }
                            }
                        }
                    }
                }
            });

        // Start with initial snapshot, then live updates
        let initial_stream = futures::stream::once(async move { Ok(initial_msg) });
        let combined_stream = initial_stream.chain(filtered_stream).boxed();

        Ok(combined_stream)
    }

    /// Stream execution processes for a specific conversation with initial snapshot (raw LogMsg format for WebSocket)
    pub async fn stream_execution_processes_for_conversation_raw(
        &self,
        conversation_session_id: Uuid,
        show_soft_deleted: bool,
    ) -> Result<futures::stream::BoxStream<'static, Result<LogMsg, std::io::Error>>, EventError>
    {
        async fn build_conversation_processes_snapshot(
            db_pool: &SqlitePool,
            conversation_session_id: Uuid,
            show_soft_deleted: bool,
        ) -> Result<LogMsg, sqlx::Error> {
            let processes = ExecutionProcess::find_by_conversation_session_id(
                db_pool,
                conversation_session_id,
                show_soft_deleted,
            )
            .await?;

            let processes_map: serde_json::Map<String, serde_json::Value> = processes
                .into_iter()
                .map(|process| {
                    (
                        process.id.to_string(),
                        serde_json::to_value(process).unwrap(),
                    )
                })
                .collect();

            let patch = json!([{
                "op": "replace",
                "path": "/execution_processes",
                "value": processes_map
            }]);

            Ok(LogMsg::JsonPatch(serde_json::from_value(patch).unwrap()))
        }

        // Get all execution processes for this conversation
        let processes = ExecutionProcess::find_by_conversation_session_id(
            &self.db.pool,
            conversation_session_id,
            show_soft_deleted,
        )
        .await?;

        // Convert processes array to object keyed by process ID
        let processes_map: serde_json::Map<String, serde_json::Value> = processes
            .into_iter()
            .map(|process| {
                (
                    process.id.to_string(),
                    serde_json::to_value(process).unwrap(),
                )
            })
            .collect();

        let initial_patch = json!([{
            "op": "replace",
            "path": "/execution_processes",
            "value": processes_map
        }]);
        let initial_msg = LogMsg::JsonPatch(serde_json::from_value(initial_patch).unwrap());

        let db_pool = self.db.pool.clone();

        // Get filtered event stream
        let filtered_stream =
            BroadcastStream::new(self.msg_store.get_receiver()).filter_map(move |msg_result| {
                let db_pool = db_pool.clone();
                async move {
                    match msg_result {
                        Ok(LogMsg::JsonPatch(patch)) => {
                            // Filter events based on conversation_session_id
                            if let Some(patch_op) = patch.0.first() {
                                // Check if this is a modern execution process patch
                                if patch_op.path().starts_with("/execution_processes/") {
                                    match patch_op {
                                        json_patch::PatchOperation::Add(op) => {
                                            // Parse execution process data directly from value
                                            if let Ok(process) =
                                                serde_json::from_value::<ExecutionProcess>(
                                                    op.value.clone(),
                                                )
                                                && process.conversation_session_id
                                                    == Some(conversation_session_id)
                                            {
                                                if !show_soft_deleted && process.dropped {
                                                    let remove_patch =
                                                        execution_process_patch::remove(process.id);
                                                    return Some(Ok(LogMsg::JsonPatch(
                                                        remove_patch,
                                                    )));
                                                }
                                                return Some(Ok(LogMsg::JsonPatch(patch)));
                                            }
                                        }
                                        json_patch::PatchOperation::Replace(op) => {
                                            // Parse execution process data directly from value
                                            if let Ok(process) =
                                                serde_json::from_value::<ExecutionProcess>(
                                                    op.value.clone(),
                                                )
                                                && process.conversation_session_id
                                                    == Some(conversation_session_id)
                                            {
                                                if !show_soft_deleted && process.dropped {
                                                    let remove_patch =
                                                        execution_process_patch::remove(process.id);
                                                    return Some(Ok(LogMsg::JsonPatch(
                                                        remove_patch,
                                                    )));
                                                }
                                                return Some(Ok(LogMsg::JsonPatch(patch)));
                                            }
                                        }
                                        json_patch::PatchOperation::Remove(_) => {
                                            // For remove operations, we can't verify conversation_session_id
                                            // so we allow all removals and let the client handle filtering
                                            return Some(Ok(LogMsg::JsonPatch(patch)));
                                        }
                                        _ => {}
                                    }
                                }
                                // Fallback to legacy EventPatch format for backward compatibility
                                else if let Ok(event_patch_value) = serde_json::to_value(patch_op)
                                    && let Ok(event_patch) =
                                        serde_json::from_value::<EventPatch>(event_patch_value)
                                {
                                    match &event_patch.value.record {
                                        RecordTypes::ExecutionProcess(process) => {
                                            if process.conversation_session_id
                                                == Some(conversation_session_id)
                                            {
                                                if !show_soft_deleted && process.dropped {
                                                    let remove_patch =
                                                        execution_process_patch::remove(process.id);
                                                    return Some(Ok(LogMsg::JsonPatch(
                                                        remove_patch,
                                                    )));
                                                }
                                                return Some(Ok(LogMsg::JsonPatch(patch)));
                                            }
                                        }
                                        RecordTypes::DeletedExecutionProcess { .. } => {
                                            // DeletedExecutionProcess doesn't have conversation_session_id
                                            // so we allow all deletions and let the client handle filtering
                                            return Some(Ok(LogMsg::JsonPatch(patch)));
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            None
                        }
                        Ok(other) => Some(Ok(other)), // Pass through non-patch messages
                        Err(BroadcastStreamRecvError::Lagged(skipped)) => {
                            tracing::warn!(
                                skipped = skipped,
                                conversation_session_id = %conversation_session_id,
                                "execution processes stream lagged; resyncing snapshot"
                            );

                            match build_conversation_processes_snapshot(
                                &db_pool,
                                conversation_session_id,
                                show_soft_deleted,
                            )
                            .await
                            {
                                Ok(snapshot) => Some(Ok(snapshot)),
                                Err(err) => {
                                    tracing::error!(
                                        error = %err,
                                        "failed to resync execution processes after lag"
                                    );
                                    Some(Err(std::io::Error::other(format!(
                                        "failed to resync execution processes after lag: {err}"
                                    ))))
                                }
                            }
                        }
                    }
                }
            });

        // Start with initial snapshot, then live updates
        let initial_stream = futures::stream::once(async move { Ok(initial_msg) });
        let combined_stream = initial_stream.chain(filtered_stream).boxed();

        Ok(combined_stream)
    }

    /// Stream workspaces for a specific task with optional initial snapshot (raw LogMsg format for WebSocket)
    pub async fn stream_workspaces_for_task_raw(
        &self,
        task_id: Uuid,
        include_snapshot: bool,
    ) -> Result<futures::stream::BoxStream<'static, Result<LogMsg, std::io::Error>>, EventError>
    {
        async fn build_workspaces_snapshot(
            db_pool: &SqlitePool,
            task_id: Uuid,
        ) -> Result<LogMsg, anyhow::Error> {
            let workspaces_with_sessions =
                WorkspaceWithSession::fetch_with_latest_sessions(db_pool, Some(task_id)).await?;

            let workspaces_map: serde_json::Map<String, serde_json::Value> = workspaces_with_sessions
                .into_iter()
                .map(|ws| {
                    (
                        ws.workspace.id.to_string(),
                        serde_json::to_value(ws).unwrap(),
                    )
                })
                .collect();

            let patch = json!([{
                "op": "replace",
                "path": "/workspaces",
                "value": workspaces_map
            }]);

            Ok(LogMsg::JsonPatch(serde_json::from_value(patch).unwrap()))
        }

        let db_pool = self.db.pool.clone();

        // Get filtered event stream
        let filtered_stream =
            BroadcastStream::new(self.msg_store.get_receiver()).filter_map(move |msg_result| {
                let db_pool = db_pool.clone();
                async move {
                    match msg_result {
                        Ok(LogMsg::JsonPatch(patch)) => {
                            if let Some(patch_op) = patch.0.first() {
                                // Check if this is a workspace patch
                                if patch_op.path().starts_with("/workspaces/") {
                                    match patch_op {
                                        json_patch::PatchOperation::Add(op) => {
                                            // Parse workspace data directly from value
                                            if let Ok(workspace) =
                                                serde_json::from_value::<Workspace>(op.value.clone())
                                                && workspace.task_id == task_id
                                            {
                                                let session = Session::find_latest_by_workspace_id(
                                                    &db_pool,
                                                    workspace.id,
                                                )
                                                .await
                                                .ok()
                                                .flatten();

                                                let workspace_with_session =
                                                    WorkspaceWithSession { workspace, session };
                                                let patch = json_patch::Patch(vec![
                                                    json_patch::PatchOperation::Add(
                                                        json_patch::AddOperation {
                                                            path: op.path.clone(),
                                                            value: serde_json::to_value(
                                                                workspace_with_session,
                                                            )
                                                            .unwrap(),
                                                        },
                                                    ),
                                                ]);
                                                return Some(Ok(LogMsg::JsonPatch(patch)));
                                            }
                                        }
                                        json_patch::PatchOperation::Replace(op) => {
                                            // Parse workspace data directly from value
                                            if let Ok(workspace) =
                                                serde_json::from_value::<Workspace>(op.value.clone())
                                                && workspace.task_id == task_id
                                            {
                                                let session = Session::find_latest_by_workspace_id(
                                                    &db_pool,
                                                    workspace.id,
                                                )
                                                .await
                                                .ok()
                                                .flatten();

                                                let workspace_with_session =
                                                    WorkspaceWithSession { workspace, session };
                                                let patch = json_patch::Patch(vec![
                                                    json_patch::PatchOperation::Replace(
                                                        json_patch::ReplaceOperation {
                                                            path: op.path.clone(),
                                                            value: serde_json::to_value(
                                                                workspace_with_session,
                                                            )
                                                            .unwrap(),
                                                        },
                                                    ),
                                                ]);
                                                return Some(Ok(LogMsg::JsonPatch(patch)));
                                            }
                                        }
                                        json_patch::PatchOperation::Remove(_) => {
                                            // For remove operations, we can't verify task_id
                                            // since we don't have the workspace data anymore.
                                            // Pass through all removals and let the client handle filtering.
                                            return Some(Ok(LogMsg::JsonPatch(patch)));
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            None
                        }
                        Ok(other) => Some(Ok(other)), // Pass through non-patch messages
                        Err(BroadcastStreamRecvError::Lagged(skipped)) => {
                            tracing::warn!(
                                skipped = skipped,
                                task_id = %task_id,
                                "workspaces stream lagged; resyncing snapshot"
                            );

                            match build_workspaces_snapshot(&db_pool, task_id).await {
                                Ok(snapshot) => Some(Ok(snapshot)),
                                Err(err) => {
                                    tracing::error!(
                                        error = %err,
                                        "failed to resync workspaces after lag"
                                    );
                                    Some(Err(std::io::Error::other(format!(
                                        "failed to resync workspaces after lag: {err}"
                                    ))))
                                }
                            }
                        }
                    }
                }
            });

        if !include_snapshot {
            return Ok(filtered_stream.boxed());
        }

        // Get initial snapshot of workspaces for this task
        let initial_msg = build_workspaces_snapshot(&self.db.pool, task_id)
            .await
            .map_err(EventError::Other)?;

        // Start with initial snapshot, then live updates
        let initial_stream = futures::stream::once(async move { Ok(initial_msg) });
        let combined_stream = initial_stream.chain(filtered_stream).boxed();

        Ok(combined_stream)
    }

    /// Stream a single scratch item with initial snapshot (raw LogMsg format for WebSocket)
    pub async fn stream_scratch_raw(
        &self,
        scratch_id: Uuid,
        scratch_type: &db::models::scratch::ScratchType,
    ) -> Result<futures::stream::BoxStream<'static, Result<LogMsg, std::io::Error>>, EventError>
    {
        // Treat errors (e.g., corrupted/malformed data) the same as "scratch not found"
        // This prevents the websocket from closing and retrying indefinitely
        let scratch = match Scratch::find_by_id(&self.db.pool, scratch_id, scratch_type).await {
            Ok(scratch) => scratch,
            Err(e) => {
                tracing::warn!(
                    scratch_id = %scratch_id,
                    scratch_type = %scratch_type,
                    error = %e,
                    "Failed to load scratch, treating as empty"
                );
                None
            }
        };

        let initial_patch = json!([{
            "op": "replace",
            "path": "/scratch",
            "value": scratch
        }]);
        let initial_msg = LogMsg::JsonPatch(serde_json::from_value(initial_patch).unwrap());

        let type_str = scratch_type.to_string();

        // Filter to only this scratch's events by matching id and payload.type in the patch value
        let filtered_stream =
            BroadcastStream::new(self.msg_store.get_receiver()).filter_map(move |msg_result| {
                let id_str = scratch_id.to_string();
                let type_str = type_str.clone();
                async move {
                    match msg_result {
                        Ok(LogMsg::JsonPatch(patch)) => {
                            if let Some(op) = patch.0.first()
                                && op.path() == "/scratch"
                            {
                                // Extract id and payload.type from the patch value
                                let value = match op {
                                    json_patch::PatchOperation::Add(a) => Some(&a.value),
                                    json_patch::PatchOperation::Replace(r) => Some(&r.value),
                                    json_patch::PatchOperation::Remove(_) => None,
                                    _ => None,
                                };

                                let matches = value.is_some_and(|v| {
                                    let id_matches =
                                        v.get("id").and_then(|v| v.as_str()) == Some(&id_str);
                                    let type_matches = v
                                        .get("payload")
                                        .and_then(|p| p.get("type"))
                                        .and_then(|t| t.as_str())
                                        == Some(&type_str);
                                    id_matches && type_matches
                                });

                                if matches {
                                    return Some(Ok(LogMsg::JsonPatch(patch)));
                                }
                            }
                            None
                        }
                        Ok(other) => Some(Ok(other)),
                        Err(BroadcastStreamRecvError::Lagged(skipped)) => {
                            tracing::warn!(
                                skipped = skipped,
                                "scratch stream lagged; dropping messages"
                            );
                            None
                        }
                    }
                }
            });

        let initial_stream = futures::stream::once(async move { Ok(initial_msg) });
        let combined_stream = initial_stream.chain(filtered_stream).boxed();
        Ok(combined_stream)
    }

    /// Stream Gantt task data for a specific project with initial snapshot.
    ///
    /// Monitors task changes, dependency changes, and execution process updates
    /// to provide real-time Gantt chart updates.
    pub async fn stream_gantt_raw(
        &self,
        project_id: Uuid,
    ) -> Result<futures::stream::BoxStream<'static, Result<LogMsg, std::io::Error>>, EventError>
    {
        fn build_gantt_snapshot(tasks: Vec<GanttTask>) -> LogMsg {
            let tasks_map: serde_json::Map<String, serde_json::Value> = tasks
                .into_iter()
                .map(|task| (task.id.to_string(), serde_json::to_value(task).unwrap()))
                .collect();

            let patch = json!([{
                "op": "replace",
                "path": "/gantt_tasks",
                "value": tasks_map
            }]);

            LogMsg::JsonPatch(serde_json::from_value(patch).unwrap())
        }

        // Get initial snapshot
        let tasks = GanttTask::find_by_project_id(&self.db.pool, project_id).await?;

        // Cache project membership for all tasks
        for task in &tasks {
            cache_project_for_task(task.id, project_id).await;
        }

        let initial_msg = build_gantt_snapshot(tasks);

        let db_pool = self.db.pool.clone();

        // Filter stream for events that affect this project's Gantt view
        let filtered_stream =
            BroadcastStream::new(self.msg_store.get_receiver()).filter_map(move |msg_result| {
                let db_pool = db_pool.clone();
                async move {
                    match msg_result {
                        Ok(LogMsg::JsonPatch(patch)) => {
                            if let Some(patch_op) = patch.0.first() {
                                // Handle task changes
                                if patch_op.path().starts_with("/tasks/") {
                                    // Helper to extract task_id from task value if it belongs to this project
                                    let extract_project_task_id =
                                        |value: &serde_json::Value| -> Option<Uuid> {
                                            if let Ok(task) =
                                                serde_json::from_value::<TaskWithAttemptStatus>(
                                                    value.clone(),
                                                )
                                                && task.project_id == project_id
                                            {
                                                return Some(task.id);
                                            }
                                            None
                                        };

                                    let task_id = match patch_op {
                                        json_patch::PatchOperation::Add(op) => {
                                            if let Ok(task) =
                                                serde_json::from_value::<TaskWithAttemptStatus>(
                                                    op.value.clone(),
                                                )
                                            {
                                                cache_project_for_task(task.id, task.project_id)
                                                    .await;
                                            }
                                            extract_project_task_id(&op.value)
                                        }
                                        json_patch::PatchOperation::Replace(op) => {
                                            if let Ok(task) =
                                                serde_json::from_value::<TaskWithAttemptStatus>(
                                                    op.value.clone(),
                                                )
                                            {
                                                cache_project_for_task(task.id, task.project_id)
                                                    .await;
                                            }
                                            extract_project_task_id(&op.value)
                                        }
                                        json_patch::PatchOperation::Remove(_) => {
                                            if let Some(task_id) =
                                                task_id_from_path(patch_op.path())
                                            {
                                                invalidate_task_cache(task_id).await;
                                                // Emit remove patch for deleted task
                                                let remove_patch = json!([{
                                                    "op": "remove",
                                                    "path": format!("/gantt_tasks/{}", task_id)
                                                }]);
                                                return Some(Ok(LogMsg::JsonPatch(
                                                    serde_json::from_value(remove_patch).unwrap(),
                                                )));
                                            }
                                            None
                                        }
                                        _ => None,
                                    };

                                    // If task belongs to this project, rebuild its GanttTask
                                    if let Some(task_id) = task_id
                                        && let Ok(Some(gantt_task)) =
                                            GanttTask::find_by_id(&db_pool, task_id).await
                                    {
                                        let patch = json!([{
                                            "op": "replace",
                                            "path": format!("/gantt_tasks/{}", task_id),
                                            "value": gantt_task
                                        }]);
                                        return Some(Ok(LogMsg::JsonPatch(
                                            serde_json::from_value(patch).unwrap(),
                                        )));
                                    }
                                }

                                // Handle execution process changes (affects progress/timeline)
                                if patch_op.path().starts_with("/execution_processes/") {
                                    let task_id = match patch_op {
                                        json_patch::PatchOperation::Add(op) => {
                                            if let Ok(process) =
                                                serde_json::from_value::<ExecutionProcess>(
                                                    op.value.clone(),
                                                )
                                            {
                                                get_task_id_for_execution_process(
                                                    &db_pool,
                                                    process.session_id,
                                                )
                                                .await
                                            } else {
                                                None
                                            }
                                        }
                                        json_patch::PatchOperation::Replace(op) => {
                                            if let Ok(process) =
                                                serde_json::from_value::<ExecutionProcess>(
                                                    op.value.clone(),
                                                )
                                            {
                                                get_task_id_for_execution_process(
                                                    &db_pool,
                                                    process.session_id,
                                                )
                                                .await
                                            } else {
                                                None
                                            }
                                        }
                                        _ => None,
                                    };

                                    if let Some(task_id) = task_id
                                        && let Some(task_project_id) =
                                            get_project_for_task(&db_pool, task_id).await
                                        && task_project_id == project_id
                                        && let Ok(Some(gantt_task)) =
                                            GanttTask::find_by_id(&db_pool, task_id).await
                                    {
                                        let patch = json!([{
                                            "op": "replace",
                                            "path": format!("/gantt_tasks/{}", task_id),
                                            "value": gantt_task
                                        }]);
                                        return Some(Ok(LogMsg::JsonPatch(
                                            serde_json::from_value(patch).unwrap(),
                                        )));
                                    }
                                }

                                // Handle legacy EventPatch format for task_dependencies changes
                                if let Ok(event_patch_value) = serde_json::to_value(patch_op)
                                    && let Ok(event_patch) =
                                        serde_json::from_value::<EventPatch>(event_patch_value)
                                {
                                    match &event_patch.value.record {
                                        RecordTypes::Task(task) => {
                                            cache_project_for_task(task.id, task.project_id).await;
                                            if task.project_id == project_id
                                                && let Ok(Some(gantt_task)) =
                                                    GanttTask::find_by_id(&db_pool, task.id).await
                                            {
                                                let patch = json!([{
                                                    "op": "replace",
                                                    "path": format!("/gantt_tasks/{}", task.id),
                                                    "value": gantt_task
                                                }]);
                                                return Some(Ok(LogMsg::JsonPatch(
                                                    serde_json::from_value(patch).unwrap(),
                                                )));
                                            }
                                        }
                                        RecordTypes::DeletedTask {
                                            project_id: deleted_project_id,
                                            task_id,
                                            ..
                                        } => {
                                            if let Some(task_id) = task_id {
                                                invalidate_task_cache(*task_id).await;
                                            }
                                            if let (Some(del_proj_id), Some(task_id)) =
                                                (deleted_project_id, task_id)
                                                && *del_proj_id == project_id
                                            {
                                                let remove_patch = json!([{
                                                    "op": "remove",
                                                    "path": format!("/gantt_tasks/{}", task_id)
                                                }]);
                                                return Some(Ok(LogMsg::JsonPatch(
                                                    serde_json::from_value(remove_patch).unwrap(),
                                                )));
                                            }
                                        }
                                        RecordTypes::ExecutionProcess(process) => {
                                            // Look up the task via session -> workspace
                                            if let Some(task_id) =
                                                get_task_id_for_execution_process(
                                                    &db_pool,
                                                    process.session_id,
                                                )
                                                .await
                                                && let Some(task_project_id) =
                                                    get_project_for_task(&db_pool, task_id).await
                                                && task_project_id == project_id
                                                && let Ok(Some(gantt_task)) =
                                                    GanttTask::find_by_id(&db_pool, task_id).await
                                            {
                                                let patch = json!([{
                                                    "op": "replace",
                                                    "path": format!("/gantt_tasks/{}", task_id),
                                                    "value": gantt_task
                                                }]);
                                                return Some(Ok(LogMsg::JsonPatch(
                                                    serde_json::from_value(patch).unwrap(),
                                                )));
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            None
                        }
                        Ok(other) => Some(Ok(other)), // Pass through non-patch messages
                        Err(BroadcastStreamRecvError::Lagged(skipped)) => {
                            tracing::warn!(
                                skipped = skipped,
                                project_id = %project_id,
                                "gantt stream lagged; resyncing snapshot"
                            );
                            // Resync with full snapshot on lag
                            match GanttTask::find_by_project_id(&db_pool, project_id).await {
                                Ok(tasks) => Some(Ok(build_gantt_snapshot(tasks))),
                                Err(err) => {
                                    tracing::error!(
                                        error = %err,
                                        "failed to resync gantt after lag"
                                    );
                                    Some(Err(std::io::Error::other(format!(
                                        "failed to resync gantt after lag: {err}"
                                    ))))
                                }
                            }
                        }
                    }
                }
            });

        let initial_stream = futures::stream::once(async move { Ok(initial_msg) });
        let combined_stream = initial_stream.chain(filtered_stream).boxed();

        Ok(combined_stream)
    }

    /// Stream notifications with optional project filtering and initial snapshot.
    ///
    /// When `project_id` is Some, only notifications for that project are streamed.
    /// When `project_id` is None, only global notifications (project_id IS NULL) are streamed.
    /// When `include_snapshot` is true, the stream starts with the last 100 notifications.
    pub async fn stream_notifications_raw(
        &self,
        project_id: Option<Uuid>,
        include_snapshot: bool,
    ) -> Result<futures::stream::BoxStream<'static, Result<LogMsg, std::io::Error>>, EventError>
    {
        fn build_notifications_snapshot(notifications: Vec<Notification>) -> LogMsg {
            let notifications_map: serde_json::Map<String, serde_json::Value> = notifications
                .into_iter()
                .map(|notification| {
                    (
                        notification.id.to_string(),
                        serde_json::to_value(notification).unwrap(),
                    )
                })
                .collect();

            let patch = json!([{
                "op": "replace",
                "path": "/notifications",
                "value": notifications_map
            }]);

            LogMsg::JsonPatch(serde_json::from_value(patch).unwrap())
        }

        let db_pool = self.db.pool.clone();

        // Get filtered event stream (notifications only, filtered by project_id)
        let filtered_stream =
            BroadcastStream::new(self.msg_store.get_receiver()).filter_map(move |msg_result| {
                let db_pool = db_pool.clone();
                async move {
                    match msg_result {
                        Ok(LogMsg::JsonPatch(patch)) => {
                            if let Some(patch_op) = patch.0.first()
                                && patch_op.path().starts_with("/notifications/")
                            {
                                // Check if the notification matches our project_id filter
                                let matches_project = match patch_op {
                                    json_patch::PatchOperation::Add(op) => {
                                        if let Ok(notification) =
                                            serde_json::from_value::<Notification>(op.value.clone())
                                        {
                                            notification.project_id == project_id
                                        } else {
                                            false
                                        }
                                    }
                                    json_patch::PatchOperation::Replace(op) => {
                                        if let Ok(notification) =
                                            serde_json::from_value::<Notification>(op.value.clone())
                                        {
                                            notification.project_id == project_id
                                        } else {
                                            false
                                        }
                                    }
                                    json_patch::PatchOperation::Remove(_) => {
                                        // For remove operations, we can't verify project_id
                                        // since we don't have the notification data anymore.
                                        // Pass through all removals and let the client handle filtering.
                                        true
                                    }
                                    _ => false,
                                };

                                if matches_project {
                                    return Some(Ok(LogMsg::JsonPatch(patch)));
                                }
                            }
                            None
                        }
                        Ok(other) => Some(Ok(other)), // Pass through non-patch messages
                        Err(BroadcastStreamRecvError::Lagged(skipped)) => {
                            tracing::warn!(
                                skipped = skipped,
                                project_id = ?project_id,
                                "notifications stream lagged; resyncing snapshot"
                            );
                            // Resync with full snapshot on lag
                            let notifications = if let Some(pid) = project_id {
                                Notification::find_by_project_id(&db_pool, pid, Some(100)).await
                            } else {
                                Notification::find_global(&db_pool, Some(100)).await
                            };
                            match notifications {
                                Ok(notifs) => Some(Ok(build_notifications_snapshot(notifs))),
                                Err(err) => {
                                    tracing::error!(
                                        error = %err,
                                        "failed to resync notifications after lag"
                                    );
                                    Some(Err(std::io::Error::other(format!(
                                        "failed to resync notifications after lag: {err}"
                                    ))))
                                }
                            }
                        }
                    }
                }
            });

        if !include_snapshot {
            return Ok(filtered_stream.boxed());
        }

        // Get initial snapshot of notifications (last 100)
        let notifications = if let Some(pid) = project_id {
            Notification::find_by_project_id(&self.db.pool, pid, Some(100)).await?
        } else {
            Notification::find_global(&self.db.pool, Some(100)).await?
        };
        let initial_msg = build_notifications_snapshot(notifications);

        // Start with initial snapshot, then live updates
        let initial_stream = futures::stream::once(async move { Ok(initial_msg) });
        let combined_stream = initial_stream.chain(filtered_stream).boxed();

        Ok(combined_stream)
    }
}
