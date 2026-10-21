use std::time::Duration;

use anyhow::{
    Result,
    anyhow,
    ensure,
};
use tokio::{
    select,
    sync::Semaphore,
    task::JoinHandle,
    time::timeout,
};
use tungstenite::protocol::WebSocketConfig;

// Enums
enum CompletedWebSocketTask {
    Read,
    Write,
}

// Constants/Statics
const DEFAULT_BUFFER_SIZE: usize = 8 * 1024;

/// Default message capacity of each internal channel.
pub const DEFAULT_CHANNEL_CAPACITY: usize = 2048;

/// Default maximum outgoing Binary payload size after packet transformation, excluding frame headers.
pub const DEFAULT_MAX_OUTGOING_MESSAGE_SIZE: usize = 2 * 1024 * 1024;

/// Write-buffer bytes reserved beyond an outgoing payload: the maximum masked frame header (14 bytes)
/// plus room for queued control frames.
pub const OUTGOING_FRAME_OVERHEAD: usize = 14 + 256;

// Functions

/// Returns the default WebSocket transport configuration shared by servers and clients.
///
/// The defaults are 8 MiB per incoming frame, 16 MiB per incoming message, and 8 KiB read
/// and write buffers. The maximum write buffer fits [`DEFAULT_MAX_OUTGOING_MESSAGE_SIZE`]
/// plus the write buffer and [`OUTGOING_FRAME_OVERHEAD`]. Other settings use
/// [`WebSocketConfig::default`].
#[inline]
pub fn default_websocket_config() -> WebSocketConfig {
    WebSocketConfig::default()
        .max_frame_size(Some(8 * 1024 * 1024))
        .max_message_size(Some(16 * 1024 * 1024))
        .max_write_buffer_size(DEFAULT_BUFFER_SIZE + DEFAULT_MAX_OUTGOING_MESSAGE_SIZE + OUTGOING_FRAME_OVERHEAD)
        .read_buffer_size(DEFAULT_BUFFER_SIZE)
        .write_buffer_size(DEFAULT_BUFFER_SIZE)
}

/// Waits for either transport task to finish, then stops its sibling.
/// If neither finishes within the grace period, aborts and joins both tasks.
pub async fn shutdown_websocket_stream_tasks(
    mut read_task: JoinHandle<()>,
    mut write_task: JoinHandle<()>,
    shutdown_timeout: Duration,
) {
    let graceful_shutdown = timeout(shutdown_timeout, async {
        select! {
            _ = &mut read_task => CompletedWebSocketTask::Read,
            _ = &mut write_task => CompletedWebSocketTask::Write,
        }
    })
    .await;

    match graceful_shutdown {
        Ok(CompletedWebSocketTask::Read) => {
            write_task.abort();
            let _ = write_task.await;
        },
        Ok(CompletedWebSocketTask::Write) => {
            read_task.abort();
            let _ = read_task.await;
        },
        Err(_) => {
            #[cfg(feature = "tracing")]
            tracing::warn!(
                timeout_ms = u64::try_from(shutdown_timeout.as_millis()).unwrap_or(u64::MAX),
                "graceful WebSocket shutdown timed out; aborting tasks"
            );

            read_task.abort();
            write_task.abort();
            let _ = read_task.await;
            let _ = write_task.await;
        },
    }
}

/// Validates queue capacity and outgoing WebSocket buffer requirements.
///
/// # Errors
///
/// Returns an error for invalid capacities, size overflow, or an insufficient write buffer.
pub fn validate_transport_config(
    config: &WebSocketConfig,
    channel_capacity: usize,
    max_outgoing_message_size: usize,
) -> Result<()> {
    ensure!(
        channel_capacity > 0 && channel_capacity <= Semaphore::MAX_PERMITS,
        "channel_capacity must be between 1 and {}",
        Semaphore::MAX_PERMITS
    );

    ensure!(
        max_outgoing_message_size > 0,
        "max_outgoing_message_size must be greater than zero"
    );

    let required = config
        .write_buffer_size
        .checked_add(max_outgoing_message_size)
        .and_then(|size| size.checked_add(OUTGOING_FRAME_OVERHEAD))
        .ok_or_else(|| anyhow!("outgoing WebSocket buffer size calculation overflowed"))?;

    ensure!(
        config.max_write_buffer_size >= required,
        "websocket_config.max_write_buffer_size ({}) must be at least {required} bytes for write_buffer_size ({}) and max_outgoing_message_size ({max_outgoing_message_size})",
        config.max_write_buffer_size,
        config.write_buffer_size
    );

    Ok(())
}
