use std::time::Duration;

use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use crate::core::packet::codecs::WsIoPacketCodec;

// Structs
#[derive(Debug)]
pub(crate) struct WsIoServerConfig {
    /// Maximum number of namespace broadcast send operations to run at once.
    ///
    /// Higher values can improve fan-out throughput, but also increase the number of
    /// in-flight connection sends and memory pressure. This is passed to
    /// `StreamExt::for_each_concurrent`, where `0` is treated as no concurrency
    /// limit.
    ///
    /// Can be overridden by namespace-level configuration.
    pub(crate) broadcast_concurrency_limit: usize,

    /// Maximum duration allowed for an accepted HTTP request to finish the
    /// WebSocket upgrade.
    ///
    /// The timeout covers waiting on the HTTP adapter's upgrade future after the
    /// request path has matched this server.
    ///
    /// Can be overridden by namespace-level configuration.
    pub(crate) http_request_upgrade_timeout: Duration,

    /// Maximum duration allowed for the namespace init-request handler to execute.
    ///
    /// The handler is configured with `WsIoServerNamespaceBuilder::with_init_request`
    /// and may return optional data that is encoded and sent to the client during
    /// the connection handshake.
    ///
    /// Can be overridden by namespace-level configuration.
    pub(crate) init_request_handler_timeout: Duration,

    /// Maximum duration allowed for the namespace init-response handler to execute.
    ///
    /// The handler is configured with `WsIoServerNamespaceBuilder::with_init_response`
    /// and receives the optional client response data after it is decoded with
    /// `packet_codec`.
    ///
    /// Can be overridden by namespace-level configuration.
    pub(crate) init_response_handler_timeout: Duration,

    /// Maximum duration to wait for the client to send the init-response packet.
    ///
    /// This applies after the server sends its init packet. If the client does not
    /// answer before the timeout, the handshake is treated as failed.
    ///
    /// Can be overridden by namespace-level configuration.
    pub(crate) init_response_timeout: Duration,

    /// Maximum duration allowed for namespace middleware execution.
    ///
    /// Middleware is configured with `WsIoServerNamespaceBuilder::with_middleware`
    /// and runs during connection setup before the on-connect handler.
    ///
    /// Can be overridden by namespace-level configuration.
    pub(crate) middleware_execution_timeout: Duration,

    /// Maximum duration allowed for a connection's on-close handler to execute.
    ///
    /// This applies to handlers registered from `WsIoServerConnection::on_close`.
    ///
    /// Can be overridden by namespace-level configuration.
    pub(crate) on_close_handler_timeout: Duration,

    /// Maximum duration allowed for the namespace on-connect handler to execute.
    ///
    /// The on-connect handler is configured with
    /// `WsIoServerNamespaceBuilder::on_connect` and runs during connection setup
    /// after middleware.
    ///
    /// Can be overridden by namespace-level configuration.
    pub(crate) on_connect_handler_timeout: Duration,

    /// Packet codec used to encode and decode ws.io protocol packets.
    ///
    /// The same codec must be understood by the client. It also controls whether
    /// encoded packets are sent as WebSocket text or binary messages.
    ///
    /// Can be overridden by namespace-level configuration.
    pub(crate) packet_codec: WsIoPacketCodec,

    /// HTTP request path handled by the server adapter.
    ///
    /// Requests whose URI path does not match this value pass through to the
    /// wrapped service. Client namespace selection is carried separately in the
    /// `namespace` query parameter.
    pub(crate) request_path: String,

    /// Tungstenite WebSocket transport limits and buffer sizes.
    ///
    /// This config is passed to `WebSocketStream::from_raw_socket` and is also
    /// used to size internal connection channels from the configured
    /// max-write/write-buffer ratio.
    ///
    /// Can be overridden by namespace-level configuration.
    pub(crate) websocket_config: WebSocketConfig,
}
