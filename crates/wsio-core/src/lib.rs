#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

use tungstenite::protocol::WebSocketConfig;

pub mod event;
pub mod packet;
pub mod traits;
pub mod types;
pub mod utils;

pub fn channel_capacity_from_websocket_config(websocket_config: &WebSocketConfig) -> usize {
    let ratio = (websocket_config.max_write_buffer_size as f64 / websocket_config.write_buffer_size as f64).max(1.0);
    let capacity = (ratio.log2() * 256.0).round() as usize;
    capacity.clamp(64, 16384)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_capacity_from_websocket_config() {
        let mut config = WebSocketConfig::default();

        config.write_buffer_size = 1024;
        config.max_write_buffer_size = 1024 * 16;
        assert_eq!(channel_capacity_from_websocket_config(&config), 1024);

        config.write_buffer_size = 1024;
        config.max_write_buffer_size = 512;
        assert_eq!(channel_capacity_from_websocket_config(&config), 64);

        config.write_buffer_size = 1024;
        config.max_write_buffer_size = 1024 * 1024 * 1024;
        assert_eq!(channel_capacity_from_websocket_config(&config), 5120);

        config.write_buffer_size = 1;
        config.max_write_buffer_size = 1024 * 1024 * 1024 * 1024;
        assert_eq!(channel_capacity_from_websocket_config(&config), 10240);

        config.write_buffer_size = 1;
        config.max_write_buffer_size = usize::MAX;
        assert_eq!(channel_capacity_from_websocket_config(&config), 16384);
    }
}
