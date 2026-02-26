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

        // Scenario 1: High ratio
        config.write_buffer_size = 1024;
        config.max_write_buffer_size = 1024 * 16; // ratio 16
        let capacity = channel_capacity_from_websocket_config(&config);
        assert_eq!(capacity, 1024); // log2(16) * 256 = 4 * 256 = 1024

        // Scenario 2: Ratio < 1 (fallback max)
        config.write_buffer_size = 1024;
        config.max_write_buffer_size = 512;
        let capacity = channel_capacity_from_websocket_config(&config);
        assert_eq!(capacity, 64); // log2(1) * 256 = 0 -> clamped to 64

        // Scenario 3: Very high ratio (but within clamp)
        config.write_buffer_size = 1024;
        config.max_write_buffer_size = 1024 * 1024 * 1024; // ratio 2^20
        let capacity = channel_capacity_from_websocket_config(&config);
        assert_eq!(capacity, 5120); // log2(2^20) * 256 = 20 * 256 = 5120

        // Scenario 4: Absolute maximum clamp
        config.write_buffer_size = 1;
        config.max_write_buffer_size = 1024 * 1024 * 1024 * 1024; // ratio 2^40
        let capacity = channel_capacity_from_websocket_config(&config);
        assert_eq!(capacity, 10240); // 40 * 256 = 10240, still below 16384
    }
}
