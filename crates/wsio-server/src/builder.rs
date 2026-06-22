use std::time::Duration;

use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use crate::{
    WsIoServer,
    config::WsIoServerConfig,
    core::packet::codecs::WsIoPacketCodec,
    runtime::WsIoServerRuntime,
};

// Structs

/// Builder for configuring and creating a [`WsIoServer`].
///
/// Server-level settings become the defaults inherited by namespaces created
/// from the server. Namespace builders may override most of these values per
/// namespace.
#[derive(Debug)]
pub struct WsIoServerBuilder {
    config: WsIoServerConfig,
}

impl WsIoServerBuilder {
    pub(crate) fn new() -> Self {
        Self {
            config: WsIoServerConfig {
                broadcast_concurrency_limit: 512,
                http_request_upgrade_timeout: Duration::from_secs(3),
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
    /// Sets the default maximum number of broadcast send operations to run at
    /// once.
    ///
    /// This value is inherited by namespace builders and passed to
    /// `StreamExt::for_each_concurrent`; `0` is treated as no concurrency limit.
    pub fn broadcast_concurrency_limit(mut self, broadcast_concurrency_limit: usize) -> Self {
        self.config.broadcast_concurrency_limit = broadcast_concurrency_limit;
        self
    }

    /// Builds a [`WsIoServer`] with the accumulated configuration.
    pub fn build(self) -> WsIoServer {
        WsIoServer(WsIoServerRuntime::new(self.config))
    }

    /// Sets the default timeout for a matched HTTP request to finish the
    /// WebSocket upgrade.
    ///
    /// The timeout wraps the HTTP adapter's upgrade future. Namespace builders
    /// inherit this value and may override it.
    pub fn http_request_upgrade_timeout(mut self, duration: Duration) -> Self {
        self.config.http_request_upgrade_timeout = duration;
        self
    }

    /// Sets the default maximum duration allowed for init-request handlers.
    ///
    /// Init-request handlers are registered per namespace with
    /// `WsIoServerNamespaceBuilder::with_init_request`.
    pub fn init_request_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.init_request_handler_timeout = duration;
        self
    }

    /// Sets the default maximum duration allowed for init-response handlers.
    ///
    /// Init-response handlers are registered per namespace with
    /// `WsIoServerNamespaceBuilder::with_init_response`.
    pub fn init_response_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.init_response_handler_timeout = duration;
        self
    }

    /// Sets the default timeout for waiting on a client init-response packet.
    ///
    /// This timeout starts after the server sends its init packet. Namespace
    /// builders inherit this value and may override it.
    pub fn init_response_timeout(mut self, duration: Duration) -> Self {
        self.config.init_response_timeout = duration;
        self
    }

    /// Sets the default maximum duration allowed for namespace middleware.
    ///
    /// Middleware is registered per namespace with
    /// `WsIoServerNamespaceBuilder::with_middleware`.
    pub fn middleware_execution_timeout(mut self, duration: Duration) -> Self {
        self.config.middleware_execution_timeout = duration;
        self
    }

    /// Sets the default maximum duration allowed for per-connection close
    /// handlers.
    ///
    /// This applies to handlers registered through `WsIoServerConnection::on_close`.
    pub fn on_close_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.on_close_handler_timeout = duration;
        self
    }

    /// Sets the default maximum duration allowed for namespace on-connect
    /// handlers.
    ///
    /// On-connect handlers are registered per namespace with
    /// `WsIoServerNamespaceBuilder::on_connect`.
    pub fn on_connect_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.on_connect_handler_timeout = duration;
        self
    }

    /// Sets the default packet codec for namespaces.
    ///
    /// The codec is used for ws.io protocol packets and init payload data.
    /// Namespace builders inherit this value and may override it.
    pub fn packet_codec(mut self, packet_codec: WsIoPacketCodec) -> Self {
        self.config.packet_codec = packet_codec;
        self
    }

    /// Sets the HTTP request path handled by the server adapter.
    ///
    /// Requests whose URI path does not match this value pass through to the
    /// wrapped service. Client namespace routing is carried separately in the
    /// `namespace` query parameter.
    pub fn request_path(mut self, request_path: impl AsRef<str>) -> Self {
        self.config.request_path = request_path.as_ref().into();
        self
    }

    /// Replaces the default Tungstenite WebSocket configuration.
    ///
    /// Namespace builders inherit this value. It controls transport limits and
    /// buffer sizes and is also used to derive internal channel capacity from the
    /// configured max-write/write-buffer ratio.
    pub fn websocket_config(mut self, websocket_config: WebSocketConfig) -> Self {
        self.config.websocket_config = websocket_config;
        self
    }

    /// Mutates the current default Tungstenite WebSocket configuration in place.
    ///
    /// Prefer this when you want to adjust one or two fields while keeping the
    /// builder defaults for the rest.
    pub fn websocket_config_mut<F: FnOnce(&mut WebSocketConfig)>(mut self, f: F) -> Self {
        f(&mut self.config.websocket_config);
        self
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn test_builder_configuration_chaining() {
        let server = WsIoServer::builder()
            .broadcast_concurrency_limit(1024)
            .http_request_upgrade_timeout(Duration::from_millis(750))
            .init_request_handler_timeout(Duration::from_secs(1))
            .init_response_handler_timeout(Duration::from_secs(2))
            .init_response_timeout(Duration::from_secs(3))
            .middleware_execution_timeout(Duration::from_secs(4))
            .on_close_handler_timeout(Duration::from_secs(5))
            .on_connect_handler_timeout(Duration::from_secs(6))
            .packet_codec(WsIoPacketCodec::Msgpack)
            .request_path("/custom")
            .websocket_config_mut(|config| {
                *config = config.max_frame_size(Some(999));
            })
            .build();

        // Access internal config through the built runtime
        let config = &server.0.config;
        assert_eq!(config.broadcast_concurrency_limit, 1024);
        assert_eq!(config.http_request_upgrade_timeout, Duration::from_millis(750));
        assert_eq!(config.init_request_handler_timeout, Duration::from_secs(1));
        assert_eq!(config.init_response_handler_timeout, Duration::from_secs(2));
        assert_eq!(config.init_response_timeout, Duration::from_secs(3));
        assert_eq!(config.middleware_execution_timeout, Duration::from_secs(4));
        assert_eq!(config.on_close_handler_timeout, Duration::from_secs(5));
        assert_eq!(config.on_connect_handler_timeout, Duration::from_secs(6));
        assert!(matches!(config.packet_codec, WsIoPacketCodec::Msgpack));
        assert_eq!(config.request_path, "/custom");
        assert_eq!(config.websocket_config.max_frame_size, Some(999));
    }

    #[test]
    fn test_builder_websocket_config_override() {
        let config = WebSocketConfig::default().max_frame_size(Some(42));
        let server = WsIoServer::builder().websocket_config(config).build();
        assert_eq!(server.0.config.websocket_config.max_frame_size, Some(42));
    }
}
