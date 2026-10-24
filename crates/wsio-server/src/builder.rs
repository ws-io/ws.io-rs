use std::time::Duration;

use anyhow::Result;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use crate::{
    WsIoServer,
    config::WsIoServerConfig,
    core::{
        packet::{
            codecs::WsIoPacketCodec,
            transformers::WsIoPacketTransformer,
        },
        transport::{
            DEFAULT_CHANNEL_CAPACITY,
            DEFAULT_MAX_OUTGOING_MESSAGE_SIZE,
            default_websocket_config,
            validate_transport_config,
        },
        utils::path::resolve_path,
    },
    runtime::WsIoServerRuntime,
};

// Structs

/// A builder for configuring and creating a [`WsIoServer`].
///
/// Server-level settings are inherited by namespaces created from the server.
/// Namespace builders may override most settings per namespace.
///
/// Configuration methods consume the builder and return the updated builder.
#[derive(Debug)]
#[must_use]
pub struct WsIoServerBuilder {
    config: WsIoServerConfig,
}

impl WsIoServerBuilder {
    pub(crate) fn new() -> Self {
        Self {
            config: WsIoServerConfig {
                broadcast_concurrency_limit: 512,
                channel_capacity: DEFAULT_CHANNEL_CAPACITY,
                connection_idle_timeout: Some(Duration::from_secs(90)),
                disconnect_timeout: Duration::from_secs(5),
                http_request_upgrade_timeout: Duration::from_secs(3),
                init_request_handler_timeout: Duration::from_secs(3),
                init_response_handler_timeout: Duration::from_secs(3),
                init_response_timeout: Duration::from_secs(5),
                max_outgoing_message_size: DEFAULT_MAX_OUTGOING_MESSAGE_SIZE,
                middleware_execution_timeout: Duration::from_secs(2),
                on_close_handler_timeout: Duration::from_secs(2),
                on_connect_handler_timeout: Duration::from_secs(3),
                packet_codec: WsIoPacketCodec::Msgpack,
                packet_transformer: WsIoPacketTransformer::default(),
                request_path: "/ws.io".to_owned(),
                websocket_config: default_websocket_config(),
            },
        }
    }

    // Public methods
    /// Sets the default maximum number of concurrent broadcast sends.
    ///
    /// Namespace builders inherit this value. `0` means unlimited concurrency.
    pub fn broadcast_concurrency_limit(mut self, broadcast_concurrency_limit: usize) -> Self {
        self.config.broadcast_concurrency_limit = broadcast_concurrency_limit;
        self
    }

    /// Builds a [`WsIoServer`] from this builder's configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if channel capacity is invalid, the outgoing limit is zero,
    /// or the write buffer cannot accommodate the outgoing limit and required overhead.
    pub fn build(self) -> Result<WsIoServer> {
        validate_transport_config(
            &self.config.websocket_config,
            self.config.channel_capacity,
            self.config.max_outgoing_message_size,
        )?;

        Ok(WsIoServer(WsIoServerRuntime::new(self.config)))
    }

    /// Sets the default message capacity of each internal channel.
    ///
    /// Defaults to `2048`. Must be between 1 and [`tokio::sync::Semaphore::MAX_PERMITS`].
    /// This limits message count, not bytes, and is independent of WebSocket buffer sizes.
    /// Namespace builders inherit this value and may override it.
    pub fn channel_capacity(mut self, channel_capacity: usize) -> Self {
        self.config.channel_capacity = channel_capacity;
        self
    }

    /// Sets the default maximum duration for waiting for the next WebSocket message on a ready connection.
    ///
    /// Packet processing time is excluded. Defaults to 90 seconds; `None` disables the timeout.
    /// `Some(Duration::ZERO)` does not disable the timeout.
    /// Namespace builders inherit this value and may override it.
    pub fn connection_idle_timeout(mut self, duration: Option<Duration>) -> Self {
        self.config.connection_idle_timeout = duration;
        self
    }

    /// Sets the default timeout for each disconnect phase: preparing and queueing the
    /// protocol packet, then waiting for transport shutdown after cancellation.
    ///
    /// Defaults to 5 seconds. Cleanup callbacks have their own timeouts.
    /// A disconnect may take up to two phases; zero requests no grace period.
    /// Namespace builders inherit this value and may override it.
    pub fn disconnect_timeout(mut self, duration: Duration) -> Self {
        self.config.disconnect_timeout = duration;
        self
    }

    /// Sets the default maximum duration for a matched HTTP request's WebSocket
    /// upgrade.
    ///
    /// The timeout wraps the HTTP adapter's upgrade future. Namespace builders
    /// inherit this value and may override it.
    pub fn http_request_upgrade_timeout(mut self, duration: Duration) -> Self {
        self.config.http_request_upgrade_timeout = duration;
        self
    }

    /// Sets the default maximum duration for init-request handlers.
    ///
    /// This limits the init-request callback configured on namespace builders created
    /// with [`WsIoServer::new_namespace_builder`].
    pub fn init_request_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.init_request_handler_timeout = duration;
        self
    }

    /// Sets the default maximum duration for init-response handlers.
    ///
    /// This limits the init-response callback configured on namespace builders created
    /// with [`WsIoServer::new_namespace_builder`].
    pub fn init_response_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.init_response_handler_timeout = duration;
        self
    }

    /// Sets the default maximum duration for waiting for a client init response.
    ///
    /// This starts when the connection begins waiting for the client response,
    /// before the server init packet is queued for sending. Namespace builders
    /// inherit this value and may override it.
    pub fn init_response_timeout(mut self, duration: Duration) -> Self {
        self.config.init_response_timeout = duration;
        self
    }

    /// Sets the default maximum outgoing Binary payload size after packet transformation, excluding frame headers.
    ///
    /// Defaults to 2 MiB; zero is rejected. The limit applies after codec encoding
    /// and transformation, before queueing. The write buffer must fit this limit
    /// plus its configured write threshold and [`OUTGOING_FRAME_OVERHEAD`](crate::core::transport::OUTGOING_FRAME_OVERHEAD) bytes.
    /// Messages are not fragmented; the peer must accept a frame of this size.
    /// Large broadcasts can leave a large retained write buffer on each connection.
    /// Namespace builders inherit this value and may override it.
    pub fn max_outgoing_message_size(mut self, max_outgoing_message_size: usize) -> Self {
        self.config.max_outgoing_message_size = max_outgoing_message_size;
        self
    }

    /// Sets the default maximum duration for namespace middleware.
    ///
    /// This limits the middleware callback configured on namespace builders created
    /// with [`WsIoServer::new_namespace_builder`].
    pub fn middleware_execution_timeout(mut self, duration: Duration) -> Self {
        self.config.middleware_execution_timeout = duration;
        self
    }

    /// Sets the default maximum duration for per-connection close handlers.
    ///
    /// This applies to handlers registered through
    /// [`on_close`](crate::connection::WsIoServerConnection::on_close).
    pub fn on_close_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.on_close_handler_timeout = duration;
        self
    }

    /// Sets the default maximum duration for namespace on-connect handlers.
    ///
    /// This limits the on-connect callback configured on namespace builders created
    /// with [`WsIoServer::new_namespace_builder`].
    pub fn on_connect_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.on_connect_handler_timeout = duration;
        self
    }

    /// Sets the default packet codec for namespaces.
    ///
    /// The codec handles ws.io protocol packets and init payload data. Namespace
    /// builders inherit this value and may override it.
    pub fn packet_codec(mut self, packet_codec: WsIoPacketCodec) -> Self {
        self.config.packet_codec = packet_codec;
        self
    }

    /// Sets the default packet transformer for namespaces.
    ///
    /// A namespace builder may override it for that namespace only.
    pub fn packet_transformer(mut self, packet_transformer: WsIoPacketTransformer) -> Self {
        self.config.packet_transformer = packet_transformer;
        self
    }

    /// Sets the HTTP request path handled by the server adapter, normalized and validated with [`resolve_path`].
    ///
    /// Requests with a different URI path pass through to the wrapped service.
    /// Client namespace selection is carried separately in the `namespace` query
    /// parameter.
    ///
    /// # Errors
    ///
    /// Returns an error if the path contains unsupported characters or `.` or `..` segments.
    pub fn request_path(mut self, request_path: impl AsRef<str>) -> Result<Self> {
        self.config.request_path = resolve_path(request_path.as_ref())?;
        Ok(self)
    }

    /// Replaces the default Tungstenite WebSocket configuration.
    ///
    /// Namespace builders inherit this value. It controls transport limits and
    /// buffer sizes independently of [`Self::channel_capacity`].
    pub fn websocket_config(mut self, websocket_config: WebSocketConfig) -> Self {
        self.config.websocket_config = websocket_config;
        self
    }

    /// Mutates the current default Tungstenite WebSocket configuration in place.
    ///
    /// The closure `f` runs once synchronously with a mutable borrow of the configuration.
    /// This preserves fields that the closure does not change.
    pub fn websocket_config_mut<F: FnOnce(&mut WebSocketConfig)>(mut self, f: F) -> Self {
        f(&mut self.config.websocket_config);
        self
    }
}

#[cfg(test)]
mod tests;
