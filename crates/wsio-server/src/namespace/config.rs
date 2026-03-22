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
    /// Maximum number of concurrent broadcast operations.
    pub(crate) broadcast_concurrency_limit: usize,

    /// Maximum duration to wait for the http request to upgrade the connection.
    pub(super) http_request_upgrade_timeout: Duration,

    pub(crate) init_request_handler: Option<InitRequestHandler>,

    /// Maximum duration allowed for the init request handler to execute.
    pub(crate) init_request_handler_timeout: Duration,

    pub(crate) init_response_handler: Option<InitResponseHandler>,

    /// Maximum duration allowed for the init response handler to execute.
    pub(crate) init_response_handler_timeout: Duration,

    /// Maximum duration to wait for the client to send the init response packet.
    pub(crate) init_response_timeout: Duration,

    pub(crate) middleware: Option<BoxAsyncUnaryResultHandler<WsIoServerConnection>>,

    /// Maximum duration allowed for middleware execution.
    pub(crate) middleware_execution_timeout: Duration,

    /// Maximum duration allowed for the on_close handler to execute.
    pub(crate) on_close_handler_timeout: Duration,

    /// Maximum duration allowed for the on_connect handler to execute.
    pub(crate) on_connect_handler_timeout: Duration,

    pub(crate) on_connect_handler: Option<BoxAsyncUnaryResultHandler<WsIoServerConnection>>,

    pub(crate) on_ready_handler: Option<ArcAsyncUnaryResultHandler<WsIoServerConnection>>,

    pub(crate) packet_codec: WsIoPacketCodec,

    pub(super) path: String,

    pub(crate) websocket_config: WebSocketConfig,
}
