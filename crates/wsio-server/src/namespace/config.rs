use std::{
    fmt::{
        Debug as FmtDebug,
        Formatter,
        Result as FmtResult,
    },
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use bytes::Bytes;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use crate::{
    connection::WsIoServerConnection,
    core::{
        packet::{
            codecs::WsIoPacketCodec,
            transformers::WsIoPacketTransformer,
        },
        types::{
            ArcAsyncUnaryResultHandler,
            BoxAsyncUnaryResultHandler,
        },
    },
};

// Types
type InitRequestHandler = Box<
    dyn for<'a> Fn(
            Arc<WsIoServerConnection>,
            &'a WsIoPacketCodec,
        ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>>> + Send + 'a>>
        + Send
        + Sync
        + 'static,
>;

type InitResponseHandler = Box<
    dyn for<'a> Fn(
            Arc<WsIoServerConnection>,
            Option<&'a [u8]>,
            &'a WsIoPacketCodec,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
        + Send
        + Sync
        + 'static,
>;

// Structs
pub(crate) struct WsIoServerNamespaceConfig {
    /// Maximum number of concurrent broadcast sends for this namespace.
    ///
    /// This is inherited from `WsIoServerConfig` and passed to
    /// `StreamExt::for_each_concurrent`; `0` means unlimited concurrency.
    pub(crate) broadcast_concurrency_limit: usize,

    /// Maximum duration for a matched HTTP request's WebSocket upgrade.
    pub(super) http_request_upgrade_timeout: Duration,

    /// Optional server-side init-request handler.
    ///
    /// When present, it runs during connection setup and may return optional data
    /// encoded with `packet_codec` for the client init packet.
    pub(crate) init_request_handler: Option<InitRequestHandler>,

    /// Maximum duration for `init_request_handler`.
    pub(crate) init_request_handler_timeout: Duration,

    /// Optional server-side init-response handler.
    ///
    /// When present, it receives the decoded optional client response payload and
    /// runs before middleware and on-connect processing.
    pub(crate) init_response_handler: Option<InitResponseHandler>,

    /// Maximum duration for `init_response_handler`.
    pub(crate) init_response_handler_timeout: Duration,

    /// Maximum duration for waiting for the client init-response packet.
    pub(crate) init_response_timeout: Duration,

    /// Optional namespace middleware for connection setup.
    ///
    /// Middleware runs after init-response handling and before the on-connect
    /// handler. Returning an error aborts connection setup.
    pub(crate) middleware: Option<BoxAsyncUnaryResultHandler<WsIoServerConnection>>,

    /// Maximum duration for `middleware`.
    pub(crate) middleware_execution_timeout: Duration,

    /// Maximum duration for a connection's on-close handler.
    pub(crate) on_close_handler_timeout: Duration,

    /// Optional namespace on-connect handler.
    ///
    /// Runs during setup after middleware and before the ready packet is sent.
    pub(crate) on_connect_handler: Option<BoxAsyncUnaryResultHandler<WsIoServerConnection>>,

    /// Maximum duration for `on_connect_handler`.
    pub(crate) on_connect_handler_timeout: Duration,

    /// Optional namespace on-ready handler.
    ///
    /// Runs after connection setup completes and the connection is marked ready.
    /// It is spawned instead of being awaited in the setup path.
    pub(crate) on_ready_handler: Option<ArcAsyncUnaryResultHandler<WsIoServerConnection>>,

    /// Packet codec for this namespace's protocol packets and init data.
    pub(crate) packet_codec: WsIoPacketCodec,

    /// Transformer for this namespace's complete encoded WebSocket packets.
    pub(crate) packet_transformer: WsIoPacketTransformer,

    /// Namespace path used to route clients from the `namespace` query parameter
    /// after the server request path matches.
    pub(super) path: String,

    /// Tungstenite WebSocket transport limits and buffer sizes.
    ///
    /// The namespace receives a copy of the server-level configuration and may
    /// override it independently. It also derives internal connection channel
    /// capacity from the configured max-write/write-buffer ratio.
    pub(crate) websocket_config: WebSocketConfig,
}

impl FmtDebug for WsIoServerNamespaceConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("WsIoServerNamespaceConfig")
            .field("path", &self.path)
            .field("broadcast_concurrency_limit", &self.broadcast_concurrency_limit)
            .field("http_request_upgrade_timeout", &self.http_request_upgrade_timeout)
            .field(
                "init_request_handler",
                &self.init_request_handler.as_ref().map(|_| "<handler>"),
            )
            .field("init_request_handler_timeout", &self.init_request_handler_timeout)
            .field(
                "init_response_handler",
                &self.init_response_handler.as_ref().map(|_| "<handler>"),
            )
            .field("init_response_handler_timeout", &self.init_response_handler_timeout)
            .field("init_response_timeout", &self.init_response_timeout)
            .field("middleware", &self.middleware.as_ref().map(|_| "<handler>"))
            .field("middleware_execution_timeout", &self.middleware_execution_timeout)
            .field("on_close_handler_timeout", &self.on_close_handler_timeout)
            .field(
                "on_connect_handler",
                &self.on_connect_handler.as_ref().map(|_| "<handler>"),
            )
            .field("on_connect_handler_timeout", &self.on_connect_handler_timeout)
            .field("on_ready_handler", &self.on_ready_handler.as_ref().map(|_| "<handler>"))
            .field("packet_codec", &self.packet_codec)
            .field("packet_transformer", &"<transformer>")
            .field("websocket_config", &self.websocket_config)
            .finish()
    }
}
