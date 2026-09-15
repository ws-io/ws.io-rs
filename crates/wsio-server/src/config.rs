use std::time::Duration;

use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use crate::core::packet::{
    codecs::WsIoPacketCodec,
    transformers::WsIoPacketTransformer,
};

// Structs
#[derive(Debug)]
pub(crate) struct WsIoServerConfig {
    /// Maximum number of concurrent namespace broadcast sends.
    ///
    /// Higher values can improve fan-out throughput but increase in-flight sends
    /// and memory pressure. This is passed to
    /// `StreamExt::for_each_concurrent`; `0` means unlimited concurrency.
    ///
    /// Namespace builders may override this value.
    pub(crate) broadcast_concurrency_limit: usize,

    /// Maximum duration for an accepted HTTP request's WebSocket upgrade.
    ///
    /// This covers the HTTP adapter's upgrade future after the request path
    /// matches this server.
    ///
    /// Namespace builders may override this value.
    pub(crate) http_request_upgrade_timeout: Duration,

    /// Maximum duration for the namespace init-request handler.
    ///
    /// The handler is configured with
    /// `WsIoServerNamespaceBuilder::with_init_request` and may return optional
    /// data for the connection handshake.
    ///
    /// Namespace builders may override this value.
    pub(crate) init_request_handler_timeout: Duration,

    /// Maximum duration for the namespace init-response handler.
    ///
    /// The handler is configured with
    /// `WsIoServerNamespaceBuilder::with_init_response` and receives the optional
    /// client response data decoded with `packet_codec`.
    ///
    /// Namespace builders may override this value.
    pub(crate) init_response_handler_timeout: Duration,

    /// Maximum duration for waiting for the client init-response packet.
    ///
    /// This starts after the server sends its init packet. If the client does not
    /// answer in time, the handshake fails.
    ///
    /// Namespace builders may override this value.
    pub(crate) init_response_timeout: Duration,

    /// Maximum duration for namespace middleware execution.
    ///
    /// Middleware is configured with
    /// `WsIoServerNamespaceBuilder::with_middleware` and runs during connection
    /// setup before the on-connect handler.
    ///
    /// Namespace builders may override this value.
    pub(crate) middleware_execution_timeout: Duration,

    /// Maximum duration for a connection's on-close handler.
    ///
    /// This applies to handlers registered with `WsIoServerConnection::on_close`.
    ///
    /// Namespace builders may override this value.
    pub(crate) on_close_handler_timeout: Duration,

    /// Maximum duration for the namespace on-connect handler.
    ///
    /// The handler is configured with `WsIoServerNamespaceBuilder::on_connect`
    /// and runs during connection setup after middleware.
    ///
    /// Namespace builders may override this value.
    pub(crate) on_connect_handler_timeout: Duration,

    /// Packet codec for ws.io protocol packets.
    ///
    /// The codec must match the client. All supported codecs use binary WebSocket
    /// messages.
    ///
    /// Namespace builders may override this value.
    pub(crate) packet_codec: WsIoPacketCodec,

    /// Transformer applied to complete encoded WebSocket packets.
    ///
    /// Namespace builders may override this value.
    pub(crate) packet_transformer: WsIoPacketTransformer,

    /// HTTP request path handled by the server adapter.
    ///
    /// Requests with a different URI path pass through to the wrapped service.
    /// Client namespace selection is carried separately in the `namespace` query
    /// parameter.
    pub(crate) request_path: String,

    /// Tungstenite WebSocket transport limits and buffer sizes.
    ///
    /// The configuration is passed to `WebSocketStream::from_raw_socket` and
    /// derives internal connection channel capacity from the configured
    /// max-write/write-buffer ratio.
    ///
    /// Namespace builders may override this value.
    pub(crate) websocket_config: WebSocketConfig,
}
