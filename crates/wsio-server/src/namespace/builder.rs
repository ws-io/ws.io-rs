use std::{
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use serde::{
    Serialize,
    de::DeserializeOwned,
};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use super::{
    WsIoServerNamespace,
    config::WsIoServerNamespaceConfig,
};
use crate::{
    connection::WsIoServerConnection,
    core::{
        packet::{
            codecs::WsIoPacketCodec,
            transformers::WsIoPacketTransformer,
        },
        transport::validate_transport_config,
        utils::normalize_path,
    },
    runtime::WsIoServerRuntime,
};

// Structs

/// A builder for configuring and registering a server namespace.
///
/// Namespace builders inherit server-level defaults when created. Their methods
/// override those values for this namespace only.
///
/// Configuration methods consume the builder and return the updated builder.
/// Registering a callback replaces the previous callback of that kind. Callbacks
/// return asynchronous work and may run concurrently for different connections.
/// No ordering is guaranteed across connections. A timeout drops unfinished callback
/// work without rolling back completed side effects.
#[derive(Debug)]
pub struct WsIoServerNamespaceBuilder {
    config: WsIoServerNamespaceConfig,
    runtime: Arc<WsIoServerRuntime>,
}

impl WsIoServerNamespaceBuilder {
    pub(crate) fn new(path: &str, runtime: Arc<WsIoServerRuntime>) -> Self {
        Self {
            config: WsIoServerNamespaceConfig {
                broadcast_concurrency_limit: runtime.config.broadcast_concurrency_limit,
                channel_capacity: runtime.config.channel_capacity,
                connection_idle_timeout: runtime.config.connection_idle_timeout,
                disconnect_timeout: runtime.config.disconnect_timeout,
                http_request_upgrade_timeout: runtime.config.http_request_upgrade_timeout,
                init_request_handler: None,
                init_request_handler_timeout: runtime.config.init_request_handler_timeout,
                init_response_handler: None,
                init_response_handler_timeout: runtime.config.init_response_handler_timeout,
                init_response_timeout: runtime.config.init_response_timeout,
                max_outgoing_message_size: runtime.config.max_outgoing_message_size,
                middleware: None,
                middleware_execution_timeout: runtime.config.middleware_execution_timeout,
                on_connect_handler: None,
                on_close_handler_timeout: runtime.config.on_close_handler_timeout,
                on_connect_handler_timeout: runtime.config.on_connect_handler_timeout,
                on_ready_handler: None,
                packet_codec: runtime.config.packet_codec.clone(),
                packet_transformer: runtime.config.packet_transformer.clone(),
                path: normalize_path(path),
                websocket_config: runtime.config.websocket_config,
            },
            runtime,
        }
    }

    // Public methods

    /// Sets the maximum number of concurrent broadcast sends.
    ///
    /// `0` means unlimited concurrency.
    pub fn broadcast_concurrency_limit(mut self, broadcast_concurrency_limit: usize) -> Self {
        self.config.broadcast_concurrency_limit = broadcast_concurrency_limit;
        self
    }

    /// Sets the message capacity of each internal connection channel.
    ///
    /// Inherits the server default of 2048. Must be between 1 and
    /// [`tokio::sync::Semaphore::MAX_PERMITS`]. The limit counts messages, not bytes.
    pub fn channel_capacity(mut self, channel_capacity: usize) -> Self {
        self.config.channel_capacity = channel_capacity;
        self
    }

    /// Sets the maximum duration for waiting for the next WebSocket message on a ready connection.
    ///
    /// Packet processing time is excluded. `None` disables the timeout.
    /// `Some(Duration::ZERO)` does not disable the timeout.
    pub fn connection_idle_timeout(mut self, duration: Option<Duration>) -> Self {
        self.config.connection_idle_timeout = duration;
        self
    }

    /// Sets the timeout for each disconnect phase: preparing and queueing the protocol
    /// packet, then waiting for transport shutdown after cancellation.
    ///
    /// Inherits the server default of 5 seconds. Cleanup callbacks have their own timeouts.
    /// A disconnect may take up to two phases; zero requests no grace period.
    pub fn disconnect_timeout(mut self, duration: Duration) -> Self {
        self.config.disconnect_timeout = duration;
        self
    }

    /// Sets the maximum duration for a matched HTTP request's WebSocket upgrade.
    ///
    /// The timeout wraps the HTTP adapter's upgrade future before the namespace
    /// creates the WebSocket stream.
    pub fn http_request_upgrade_timeout(mut self, duration: Duration) -> Self {
        self.config.http_request_upgrade_timeout = duration;
        self
    }

    /// Sets the maximum duration for the init-request handler.
    ///
    /// The handler is registered with [`Self::with_init_request`] and runs before
    /// the server sends its init packet.
    pub fn init_request_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.init_request_handler_timeout = duration;
        self
    }

    /// Sets the maximum duration for the init-response handler.
    ///
    /// The handler is registered with [`Self::with_init_response`] and runs after
    /// the client sends its init response.
    pub fn init_response_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.init_response_handler_timeout = duration;
        self
    }

    /// Sets the maximum duration for waiting for the client init-response packet.
    ///
    /// This starts when the connection begins waiting for the client response,
    /// before the server init packet is queued for sending. If the client does not
    /// respond in time, the connection closes.
    pub fn init_response_timeout(mut self, duration: Duration) -> Self {
        self.config.init_response_timeout = duration;
        self
    }

    /// Sets the maximum outgoing Binary payload size after packet transformation, excluding frame headers.
    ///
    /// Inherits the server default of 2 MiB; zero is rejected. Oversized packets
    /// are rejected before queueing. Registration requires a write buffer of at
    /// least this limit plus the write threshold and [`OUTGOING_FRAME_OVERHEAD`](crate::core::transport::OUTGOING_FRAME_OVERHEAD) bytes.
    /// Messages are not fragmented, so the peer's frame limit must accommodate them.
    /// Large broadcasts can leave a large retained write buffer on each connection.
    pub fn max_outgoing_message_size(mut self, max_outgoing_message_size: usize) -> Self {
        self.config.max_outgoing_message_size = max_outgoing_message_size;
        self
    }

    /// Sets the maximum duration for namespace middleware.
    ///
    /// Middleware is registered with [`Self::with_middleware`] and runs during
    /// connection setup before the on-connect handler.
    pub fn middleware_execution_timeout(mut self, duration: Duration) -> Self {
        self.config.middleware_execution_timeout = duration;
        self
    }

    /// Sets the maximum duration for per-connection close handlers.
    ///
    /// This applies to handlers registered with [`on_close`](crate::connection::WsIoServerConnection::on_close).
    pub fn on_close_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.on_close_handler_timeout = duration;
        self
    }

    /// Registers the namespace on-connect handler.
    ///
    /// The handler receives the connection once when setup reaches this stage. Its
    /// future is awaited within [`Self::on_connect_handler_timeout`]. A returned
    /// error or timeout aborts connection setup.
    ///
    /// The handler runs during connection setup after middleware and before the
    /// connection is inserted into the namespace and marked ready.
    pub fn on_connect<H, Fut>(mut self, handler: H) -> Self
    where
        H: Fn(Arc<WsIoServerConnection>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        self.config.on_connect_handler = Some(Box::new(move |connection| Box::pin(handler(connection))));
        self
    }

    /// Sets the maximum duration for the namespace on-connect handler.
    pub fn on_connect_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.on_connect_handler_timeout = duration;
        self
    }

    /// Registers the namespace on-ready handler.
    ///
    /// The handler receives the ready connection once in a background task.
    /// Connection cancellation drops unfinished work. Returned errors are logged
    /// when the `tracing` feature is enabled and do not undo connection setup.
    ///
    /// The handler is spawned after the connection is inserted, marked ready, and
    /// the ready packet is queued for sending.
    pub fn on_ready<H, Fut>(mut self, handler: H) -> Self
    where
        H: Fn(Arc<WsIoServerConnection>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        self.config.on_ready_handler = Some(Arc::new(move |connection| Box::pin(handler(connection))));
        self
    }

    /// Sets the packet codec for this namespace.
    ///
    /// The codec handles ws.io protocol packets and init payload data. It must
    /// match the clients that connect to this namespace.
    pub fn packet_codec(mut self, packet_codec: WsIoPacketCodec) -> Self {
        self.config.packet_codec = packet_codec;
        self
    }

    /// Sets the packet transformer for this namespace.
    ///
    /// This overrides the transformer inherited from the server builder.
    pub fn packet_transformer(mut self, packet_transformer: WsIoPacketTransformer) -> Self {
        self.config.packet_transformer = packet_transformer;
        self
    }

    /// Registers the namespace with its server runtime.
    ///
    /// Consumes the builder and returns a shared handle to the registered namespace.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is already registered or queue/outgoing-buffer settings are invalid.
    pub fn register(self) -> Result<Arc<WsIoServerNamespace>> {
        validate_transport_config(
            &self.config.websocket_config,
            self.config.channel_capacity,
            self.config.max_outgoing_message_size,
        )?;

        let namespace = WsIoServerNamespace::new(self.config, &self.runtime);
        self.runtime.insert_namespace(Arc::clone(&namespace))?;
        Ok(namespace)
    }

    /// Replaces the full Tungstenite WebSocket configuration for this namespace.
    ///
    /// The configuration controls transport limits and buffer sizes. Internal
    /// channel capacity is configured independently with [`Self::channel_capacity`].
    pub fn websocket_config(mut self, websocket_config: WebSocketConfig) -> Self {
        self.config.websocket_config = websocket_config;
        self
    }

    /// Mutates the current Tungstenite WebSocket configuration in place.
    ///
    /// The closure `f` runs once synchronously with a mutable borrow of the configuration.
    /// This preserves fields that the closure does not change.
    pub fn websocket_config_mut<F: FnOnce(&mut WebSocketConfig)>(mut self, f: F) -> Self {
        f(&mut self.config.websocket_config);
        self
    }

    /// Registers namespace middleware for connection setup.
    ///
    /// The middleware receives the connection once when setup reaches this stage.
    /// Its future is awaited within [`Self::middleware_execution_timeout`]. A timeout
    /// also aborts connection setup.
    ///
    /// Middleware runs after init-response handling and before the on-connect
    /// handler. Returning an error aborts connection setup.
    pub fn with_middleware<H, Fut>(mut self, handler: H) -> Self
    where
        H: Fn(Arc<WsIoServerConnection>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        self.config.middleware = Some(Box::new(move |connection| Box::pin(handler(connection))));
        self
    }

    /// Registers the server-side init-request handler.
    ///
    /// The handler runs once when connection initialization starts, before sending
    /// the server init packet. Its future is awaited within
    /// [`Self::init_request_handler_timeout`]. Returning `None` omits payload data.
    /// Handler errors, encoding errors, or a timeout abort connection setup.
    ///
    /// The handler receives the connection and may return optional data to encode
    /// as `D` and send in the server init packet.
    pub fn with_init_request<H, Fut, D>(mut self, handler: H) -> Self
    where
        H: Fn(Arc<WsIoServerConnection>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Option<D>>> + Send + 'static,
        D: Serialize + Send + 'static,
    {
        let handler = Arc::new(handler);
        self.config.init_request_handler = Some(Box::new(move |connection, packet_codec| {
            let handler = Arc::clone(&handler);
            Box::pin(async move {
                handler(connection)
                    .await?
                    .map(|data| packet_codec.encode_data(&data))
                    .transpose()
            })
        }));

        self
    }

    /// Registers the server-side init-response handler.
    ///
    /// The handler runs once for the client init response accepted during setup,
    /// before middleware and the on-connect handler. Its future is awaited within
    /// [`Self::init_response_handler_timeout`]. Missing payload data is passed as
    /// `None`. Decoding errors, handler errors, or a timeout abort connection setup.
    ///
    /// The handler receives the connection and the optional client init-response
    /// payload decoded as `D`.
    pub fn with_init_response<H, Fut, D>(mut self, handler: H) -> Self
    where
        H: Fn(Arc<WsIoServerConnection>, Option<D>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
        D: DeserializeOwned + Send + 'static,
    {
        let handler = Arc::new(handler);
        self.config.init_response_handler = Some(Box::new(move |connection, bytes, packet_codec| {
            let handler = Arc::clone(&handler);
            Box::pin(async move {
                handler(
                    connection,
                    bytes.map(|bytes| packet_codec.decode_data(bytes)).transpose()?,
                )
                .await
            })
        }));

        self
    }
}
