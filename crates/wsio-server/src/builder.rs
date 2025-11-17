use std::time::Duration;

use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use crate::{
    WsIoServer,
    config::WsIoServerConfig,
    core::packet::codecs::WsIoPacketCodec,
    runtime::WsIoServerRuntime,
};

// Structs
pub struct WsIoServerBuilder {
    config: WsIoServerConfig,
}

impl WsIoServerBuilder {
    pub(crate) fn new() -> Self {
        Self {
            config: WsIoServerConfig {
                broadcast_concurrency_limit: 512,
                init_request_handler_timeout: Duration::from_secs(3),
                init_response_handler_timeout: Duration::from_secs(3),
                init_response_timeout: Duration::from_secs(5),
                middleware_execution_timeout: Duration::from_secs(2),
                on_close_handler_timeout: Duration::from_secs(2),
                on_connect_handler_timeout: Duration::from_secs(3),
                packet_codec: WsIoPacketCodec::SerdeJson,
                request_path: "/ws.io".into(),
                websocket_config: WebSocketConfig::default()
                    .max_frame_size(Some(8 * 1024 * 1024))
                    .max_message_size(Some(16 * 1024 * 1024))
                    .max_write_buffer_size(2 * 1024 * 1024)
                    .read_buffer_size(8 * 1024)
                    .write_buffer_size(8 * 1024),
            },
        }
    }

    // Public methods
    pub fn broadcast_concurrency_limit(mut self, broadcast_concurrency_limit: usize) -> Self {
        self.config.broadcast_concurrency_limit = broadcast_concurrency_limit;
        self
    }

    pub fn build(self) -> WsIoServer {
        WsIoServer(WsIoServerRuntime::new(self.config))
    }

    pub fn init_request_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.init_request_handler_timeout = duration;
        self
    }

    pub fn init_response_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.init_response_handler_timeout = duration;
        self
    }

    pub fn init_response_timeout(mut self, duration: Duration) -> Self {
        self.config.init_response_timeout = duration;
        self
    }

    pub fn middleware_execution_timeout(mut self, duration: Duration) -> Self {
        self.config.middleware_execution_timeout = duration;
        self
    }

    pub fn on_close_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.on_close_handler_timeout = duration;
        self
    }

    pub fn on_connect_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.on_connect_handler_timeout = duration;
        self
    }

    pub fn packet_codec(mut self, packet_codec: WsIoPacketCodec) -> Self {
        self.config.packet_codec = packet_codec;
        self
    }

    pub fn request_path(mut self, request_path: impl AsRef<str>) -> Self {
        self.config.request_path = request_path.as_ref().into();
        self
    }

    pub fn websocket_config(mut self, websocket_config: WebSocketConfig) -> Self {
        self.config.websocket_config = websocket_config;
        self
    }

    pub fn websocket_config_mut<F: FnOnce(&mut WebSocketConfig)>(mut self, f: F) -> Self {
        f(&mut self.config.websocket_config);
        self
    }
}
