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
