use std::{
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use bytes::Bytes;
use derive_more::Debug;
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
#[derive(Debug)]
pub(crate) struct WsIoServerNamespaceConfig {
    /// Maximum number of concurrent broadcast sends for this namespace.
    ///
    /// `0` means unlimited concurrency.
    ///
    /// Defaults to the value inherited from the server builder.
    pub(crate) broadcast_concurrency_limit: usize,

    /// Number of messages permitted in each connection channel.
    ///
    /// Defaults to the value inherited from the server builder.
    pub(crate) channel_capacity: usize,

    /// Maximum duration for waiting for the next WebSocket message on a ready connection.
    ///
    /// Packet processing time is excluded. `None` disables the timeout.
    /// `Some(Duration::ZERO)` does not disable the timeout.
    /// Defaults to the value inherited from the server builder.
    pub(super) connection_idle_timeout: Option<Duration>,

    /// Maximum duration of each disconnect phase: preparing/queueing the protocol packet,
    /// then waiting for transport shutdown after cancellation. Cleanup callbacks are excluded.
    ///
    /// Defaults to the value inherited from the server builder.
    pub(crate) disconnect_timeout: Duration,

    /// Maximum duration for a matched HTTP request's WebSocket upgrade.
    pub(super) http_request_upgrade_timeout: Duration,

    /// Optional server-side init-request handler.
    ///
    /// When present, it runs during connection setup and may return optional data
    /// encoded with `packet_codec` for the client init packet.
    ///
    /// Defaults to `None`.
    #[debug("{:?}", init_request_handler.as_ref().map(|_| "<handler>"))]
    pub(crate) init_request_handler: Option<InitRequestHandler>,

    /// Maximum duration for `init_request_handler`.
    ///
    /// Defaults to the value inherited from the server builder.
    pub(crate) init_request_handler_timeout: Duration,

    /// Optional server-side init-response handler.
    ///
    /// When present, it receives the decoded optional client response payload and
    /// runs before middleware and on-connect processing.
    ///
    /// Defaults to `None`.
    #[debug("{:?}", init_response_handler.as_ref().map(|_| "<handler>"))]
    pub(crate) init_response_handler: Option<InitResponseHandler>,

    /// Maximum duration for `init_response_handler`.
    ///
    /// Defaults to the value inherited from the server builder.
    pub(crate) init_response_handler_timeout: Duration,

    /// Maximum duration for waiting for the client init-response packet.
    ///
    /// Defaults to the value inherited from the server builder.
    pub(crate) init_response_timeout: Duration,

    /// Maximum outgoing Binary payload size after packet transformation, excluding frame headers.
    ///
    /// Defaults to the value inherited from the server builder.
    pub(crate) max_outgoing_message_size: usize,

    /// Optional namespace middleware for connection setup.
    ///
    /// Middleware runs after init-response handling and before the on-connect
    /// handler. Returning an error aborts connection setup.
    ///
    /// Defaults to `None`.
    #[debug("{:?}", middleware.as_ref().map(|_| "<handler>"))]
    pub(crate) middleware: Option<BoxAsyncUnaryResultHandler<WsIoServerConnection>>,

    /// Maximum duration for `middleware`.
    ///
    /// Defaults to the value inherited from the server builder.
    pub(crate) middleware_execution_timeout: Duration,

    /// Maximum duration for a connection's on-close handler.
    ///
    /// Defaults to the value inherited from the server builder.
    pub(crate) on_close_handler_timeout: Duration,

    /// Optional namespace on-connect handler.
    ///
    /// Runs during setup after middleware and before the ready packet is sent.
    ///
    /// Defaults to `None`.
    #[debug("{:?}", on_connect_handler.as_ref().map(|_| "<handler>"))]
    pub(crate) on_connect_handler: Option<BoxAsyncUnaryResultHandler<WsIoServerConnection>>,

    /// Maximum duration for `on_connect_handler`.
    ///
    /// Defaults to the value inherited from the server builder.
    pub(crate) on_connect_handler_timeout: Duration,

    /// Optional namespace on-ready handler.
    ///
    /// Runs in a task spawned after the connection is marked ready, inserted into
    /// the namespace, and the ready packet is queued for sending. Setup does not
    /// await the task, which may start before setup returns.
    ///
    /// Defaults to `None`.
    #[debug("{:?}", on_ready_handler.as_ref().map(|_| "<handler>"))]
    pub(crate) on_ready_handler: Option<ArcAsyncUnaryResultHandler<WsIoServerConnection>>,

    /// Packet codec for this namespace's protocol packets and init data.
    ///
    /// Defaults to the value inherited from the server builder.
    pub(crate) packet_codec: WsIoPacketCodec,

    /// Transformer for this namespace's complete encoded WebSocket packets.
    ///
    /// Defaults to the value inherited from the server builder.
    #[debug("{:?}", "<transformer>")]
    pub(crate) packet_transformer: WsIoPacketTransformer,

    /// Namespace path used to route clients from the `namespace` query parameter
    /// after the server request path matches.
    pub(super) path: String,

    /// Tungstenite WebSocket transport limits and buffer sizes.
    ///
    /// The namespace receives a copy of the server-level configuration and may
    /// override it independently. Internal channel capacity is configured separately.
    ///
    /// Defaults to the value inherited from the server builder.
    pub(crate) websocket_config: WebSocketConfig,
}

