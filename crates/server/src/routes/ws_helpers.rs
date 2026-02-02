use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt, TryStreamExt, stream::BoxStream};
use utils::log_msg::LogMsg;

/// Forward a stream of LogMsg to a WebSocket, handling connection lifecycle properly.
///
/// This function:
/// 1. Spawns a task to drain incoming client messages (for ping/pong handling)
/// 2. Forwards server messages to the client
/// 3. Properly cancels the drain task when the sender loop exits
///
/// This prevents socket leaks (CLOSE_WAIT accumulation) by ensuring both halves
/// of the WebSocket are dropped together when either side disconnects.
pub async fn forward_stream_to_ws(
    socket: WebSocket,
    stream: BoxStream<'static, Result<LogMsg, std::io::Error>>,
) -> anyhow::Result<()> {
    let mut stream = stream.map_ok(|msg| msg.to_ws_message_unchecked());

    // Split socket into sender and receiver
    let (mut sender, mut receiver) = socket.split();

    // Spawn a task to drain (and ignore) any client->server messages so pings/pongs work.
    // We use AbortHandle to cancel this task when the sender loop exits.
    let drain_handle = tokio::spawn(async move {
        while let Some(Ok(_)) = receiver.next().await {}
    });

    // Forward server messages
    let result = loop {
        match stream.next().await {
            Some(Ok(msg)) => {
                if sender.send(msg).await.is_err() {
                    break Ok(()); // client disconnected
                }
            }
            Some(Err(e)) => {
                tracing::error!("stream error: {}", e);
                break Err(anyhow::anyhow!("stream error: {}", e));
            }
            None => break Ok(()), // stream ended
        }
    };

    // Abort the drain task to ensure the receiver half is dropped.
    // This allows the TCP connection to properly close (preventing CLOSE_WAIT).
    drain_handle.abort();

    // Explicitly close the sender to trigger TCP FIN
    let _ = sender.close().await;

    result
}

/// Forward a stream of WebSocket messages directly (for non-LogMsg streams).
pub async fn forward_ws_messages(
    socket: WebSocket,
    mut stream: BoxStream<'static, Result<Message, std::io::Error>>,
) -> anyhow::Result<()> {
    // Split socket into sender and receiver
    let (mut sender, mut receiver) = socket.split();

    // Spawn a task to drain client messages
    let drain_handle = tokio::spawn(async move {
        while let Some(Ok(_)) = receiver.next().await {}
    });

    // Forward server messages
    let result = loop {
        match stream.next().await {
            Some(Ok(msg)) => {
                if sender.send(msg).await.is_err() {
                    break Ok(()); // client disconnected
                }
            }
            Some(Err(e)) => {
                tracing::error!("stream error: {}", e);
                break Err(anyhow::anyhow!("stream error: {}", e));
            }
            None => break Ok(()), // stream ended
        }
    };

    // Abort the drain task
    drain_handle.abort();

    // Explicitly close the sender
    let _ = sender.close().await;

    result
}
