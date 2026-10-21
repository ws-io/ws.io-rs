use anyhow::{
    Result,
    anyhow,
    ensure,
};
use tokio::sync::Semaphore;
use tungstenite::protocol::WebSocketConfig;

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

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use tungstenite::{
        Message,
        WebSocket,
        protocol::Role,
    };

    use super::*;

    #[test]
    fn test_default_websocket_config_is_valid() {
        let config = default_websocket_config();
        validate_transport_config(&config, DEFAULT_CHANNEL_CAPACITY, DEFAULT_MAX_OUTGOING_MESSAGE_SIZE).unwrap();

        let required = config.write_buffer_size + DEFAULT_MAX_OUTGOING_MESSAGE_SIZE + OUTGOING_FRAME_OVERHEAD;
        assert_eq!(config.max_write_buffer_size, required);
    }

    #[test]
    fn test_validate_transport_config_boundaries() {
        let outgoing = DEFAULT_MAX_OUTGOING_MESSAGE_SIZE;
        let required = DEFAULT_BUFFER_SIZE + outgoing + OUTGOING_FRAME_OVERHEAD;
        let mut config = WebSocketConfig::default()
            .write_buffer_size(DEFAULT_BUFFER_SIZE)
            .max_write_buffer_size(required);

        assert!(validate_transport_config(&config, DEFAULT_CHANNEL_CAPACITY, outgoing).is_ok());
        config.max_write_buffer_size -= 1;
        assert!(validate_transport_config(&config, DEFAULT_CHANNEL_CAPACITY, outgoing).is_err());
        config.max_write_buffer_size = usize::MAX;
        assert!(validate_transport_config(&config, 0, outgoing).is_err());
        assert!(validate_transport_config(&config, Semaphore::MAX_PERMITS + 1, outgoing).is_err());
        assert!(validate_transport_config(&config, DEFAULT_CHANNEL_CAPACITY, 0).is_err());
        assert!(validate_transport_config(&config, DEFAULT_CHANNEL_CAPACITY, usize::MAX).is_err());
        config.write_buffer_size = usize::MAX;
        assert!(validate_transport_config(&config, DEFAULT_CHANNEL_CAPACITY, 1).is_err());
    }

    #[test]
    fn test_validated_buffer_fits_maximum_outgoing_frame() {
        let config = default_websocket_config();
        for role in [Role::Server, Role::Client] {
            let mut socket = WebSocket::from_raw_socket(Cursor::new(Vec::new()), role, Some(config));
            socket
                .send(Message::Binary(vec![0; DEFAULT_MAX_OUTGOING_MESSAGE_SIZE].into()))
                .unwrap();
        }
    }
}
