use std::{
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use crate::{
    core::{
        packet::codecs::WsIoPacketCodec,
        types::{
            ArcAsyncUnaryResultHandler,
            BoxAsyncUnaryResultHandler,
        },
    },
    session::WsIoClientSession,
};

// Types
type InitHandler = Box<
    dyn for<'a> Fn(
            Arc<WsIoClientSession>,
            Option<&'a [u8]>,
            &'a WsIoPacketCodec,
        ) -> Pin<Box<dyn Future<Output = Result<Option<Vec<u8>>>> + Send + 'a>>
        + Send
        + Sync
        + 'static,
>;

// Structs
pub(crate) struct WsIoClientConfig {
    pub(crate) init_handler: Option<InitHandler>,

    /// Maximum duration allowed for the init handler to execute.
    pub(crate) init_handler_timeout: Duration,

    /// Maximum duration to wait for the server to send the init packet.
    pub(crate) init_packet_timeout: Duration,

    pub(crate) on_session_close_handler: Option<BoxAsyncUnaryResultHandler<WsIoClientSession>>,

    /// Maximum duration allowed for the on_session_close handler to execute.
    pub(crate) on_session_close_handler_timeout: Duration,

    pub(crate) on_session_ready_handler: Option<ArcAsyncUnaryResultHandler<WsIoClientSession>>,

    pub(crate) packet_codec: WsIoPacketCodec,

    pub(crate) ping_interval: Duration,

    /// Maximum duration to wait for the client to send the ready packet.
    pub(crate) ready_packet_timeout: Duration,

    pub(crate) reconnect_delay: Duration,

    pub(crate) websocket_config: WebSocketConfig,
}
