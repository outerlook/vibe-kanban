use anyhow;
use axum::{
    Extension, Router,
    extract::{
        Path, Query, State,
        ws::{WebSocket, WebSocketUpgrade},
    },
    middleware::from_fn_with_state,
    response::{IntoResponse, Json as ResponseJson},
    routing::{get, post},
};
use db::models::{
    execution_process::{ExecutionProcess, ExecutionProcessError, ExecutionProcessStatus},
    execution_process_normalized_entry::{
        ExecutionProcessNormalizedEntriesPage, ExecutionProcessNormalizedEntry,
    },
    execution_process_repo_state::ExecutionProcessRepoState,
};
use deployment::Deployment;
use futures_util::{SinkExt, StreamExt, TryStreamExt, stream::BoxStream};
use serde::Deserialize;
use services::services::container::{ContainerError, ContainerService};
use utils::{log_msg::LogMsg, response::ApiResponse};
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError, middleware::load_execution_process_middleware};

#[derive(Debug, Deserialize)]
pub struct ExecutionProcessQuery {
    pub workspace_id: Option<Uuid>,
    pub conversation_session_id: Option<Uuid>,
    /// If true, include soft-deleted (dropped) processes in results/stream
    #[serde(default)]
    pub show_soft_deleted: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct NormalizedEntriesQuery {
    pub before_index: Option<i64>,
    pub limit: Option<usize>,
}

const DEFAULT_NORMALIZED_ENTRIES_LIMIT: usize = 200;
const MAX_NORMALIZED_ENTRIES_LIMIT: usize = 500;

pub async fn get_execution_process_by_id(
    Extension(execution_process): Extension<ExecutionProcess>,
    State(_deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<ExecutionProcess>>, ApiError> {
    Ok(ResponseJson(ApiResponse::success(execution_process)))
}

pub async fn stream_raw_logs_ws(
    ws: WebSocketUpgrade,
    State(deployment): State<DeploymentImpl>,
    Path(exec_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    // Check if the stream exists before upgrading the WebSocket
    let _stream = deployment
        .container()
        .stream_raw_logs(&exec_id)
        .await
        .ok_or_else(|| {
            ApiError::ExecutionProcess(ExecutionProcessError::ExecutionProcessNotFound)
        })?;

    Ok(ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_raw_logs_ws(socket, deployment, exec_id).await {
            tracing::warn!("raw logs WS closed: {}", e);
        }
    }))
}

async fn handle_raw_logs_ws(
    socket: WebSocket,
    deployment: DeploymentImpl,
    exec_id: Uuid,
) -> anyhow::Result<()> {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use executors::logs::utils::patch::ConversationPatch;
    use utils::log_msg::LogMsg;

    // Get the raw stream and convert to JSON patches on-the-fly
    let raw_stream = deployment
        .container()
        .stream_raw_logs(&exec_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("Execution process not found"))?;

    let counter = Arc::new(AtomicUsize::new(0));
    let mut stream = raw_stream.map_ok({
        let counter = counter.clone();
        move |m| match m {
            LogMsg::Stdout(content) => {
                let index = counter.fetch_add(1, Ordering::SeqCst);
                let patch = ConversationPatch::add_stdout(index, content);
                LogMsg::JsonPatch(patch).to_ws_message_unchecked()
            }
            LogMsg::Stderr(content) => {
                let index = counter.fetch_add(1, Ordering::SeqCst);
                let patch = ConversationPatch::add_stderr(index, content);
                LogMsg::JsonPatch(patch).to_ws_message_unchecked()
            }
            LogMsg::Finished => LogMsg::Finished.to_ws_message_unchecked(),
            _ => unreachable!("Raw stream should only have Stdout/Stderr/Finished"),
        }
    });

    // Split socket into sender and receiver
    let (mut sender, mut receiver) = socket.split();

    // Drain (and ignore) any client->server messages so pings/pongs work
    tokio::spawn(async move { while let Some(Ok(_)) = receiver.next().await {} });

    // Forward server messages
    while let Some(item) = stream.next().await {
        match item {
            Ok(msg) => {
                if sender.send(msg).await.is_err() {
                    break; // client disconnected
                }
            }
            Err(e) => {
                tracing::error!("stream error: {}", e);
                break;
            }
        }
    }
    Ok(())
}

pub async fn stream_normalized_logs_ws(
    ws: WebSocketUpgrade,
    State(deployment): State<DeploymentImpl>,
    Path(exec_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let stream = deployment
        .container()
        .stream_normalized_logs(&exec_id)
        .await
        .ok_or_else(|| {
            ApiError::ExecutionProcess(ExecutionProcessError::ExecutionProcessNotFound)
        })?;

    // Convert the error type to anyhow::Error and turn TryStream -> Stream<Result<_, _>>
    let stream = stream.err_into::<anyhow::Error>().into_stream();

    Ok(ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_normalized_logs_ws(socket, stream).await {
            tracing::warn!("normalized logs WS closed: {}", e);
        }
    }))
}

pub async fn get_normalized_entries(
    Extension(execution_process): Extension<ExecutionProcess>,
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<NormalizedEntriesQuery>,
) -> Result<ResponseJson<ApiResponse<ExecutionProcessNormalizedEntriesPage>>, ApiError> {
    let limit = query
        .limit
        .unwrap_or(DEFAULT_NORMALIZED_ENTRIES_LIMIT)
        .clamp(1, MAX_NORMALIZED_ENTRIES_LIMIT);

    let existing = ExecutionProcessNormalizedEntry::count_by_execution_id(
        &deployment.db().pool,
        execution_process.id,
    )
    .await?;

    if existing == 0 && execution_process.status != ExecutionProcessStatus::Running {
        deployment
            .container()
            .backfill_normalized_entries(execution_process.id)
            .await?;
    }

    let page = ExecutionProcessNormalizedEntry::fetch_page(
        &deployment.db().pool,
        execution_process.id,
        query.before_index,
        limit,
    )
    .await
    .map_err(|err| ApiError::Container(ContainerError::Other(err)))?;

    Ok(ResponseJson(ApiResponse::success(page)))
}

async fn handle_normalized_logs_ws(
    socket: WebSocket,
    stream: impl futures_util::Stream<Item = anyhow::Result<LogMsg>> + Unpin + Send + 'static,
) -> anyhow::Result<()> {
    let mut stream = stream.map_ok(|msg| msg.to_ws_message_unchecked());
    let (mut sender, mut receiver) = socket.split();
    tokio::spawn(async move { while let Some(Ok(_)) = receiver.next().await {} });
    while let Some(item) = stream.next().await {
        match item {
            Ok(msg) => {
                if sender.send(msg).await.is_err() {
                    break;
                }
            }
            Err(e) => {
                tracing::error!("stream error: {}", e);
                break;
            }
        }
    }
    Ok(())
}

pub async fn stop_execution_process(
    Extension(execution_process): Extension<ExecutionProcess>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    deployment
        .container()
        .stop_execution(&execution_process, ExecutionProcessStatus::Killed)
        .await?;

    Ok(ResponseJson(ApiResponse::success(())))
}

pub async fn stream_execution_processes_ws(
    ws: WebSocketUpgrade,
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ExecutionProcessQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let show_soft_deleted = query.show_soft_deleted.unwrap_or(false);

    match (query.workspace_id, query.conversation_session_id) {
        (Some(workspace_id), None) => Ok(ws.on_upgrade(move |socket| async move {
            if let Err(e) = handle_workspace_execution_processes_ws(
                socket,
                deployment,
                workspace_id,
                show_soft_deleted,
            )
            .await
            {
                tracing::warn!("execution processes WS closed: {}", e);
            }
        })),
        (None, Some(conversation_session_id)) => Ok(ws.on_upgrade(move |socket| async move {
            if let Err(e) = handle_conversation_execution_processes_ws(
                socket,
                deployment,
                conversation_session_id,
                show_soft_deleted,
            )
            .await
            {
                tracing::warn!("execution processes WS closed: {}", e);
            }
        })),
        (Some(_), Some(_)) => Err(ApiError::BadRequest(
            "Cannot specify both workspace_id and conversation_session_id".to_string(),
        )),
        (None, None) => Err(ApiError::BadRequest(
            "Must specify either workspace_id or conversation_session_id".to_string(),
        )),
    }
}

async fn handle_workspace_execution_processes_ws(
    socket: WebSocket,
    deployment: DeploymentImpl,
    workspace_id: Uuid,
    show_soft_deleted: bool,
) -> anyhow::Result<()> {
    let stream = deployment
        .events()
        .stream_execution_processes_for_workspace_raw(workspace_id, show_soft_deleted)
        .await?;
    stream_execution_processes_to_ws(socket, stream).await
}

async fn handle_conversation_execution_processes_ws(
    socket: WebSocket,
    deployment: DeploymentImpl,
    conversation_session_id: Uuid,
    show_soft_deleted: bool,
) -> anyhow::Result<()> {
    let stream = deployment
        .events()
        .stream_execution_processes_for_conversation_raw(conversation_session_id, show_soft_deleted)
        .await?;
    stream_execution_processes_to_ws(socket, stream).await
}

async fn stream_execution_processes_to_ws(
    socket: WebSocket,
    stream: BoxStream<'static, Result<LogMsg, std::io::Error>>,
) -> anyhow::Result<()> {
    let mut stream = stream.map_ok(|msg| msg.to_ws_message_unchecked());

    let (mut sender, mut receiver) = socket.split();

    // Drain (and ignore) any client->server messages so pings/pongs work
    tokio::spawn(async move { while let Some(Ok(_)) = receiver.next().await {} });

    // Forward server messages
    while let Some(item) = stream.next().await {
        match item {
            Ok(msg) => {
                if sender.send(msg).await.is_err() {
                    break; // client disconnected
                }
            }
            Err(e) => {
                tracing::error!("stream error: {}", e);
                break;
            }
        }
    }
    Ok(())
}

pub async fn get_execution_process_repo_states(
    Extension(execution_process): Extension<ExecutionProcess>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<ExecutionProcessRepoState>>>, ApiError> {
    let pool = &deployment.db().pool;
    let repo_states =
        ExecutionProcessRepoState::find_by_execution_process_id(pool, execution_process.id).await?;
    Ok(ResponseJson(ApiResponse::success(repo_states)))
}

pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let workspace_id_router = Router::new()
        .route("/", get(get_execution_process_by_id))
        .route("/stop", post(stop_execution_process))
        .route("/repo-states", get(get_execution_process_repo_states))
        .route("/normalized-entries", get(get_normalized_entries))
        .route("/raw-logs/ws", get(stream_raw_logs_ws))
        .route("/normalized-logs/ws", get(stream_normalized_logs_ws))
        .layer(from_fn_with_state(
            deployment.clone(),
            load_execution_process_middleware,
        ));

    let workspaces_router = Router::new()
        .route("/stream/ws", get(stream_execution_processes_ws))
        .nest("/{id}", workspace_id_router);

    Router::new().nest("/execution-processes", workspaces_router)
}
