use std::{str::FromStr, sync::Arc};

use db::{
    DBService,
    models::{
        execution_process::ExecutionProcess, notification::Notification, project::Project,
        scratch::Scratch, task::Task, task_dependency::TaskDependency, workspace::Workspace,
    },
};
use serde_json::json;
use sqlx::{Error as SqlxError, Sqlite, SqlitePool, decode::Decode, sqlite::SqliteOperation};
use tokio::sync::{RwLock, mpsc};
use utils::msg_store::MsgStore;
use uuid::Uuid;

#[path = "events/patches.rs"]
pub mod patches;
#[path = "events/streams.rs"]
mod streams;
#[path = "events/types.rs"]
pub mod types;

pub use patches::{
    execution_process_patch, notification_patch, project_patch, scratch_patch, task_patch,
    workspace_patch,
};
pub use types::{EventError, EventPatch, EventPatchInner, HookTables, RecordTypes};

/// Maximum number of pending events in the worker queue.
/// Provides backpressure when database writes outpace event processing.
const EVENT_CHANNEL_CAPACITY: usize = 1024;

/// Event sent from the SQLite update hook to the async worker.
#[derive(Debug)]
pub struct HookEvent {
    table: HookTables,
    operation: SqliteOperation,
    rowid: i64,
}

/// Handle to the event worker task, allowing graceful shutdown.
pub struct EventWorkerHandle {
    sender: mpsc::Sender<HookEvent>,
    worker_handle: tokio::task::JoinHandle<()>,
}

impl EventWorkerHandle {
    /// Returns a clone of the sender for use with `create_hook`.
    pub fn sender(&self) -> mpsc::Sender<HookEvent> {
        self.sender.clone()
    }

    /// Shuts down the event worker, waiting for pending events to be processed.
    pub async fn shutdown(self) {
        // Drop sender to signal worker to stop
        drop(self.sender);
        // Wait for worker to finish processing remaining events
        if let Err(e) = self.worker_handle.await {
            tracing::error!("Event worker task panicked: {:?}", e);
        }
    }
}

#[derive(Clone)]
pub struct EventService {
    msg_store: Arc<MsgStore>,
    #[allow(dead_code)]
    db: DBService,
    #[allow(dead_code)]
    entry_count: Arc<RwLock<usize>>,
}

impl EventService {
    /// Creates a new EventService that will work with a DBService configured with hooks
    pub fn new(db: DBService, msg_store: Arc<MsgStore>, entry_count: Arc<RwLock<usize>>) -> Self {
        Self {
            msg_store,
            db,
            entry_count,
        }
    }

    async fn push_task_update_for_task(
        pool: &SqlitePool,
        msg_store: Arc<MsgStore>,
        task_id: Uuid,
    ) -> Result<(), SqlxError> {
        if let Some(task_with_status) = Task::find_by_id_with_attempt_status(pool, task_id).await? {
            msg_store.push_patch(task_patch::replace(&task_with_status));
        }

        Ok(())
    }

    async fn push_task_update_for_session(
        pool: &SqlitePool,
        msg_store: Arc<MsgStore>,
        session_id: Uuid,
    ) -> Result<(), SqlxError> {
        use db::models::session::Session;
        if let Some(session) = Session::find_by_id(pool, session_id).await?
            && let Some(workspace) = Workspace::find_by_id(pool, session.workspace_id).await?
        {
            Self::push_task_update_for_task(pool, msg_store, workspace.task_id).await?;
        }

        Ok(())
    }

    async fn update_materialized_status_for_session(
        pool: &SqlitePool,
        session_id: Uuid,
    ) -> Result<(), SqlxError> {
        use db::models::session::Session;
        if let Some(session) = Session::find_by_id(pool, session_id).await?
            && let Some(workspace) = Workspace::find_by_id(pool, session.workspace_id).await?
        {
            Task::update_materialized_status(pool, workspace.task_id).await?;
        }

        Ok(())
    }

    /// Spawns the event worker and returns a handle for shutdown.
    /// Must be called before `create_hook` to get the sender.
    pub fn spawn_event_worker(
        msg_store: Arc<MsgStore>,
        entry_count: Arc<RwLock<usize>>,
        db_service: DBService,
    ) -> EventWorkerHandle {
        let (sender, receiver) = mpsc::channel::<HookEvent>(EVENT_CHANNEL_CAPACITY);

        let worker_handle = tokio::spawn(Self::event_worker_loop(
            receiver,
            msg_store,
            entry_count,
            db_service,
        ));

        EventWorkerHandle {
            sender,
            worker_handle,
        }
    }

    /// The main event worker loop that processes events from the channel.
    async fn event_worker_loop(
        mut receiver: mpsc::Receiver<HookEvent>,
        msg_store: Arc<MsgStore>,
        entry_count: Arc<RwLock<usize>>,
        db: DBService,
    ) {
        while let Some(event) = receiver.recv().await {
            Self::process_hook_event(&event, &msg_store, &entry_count, &db).await;
        }
        tracing::info!("Event worker shutting down, channel closed");
    }

    /// Process a single hook event.
    async fn process_hook_event(
        event: &HookEvent,
        msg_store: &Arc<MsgStore>,
        entry_count: &Arc<RwLock<usize>>,
        db: &DBService,
    ) {
        let HookEvent {
            table,
            operation,
            rowid,
        } = event;

        // Deletions are handled in preupdate hook for reliable data capture
        if matches!(operation, SqliteOperation::Delete) {
            return;
        }

        let record_type: RecordTypes = match table {
            HookTables::Tasks => match Task::find_by_rowid(&db.pool, *rowid).await {
                Ok(Some(task)) => RecordTypes::Task(task),
                Ok(None) => RecordTypes::DeletedTask {
                    rowid: *rowid,
                    project_id: None,
                    task_id: None,
                },
                Err(e) => {
                    tracing::error!("Failed to fetch task: {:?}", e);
                    return;
                }
            },
            HookTables::Projects => match Project::find_by_rowid(&db.pool, *rowid).await {
                Ok(Some(project)) => RecordTypes::Project(project),
                Ok(None) => RecordTypes::DeletedProject {
                    rowid: *rowid,
                    project_id: None,
                },
                Err(e) => {
                    tracing::error!("Failed to fetch project: {:?}", e);
                    return;
                }
            },
            HookTables::Workspaces => match Workspace::find_by_rowid(&db.pool, *rowid).await {
                Ok(Some(workspace)) => RecordTypes::Workspace(workspace),
                Ok(None) => RecordTypes::DeletedWorkspace {
                    rowid: *rowid,
                    task_id: None,
                },
                Err(e) => {
                    tracing::error!("Failed to fetch workspace: {:?}", e);
                    return;
                }
            },
            HookTables::ExecutionProcesses => {
                match ExecutionProcess::find_by_rowid(&db.pool, *rowid).await {
                    Ok(Some(process)) => RecordTypes::ExecutionProcess(process),
                    Ok(None) => RecordTypes::DeletedExecutionProcess {
                        rowid: *rowid,
                        session_id: None,
                        process_id: None,
                    },
                    Err(e) => {
                        tracing::error!("Failed to fetch execution_process: {:?}", e);
                        return;
                    }
                }
            }
            HookTables::Scratch => match Scratch::find_by_rowid(&db.pool, *rowid).await {
                Ok(Some(scratch)) => RecordTypes::Scratch(scratch),
                Ok(None) => RecordTypes::DeletedScratch {
                    rowid: *rowid,
                    scratch_id: None,
                    scratch_type: None,
                },
                Err(e) => {
                    tracing::error!("Failed to fetch scratch: {:?}", e);
                    return;
                }
            },
            HookTables::Notifications => {
                match Notification::find_by_rowid(&db.pool, *rowid).await {
                    Ok(Some(notification)) => RecordTypes::Notification(notification),
                    Ok(None) => RecordTypes::DeletedNotification {
                        rowid: *rowid,
                        notification_id: None,
                    },
                    Err(e) => {
                        tracing::error!("Failed to fetch notification: {:?}", e);
                        return;
                    }
                }
            }
        };

        let db_op: &str = match operation {
            SqliteOperation::Insert => "insert",
            SqliteOperation::Delete => "delete",
            SqliteOperation::Update => "update",
            SqliteOperation::Unknown(_) => "unknown",
        };

        // Handle task-related operations with direct patches
        match &record_type {
            RecordTypes::Task(task) => {
                // Convert Task to TaskWithAttemptStatus
                if let Ok(Some(task_with_status)) =
                    Task::find_by_id_with_attempt_status(&db.pool, task.id).await
                {
                    let patch = match operation {
                        SqliteOperation::Insert => task_patch::add(&task_with_status),
                        SqliteOperation::Update => task_patch::replace(&task_with_status),
                        _ => task_patch::replace(&task_with_status), // fallback
                    };
                    msg_store.push_patch(patch);

                    // Push updates for tasks that depend on this one.
                    // Their is_blocked status may have changed.
                    if let Ok(dependent_tasks) = TaskDependency::find_blocking(&db.pool, task.id).await
                    {
                        // Update materialized status for dependent tasks (is_blocked column)
                        let dependent_task_ids: Vec<Uuid> =
                            dependent_tasks.iter().map(|t| t.id).collect();
                        if let Err(err) =
                            Task::update_materialized_status_bulk(&db.pool, &dependent_task_ids).await
                        {
                            tracing::error!(
                                "Failed to update materialized status for dependent tasks: {:?}",
                                err
                            );
                        }

                        for dep_task in dependent_tasks {
                            let _ = Self::push_task_update_for_task(
                                &db.pool,
                                msg_store.clone(),
                                dep_task.id,
                            )
                            .await;
                        }
                    }

                    return;
                }
            }
            RecordTypes::DeletedTask {
                task_id: Some(task_id),
                ..
            } => {
                let patch = task_patch::remove(*task_id);
                msg_store.push_patch(patch);
                return;
            }
            RecordTypes::Project(project) => {
                let patch = match operation {
                    SqliteOperation::Insert => project_patch::add(project),
                    SqliteOperation::Update => project_patch::replace(project),
                    _ => project_patch::replace(project),
                };
                msg_store.push_patch(patch);
                return;
            }
            RecordTypes::Scratch(scratch) => {
                let patch = match operation {
                    SqliteOperation::Insert => scratch_patch::add(scratch),
                    SqliteOperation::Update => scratch_patch::replace(scratch),
                    _ => scratch_patch::replace(scratch),
                };
                msg_store.push_patch(patch);
                return;
            }
            RecordTypes::DeletedScratch {
                scratch_id: Some(scratch_id),
                scratch_type: Some(scratch_type_str),
                ..
            } => {
                let patch = scratch_patch::remove(*scratch_id, scratch_type_str);
                msg_store.push_patch(patch);
                return;
            }
            RecordTypes::Workspace(workspace) => {
                // First, broadcast workspace patch
                let workspace_patch = match operation {
                    SqliteOperation::Insert => workspace_patch::add(workspace),
                    SqliteOperation::Update => workspace_patch::replace(workspace),
                    _ => workspace_patch::replace(workspace), // fallback
                };
                msg_store.push_patch(workspace_patch);

                // Update materialized status columns for the task
                if let Err(err) =
                    Task::update_materialized_status(&db.pool, workspace.task_id).await
                {
                    tracing::error!(
                        "Failed to update materialized status for task after workspace change: {:?}",
                        err
                    );
                }

                // Then, update the parent task with fresh data
                if let Ok(Some(task_with_status)) =
                    Task::find_by_id_with_attempt_status(&db.pool, workspace.task_id).await
                {
                    let task_patch = task_patch::replace(&task_with_status);
                    msg_store.push_patch(task_patch);
                }

                return;
            }
            RecordTypes::DeletedWorkspace {
                task_id: Some(task_id),
                ..
            } => {
                // Update materialized status columns for the task
                if let Err(err) = Task::update_materialized_status(&db.pool, *task_id).await {
                    tracing::error!(
                        "Failed to update materialized status for task after workspace deletion: {:?}",
                        err
                    );
                }

                // Workspace deletion should update the parent task with fresh data
                if let Ok(Some(task_with_status)) =
                    Task::find_by_id_with_attempt_status(&db.pool, *task_id).await
                {
                    let patch = task_patch::replace(&task_with_status);
                    msg_store.push_patch(patch);
                    return;
                }
            }
            RecordTypes::ExecutionProcess(process) => {
                let patch = match operation {
                    SqliteOperation::Insert => execution_process_patch::add(process),
                    SqliteOperation::Update => execution_process_patch::replace(process),
                    _ => execution_process_patch::replace(process), // fallback
                };
                msg_store.push_patch(patch);

                // Only push task update for workspace-based executions
                if let Some(session_id) = process.session_id {
                    // Update materialized status columns first
                    if let Err(err) =
                        Self::update_materialized_status_for_session(&db.pool, session_id).await
                    {
                        tracing::error!(
                            "Failed to update materialized status after execution process change: {:?}",
                            err
                        );
                    }

                    // Then push task update via WebSocket
                    if let Err(err) =
                        Self::push_task_update_for_session(&db.pool, msg_store.clone(), session_id)
                            .await
                    {
                        tracing::error!(
                            "Failed to push task update after execution process change: {:?}",
                            err
                        );
                    }
                }

                return;
            }
            RecordTypes::DeletedExecutionProcess {
                process_id: Some(process_id),
                session_id,
                ..
            } => {
                let patch = execution_process_patch::remove(*process_id);
                msg_store.push_patch(patch);

                if let Some(session_id) = session_id {
                    // Update materialized status columns first
                    if let Err(err) =
                        Self::update_materialized_status_for_session(&db.pool, *session_id).await
                    {
                        tracing::error!(
                            "Failed to update materialized status after execution process removal: {:?}",
                            err
                        );
                    }

                    // Then push task update via WebSocket
                    if let Err(err) =
                        Self::push_task_update_for_session(&db.pool, msg_store.clone(), *session_id)
                            .await
                    {
                        tracing::error!(
                            "Failed to push task update after execution process removal: {:?}",
                            err
                        );
                    }
                }

                return;
            }
            RecordTypes::Notification(notification) => {
                let patch = match operation {
                    SqliteOperation::Insert => notification_patch::add(notification),
                    SqliteOperation::Update => notification_patch::replace(notification),
                    _ => notification_patch::replace(notification),
                };
                msg_store.push_patch(patch);
                return;
            }
            RecordTypes::DeletedNotification {
                notification_id: Some(notification_id),
                ..
            } => {
                let patch = notification_patch::remove(*notification_id);
                msg_store.push_patch(patch);
                return;
            }
            _ => {}
        }

        // Fallback: use the old entries format for other record types
        let next_entry_count = {
            let mut entry_count = entry_count.write().await;
            *entry_count += 1;
            *entry_count
        };

        let event_patch: EventPatch = EventPatch {
            op: "add".to_string(),
            path: format!("/entries/{next_entry_count}"),
            value: EventPatchInner {
                db_op: db_op.to_string(),
                record: record_type,
            },
        };

        let patch =
            serde_json::from_value(json!([serde_json::to_value(event_patch).unwrap()])).unwrap();

        msg_store.push_patch(patch);
    }

    /// Creates the hook function that should be used with DBService::new_with_after_connect.
    /// The `event_sender` should come from `spawn_event_worker`.
    pub fn create_hook(
        msg_store: Arc<MsgStore>,
        event_sender: mpsc::Sender<HookEvent>,
    ) -> impl for<'a> Fn(
        &'a mut sqlx::sqlite::SqliteConnection,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), sqlx::Error>> + Send + 'a>,
    > + Send
    + Sync
    + 'static {
        move |conn: &mut sqlx::sqlite::SqliteConnection| {
            let msg_store_for_hook = msg_store.clone();
            let event_sender = event_sender.clone();
            Box::pin(async move {
                let mut handle = conn.lock_handle().await?;
                handle.set_preupdate_hook({
                    let msg_store_for_preupdate = msg_store_for_hook.clone();
                    move |preupdate: sqlx::sqlite::PreupdateHookResult<'_>| {
                        if preupdate.operation != SqliteOperation::Delete {
                            return;
                        }

                        match preupdate.table {
                            "tasks" => {
                                if let Ok(value) = preupdate.get_old_column_value(0)
                                    && let Ok(task_id) = <Uuid as Decode<Sqlite>>::decode(value)
                                {
                                    let patch = task_patch::remove(task_id);
                                    msg_store_for_preupdate.push_patch(patch);
                                }
                            }
                            "projects" => {
                                if let Ok(value) = preupdate.get_old_column_value(0)
                                    && let Ok(project_id) = <Uuid as Decode<Sqlite>>::decode(value)
                                {
                                    let patch = project_patch::remove(project_id);
                                    msg_store_for_preupdate.push_patch(patch);
                                }
                            }
                            "workspaces" => {
                                if let Ok(value) = preupdate.get_old_column_value(0)
                                    && let Ok(workspace_id) =
                                        <Uuid as Decode<Sqlite>>::decode(value)
                                {
                                    let patch = workspace_patch::remove(workspace_id);
                                    msg_store_for_preupdate.push_patch(patch);
                                }
                            }
                            "execution_processes" => {
                                if let Ok(value) = preupdate.get_old_column_value(0)
                                    && let Ok(process_id) = <Uuid as Decode<Sqlite>>::decode(value)
                                {
                                    let patch = execution_process_patch::remove(process_id);
                                    msg_store_for_preupdate.push_patch(patch);
                                }
                            }
                            "scratch" => {
                                // Composite key: need both id (column 0) and scratch_type (column 1)
                                if let Ok(id_val) = preupdate.get_old_column_value(0)
                                    && let Ok(scratch_id) = <Uuid as Decode<Sqlite>>::decode(id_val)
                                    && let Ok(type_val) = preupdate.get_old_column_value(1)
                                    && let Ok(type_str) =
                                        <String as Decode<Sqlite>>::decode(type_val)
                                {
                                    let patch = scratch_patch::remove(scratch_id, &type_str);
                                    msg_store_for_preupdate.push_patch(patch);
                                }
                            }
                            "notifications" => {
                                if let Ok(value) = preupdate.get_old_column_value(0)
                                    && let Ok(notification_id) =
                                        <Uuid as Decode<Sqlite>>::decode(value)
                                {
                                    let patch = notification_patch::remove(notification_id);
                                    msg_store_for_preupdate.push_patch(patch);
                                }
                            }
                            _ => {}
                        }
                    }
                });

                handle.set_update_hook(move |hook: sqlx::sqlite::UpdateHookResult<'_>| {
                    if let Ok(table) = HookTables::from_str(hook.table) {
                        let event = HookEvent {
                            table,
                            operation: hook.operation.clone(),
                            rowid: hook.rowid,
                        };

                        // Use try_send to avoid blocking the SQLite callback.
                        // If the channel is full, we log a warning and drop the event.
                        // This provides backpressure without blocking database operations.
                        if let Err(mpsc::error::TrySendError::Full(_)) =
                            event_sender.try_send(event)
                        {
                            tracing::warn!(
                                "Event channel full, dropping event for table {} rowid {}",
                                hook.table,
                                hook.rowid
                            );
                        }
                    }
                });

                Ok(())
            })
        }
    }

    pub fn msg_store(&self) -> &Arc<MsgStore> {
        &self.msg_store
    }
}
