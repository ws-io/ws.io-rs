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
    core::packet::codecs::WsIoPacketCodec,
    runtime::WsIoServerRuntime,
};

// Structs

/// Builder for configuring and registering a server namespace.
///
/// Namespace builders inherit server-level defaults when they are created. The
/// methods on this builder override those values for this namespace only.
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
                http_request_upgrade_timeout: runtime.config.http_request_upgrade_timeout,
                init_request_handler: None,
                init_request_handler_timeout: runtime.config.init_request_handler_timeout,
                init_response_handler: None,
                init_response_handler_timeout: runtime.config.init_response_handler_timeout,
                init_response_timeout: runtime.config.init_response_timeout,
                middleware: None,
                middleware_execution_timeout: runtime.config.middleware_execution_timeout,
                on_connect_handler: None,
                on_close_handler_timeout: runtime.config.on_close_handler_timeout,
                on_connect_handler_timeout: runtime.config.on_connect_handler_timeout,
                on_ready_handler: None,
                packet_codec: runtime.config.packet_codec,
                path: path.into(),
                websocket_config: runtime.config.websocket_config,
            },
            runtime,
        }
    }

    // Public methods
    /// Sets the maximum number of broadcast send operations to run at once.
    ///
    /// This value is passed to `StreamExt::for_each_concurrent`; `0` is treated
    /// as no concurrency limit.
    pub fn broadcast_concurrency_limit(mut self, broadcast_concurrency_limit: usize) -> Self {
        self.config.broadcast_concurrency_limit = broadcast_concurrency_limit;
        self
    }

    /// Sets how long a matched HTTP request may take to finish the WebSocket
    /// upgrade.
    ///
    /// The timeout wraps the HTTP adapter's upgrade future before the namespace
    /// creates the WebSocket stream.
    pub fn http_request_upgrade_timeout(mut self, duration: Duration) -> Self {
        self.config.http_request_upgrade_timeout = duration;
        self
    }

    /// Sets the maximum duration allowed for the init-request handler to run.
    ///
    /// The init-request handler is registered with [`Self::with_init_request`]
    /// and runs before the server sends its init packet.
    pub fn init_request_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.init_request_handler_timeout = duration;
        self
    }

    /// Sets the maximum duration allowed for the init-response handler to run.
    ///
    /// The init-response handler is registered with [`Self::with_init_response`]
    /// and runs after the client sends its init response.
    pub fn init_response_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.init_response_handler_timeout = duration;
        self
    }

    /// Sets how long the server waits for the client init-response packet.
    ///
    /// This timeout starts after the server sends its init packet. If the client
    /// does not respond in time, the connection is closed.
    pub fn init_response_timeout(mut self, duration: Duration) -> Self {
        self.config.init_response_timeout = duration;
        self
    }

    /// Sets the maximum duration allowed for namespace middleware to run.
    ///
    /// Middleware is registered with [`Self::with_middleware`] and runs during
    /// connection setup before the on-connect handler.
    pub fn middleware_execution_timeout(mut self, duration: Duration) -> Self {
        self.config.middleware_execution_timeout = duration;
        self
    }

    /// Sets the maximum duration allowed for per-connection close handlers.
    ///
    /// This applies to handlers registered through `WsIoServerConnection::on_close`.
    pub fn on_close_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.on_close_handler_timeout = duration;
        self
    }

    /// Registers a namespace on-connect handler.
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

    /// Sets the maximum duration allowed for the namespace on-connect handler.
    pub fn on_connect_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.on_connect_handler_timeout = duration;
        self
    }

    /// Registers a namespace on-ready handler.
    ///
    /// The handler is spawned asynchronously after the connection is inserted,
    /// marked ready, and the ready packet is sent.
    pub fn on_ready<H, Fut>(mut self, handler: H) -> Self
    where
        H: Fn(Arc<WsIoServerConnection>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        self.config.on_ready_handler = Some(Arc::new(move |connection| Box::pin(handler(connection))));
        self
    }

    /// Sets the packet codec used by this namespace.
    ///
    /// The codec is used for ws.io protocol packets and for init payload
    /// serialization/deserialization. It must match the clients that connect to
    /// this namespace.
    pub fn packet_codec(mut self, packet_codec: WsIoPacketCodec) -> Self {
        self.config.packet_codec = packet_codec;
        self
    }

    /// Registers the namespace with the owning server runtime.
    ///
    /// Returns an error if another namespace with the same path is already
    /// registered.
    pub fn register(self) -> Result<Arc<WsIoServerNamespace>> {
        let namespace = WsIoServerNamespace::new(self.config, self.runtime.clone());
        self.runtime.insert_namespace(namespace.clone())?;
        Ok(namespace)
    }

    /// Replaces the full Tungstenite WebSocket configuration for this namespace.
    ///
    /// This controls transport limits and buffer sizes passed to the server-side
    /// WebSocket stream. It is also used to derive internal channel capacity from
    /// the configured max-write/write-buffer ratio.
    pub fn websocket_config(mut self, websocket_config: WebSocketConfig) -> Self {
        self.config.websocket_config = websocket_config;
        self
    }

    /// Mutates the current Tungstenite WebSocket configuration in place.
    ///
    /// Prefer this when you want to adjust one or two fields while keeping the
    /// inherited server defaults for the rest.
    pub fn websocket_config_mut<F: FnOnce(&mut WebSocketConfig)>(mut self, f: F) -> Self {
        f(&mut self.config.websocket_config);
        self
    }

    /// Registers namespace middleware for connection setup.
    ///
    /// Middleware runs after init-response handling and before the on-connect
    /// handler. Returning an error aborts setup for that connection.
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
            let handler = handler.clone();
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
            let handler = handler.clone();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WsIoServer;

    #[tokio::test]
    async fn test_namespace_builder_configuration() {
        let server = Arc::new(WsIoServer::builder().build());
        let builder = WsIoServerNamespaceBuilder::new("/custom", server.0.clone())
            .broadcast_concurrency_limit(42)
            .http_request_upgrade_timeout(Duration::from_millis(750))
            .init_request_handler_timeout(Duration::from_secs(1))
            .init_response_handler_timeout(Duration::from_secs(2))
            .init_response_timeout(Duration::from_secs(3))
            .middleware_execution_timeout(Duration::from_secs(4))
            .on_close_handler_timeout(Duration::from_secs(5))
            .on_connect_handler_timeout(Duration::from_secs(6))
            .packet_codec(WsIoPacketCodec::Msgpack)
            .websocket_config(WebSocketConfig::default().max_frame_size(Some(777)))
            .websocket_config_mut(|config| {
                *config = config.max_frame_size(Some(888));
            });

        let config = &builder.config;
        assert_eq!(config.path, "/custom");
        assert_eq!(config.broadcast_concurrency_limit, 42);
        assert_eq!(config.http_request_upgrade_timeout, Duration::from_millis(750));
        assert_eq!(config.init_request_handler_timeout, Duration::from_secs(1));
        assert_eq!(config.init_response_handler_timeout, Duration::from_secs(2));
        assert_eq!(config.init_response_timeout, Duration::from_secs(3));
        assert_eq!(config.middleware_execution_timeout, Duration::from_secs(4));
        assert_eq!(config.on_close_handler_timeout, Duration::from_secs(5));
        assert_eq!(config.on_connect_handler_timeout, Duration::from_secs(6));
        assert!(matches!(config.packet_codec, WsIoPacketCodec::Msgpack));
        assert_eq!(config.websocket_config.max_frame_size, Some(888));
    }

    #[tokio::test]
    async fn test_namespace_builder_registers_lifecycle_handlers() {
        let server = Arc::new(WsIoServer::builder().build());
        let builder = WsIoServerNamespaceBuilder::new("/custom", server.0.clone())
            .on_connect(|_connection| async { Ok(()) })
            .on_ready(|_connection| async { Ok(()) })
            .with_middleware(|_connection| async { Ok(()) })
            .with_init_request(|_connection| async { Ok(Some("request".to_string())) })
            .with_init_response(|_connection, _data: Option<String>| async { Ok(()) });

        assert!(builder.config.on_connect_handler.is_some());
        assert!(builder.config.on_ready_handler.is_some());
        assert!(builder.config.middleware.is_some());
        assert!(builder.config.init_request_handler.is_some());
        assert!(builder.config.init_response_handler.is_some());
    }

    #[test]
    fn test_namespace_duplicate_registration_fails() {
        let server = Arc::new(WsIoServer::builder().build());

        let builder1 = WsIoServerNamespaceBuilder::new("/socket", server.0.clone());
        let register1_result = builder1.register();
        assert!(
            register1_result.is_ok(),
            "First namespace component should register successfully"
        );

        // Attempting to register the same path should yield an Err
        let builder2 = WsIoServerNamespaceBuilder::new("/socket", server.0.clone());
        let register2_result = builder2.register();
        assert!(
            register2_result.is_err(),
            "Duplicate namespace registration should fail on Runtime"
        );

        match register2_result {
            Err(e) => {
                assert!(e.to_string().contains("already exists"));
            },
            Ok(_) => panic!("Should have failed"),
        }
    }
}
