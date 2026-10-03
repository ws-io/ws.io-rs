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
    /// Higher values allow more in-flight sends and can increase memory pressure.
    /// `0` means unlimited concurrency.
    ///
    /// Namespace builders may override this value.
    ///
    /// Defaults to `512`.
    pub(crate) broadcast_concurrency_limit: usize,

    /// Maximum duration for an accepted HTTP request's WebSocket upgrade.
    ///
    /// This covers the HTTP adapter's upgrade future after the request path
    /// matches this server.
    ///
    /// Namespace builders may override this value.
    ///
    /// Defaults to 3 seconds.
    /// `Duration::ZERO` does not disable the timeout.
    pub(crate) http_request_upgrade_timeout: Duration,

    /// Maximum duration for the namespace init-request handler.
    ///
    /// The handler is configured with
    /// [`with_init_request`](crate::namespace::builder::WsIoServerNamespaceBuilder::with_init_request) and may return
    /// optional
    /// data for the connection handshake.
    ///
    /// Namespace builders may override this value.
    ///
    /// Defaults to 3 seconds.
    /// `Duration::ZERO` does not disable the timeout.
    pub(crate) init_request_handler_timeout: Duration,

    /// Maximum duration for the namespace init-response handler.
    ///
    /// The handler is configured with
    /// [`with_init_response`](crate::namespace::builder::WsIoServerNamespaceBuilder::with_init_response) and receives
    /// the optional
    /// client response data decoded with `packet_codec`.
    ///
    /// Namespace builders may override this value.
    ///
    /// Defaults to 3 seconds.
    /// `Duration::ZERO` does not disable the timeout.
    pub(crate) init_response_handler_timeout: Duration,

    /// Maximum duration for waiting for the client init-response packet.
    ///
    /// This starts when the connection begins waiting for the client response,
    /// before the server init packet is queued for sending. If the client does not
    /// answer in time, the handshake fails.
    ///
    /// Namespace builders may override this value.
    ///
    /// Defaults to 5 seconds.
    /// `Duration::ZERO` does not disable the timeout.
    pub(crate) init_response_timeout: Duration,

    /// Maximum duration for namespace middleware execution.
    ///
    /// Middleware is configured with
    /// [`with_middleware`](crate::namespace::builder::WsIoServerNamespaceBuilder::with_middleware) and runs during
    /// connection
    /// setup before the on-connect handler.
    ///
    /// Namespace builders may override this value.
    ///
    /// Defaults to 2 seconds.
    /// `Duration::ZERO` does not disable the timeout.
    pub(crate) middleware_execution_timeout: Duration,

    /// Maximum duration for a connection's on-close handler.
    ///
    /// This applies to handlers registered with [`on_close`](crate::connection::WsIoServerConnection::on_close).
    ///
    /// Namespace builders may override this value.
    ///
    /// Defaults to 2 seconds.
    /// `Duration::ZERO` does not disable the timeout.
    pub(crate) on_close_handler_timeout: Duration,

    /// Maximum duration for the namespace on-connect handler.
    ///
    /// The handler is configured with [`on_connect`](crate::namespace::builder::WsIoServerNamespaceBuilder::on_connect)
    /// and runs during connection setup after middleware.
    ///
    /// Namespace builders may override this value.
    ///
    /// Defaults to 3 seconds.
    /// `Duration::ZERO` does not disable the timeout.
    pub(crate) on_connect_handler_timeout: Duration,

    /// Packet codec for ws.io protocol packets.
    ///
    /// The codec must match the client. All supported codecs use binary WebSocket
    /// messages.
    ///
    /// Namespace builders may override this value.
    ///
    /// Defaults to [`WsIoPacketCodec::Msgpack`].
    pub(crate) packet_codec: WsIoPacketCodec,

    /// Transformer applied to complete encoded WebSocket packets.
    ///
    /// Namespace builders may override this value.
    ///
    /// Defaults to the no-op transformer.
    pub(crate) packet_transformer: WsIoPacketTransformer,

    /// HTTP request path handled by the server adapter.
    ///
    /// Requests with a different URI path pass through to the wrapped service.
    /// Client namespace selection is carried separately in the `namespace` query
    /// parameter.
    ///
    /// Defaults to `"/ws.io"`.
    pub(crate) request_path: String,

    /// Tungstenite WebSocket transport limits and buffer sizes.
    ///
    /// The configuration is passed to [`tokio_tungstenite::WebSocketStream::from_raw_socket`] and
    /// derives internal connection channel capacity from the configured
    /// max-write/write-buffer ratio.
    ///
    /// Namespace builders may override this value.
    ///
    /// The defaults are 8 MiB per frame, 16 MiB per message, a 2 MiB maximum write
    /// buffer, and 8 KiB read and write buffers; other settings use
    /// [`WebSocketConfig::default`].
    pub(crate) websocket_config: WebSocketConfig,
}
