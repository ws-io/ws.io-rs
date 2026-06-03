use std::{
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use crate::{
    connection::WsIoServerConnection,
    core::{
        packet::codecs::WsIoPacketCodec,
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
        ) -> Pin<Box<dyn Future<Output = Result<Option<Vec<u8>>>> + Send + 'a>>
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
    /// Maximum number of broadcast send operations this namespace runs at once.
    ///
    /// Inherited from `WsIoServerConfig` when the namespace builder is created and
    /// overridable per namespace. This is passed to
    /// `StreamExt::for_each_concurrent`, where `0` is treated as no concurrency
    /// limit.
    pub(crate) broadcast_concurrency_limit: usize,

    /// Maximum duration allowed for a matched HTTP request to finish the
    /// WebSocket upgrade for this namespace.
    pub(super) http_request_upgrade_timeout: Duration,

    /// Optional server-side init-request handler for this namespace.
    ///
    /// When present, it runs during connection setup and may return optional data
    /// that is encoded with `packet_codec` and sent to the client as the init
    /// packet payload.
    pub(crate) init_request_handler: Option<InitRequestHandler>,

    /// Maximum duration allowed for `init_request_handler` to execute.
    pub(crate) init_request_handler_timeout: Duration,

    /// Optional server-side init-response handler for this namespace.
    ///
    /// When present, it receives the optional client response payload after
    /// decoding and runs before middleware/on-connect processing continues.
    pub(crate) init_response_handler: Option<InitResponseHandler>,

    /// Maximum duration allowed for `init_response_handler` to execute.
    pub(crate) init_response_handler_timeout: Duration,

    /// Maximum duration to wait for the client to send its init-response packet.
    pub(crate) init_response_timeout: Duration,

    /// Optional namespace middleware run during connection setup.
    ///
    /// Middleware runs after init-response handling and before the on-connect
    /// handler. Returning an error rejects/aborts the connection setup.
    pub(crate) middleware: Option<BoxAsyncUnaryResultHandler<WsIoServerConnection>>,

    /// Maximum duration allowed for `middleware` execution.
    pub(crate) middleware_execution_timeout: Duration,

    /// Maximum duration allowed for a connection's on-close handler to execute.
    pub(crate) on_close_handler_timeout: Duration,

    /// Optional namespace on-connect handler.
    ///
    /// Runs during setup after middleware and before the ready packet is sent.
    pub(crate) on_connect_handler: Option<BoxAsyncUnaryResultHandler<WsIoServerConnection>>,

    /// Maximum duration allowed for `on_connect_handler` execution.
    pub(crate) on_connect_handler_timeout: Duration,

    /// Optional namespace on-ready handler.
    ///
    /// Runs asynchronously after the connection has completed setup and has been
    /// marked ready. It is spawned instead of being awaited in the setup path.
    pub(crate) on_ready_handler: Option<ArcAsyncUnaryResultHandler<WsIoServerConnection>>,

    /// Packet codec used by this namespace for protocol packets and init data.
    pub(crate) packet_codec: WsIoPacketCodec,

    /// Namespace path used for routing clients from the `namespace` query
    /// parameter after the server request path is matched.
    pub(super) path: String,

    /// Tungstenite WebSocket transport limits and buffer sizes for this namespace.
    ///
    /// The namespace receives a copy of the server-level config when the builder is
    /// created, then may override it independently. It is also used to size
    /// internal connection channels from the configured max-write/write-buffer
    /// ratio.
    pub(crate) websocket_config: WebSocketConfig,
}
