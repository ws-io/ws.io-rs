use std::{
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use tokio_tungstenite::tungstenite::{
    http::Request,
    protocol::WebSocketConfig,
};

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

type RequestModifier =
    Box<dyn Fn(Request<()>) -> Pin<Box<dyn Future<Output = Result<Request<()>>> + Send>> + Send + Sync + 'static>;

// Structs
pub(crate) struct WsIoClientConfig {
    /// Maximum duration to wait for graceful WebSocket shutdown after
    /// `disconnect` is requested.
    ///
    /// If the read/write tasks do not finish before this timeout, they are
    /// aborted so `disconnect().await` can complete.
    pub(crate) disconnect_timeout: Duration,

    /// Optional client-side init handler used during the server handshake.
    ///
    /// When the server sends an init packet, this handler receives the optional
    /// decoded payload and may return optional response data to encode and send
    /// back to the server.
    pub(crate) init_handler: Option<InitHandler>,

    /// Maximum duration allowed for `init_handler` to execute.
    pub(crate) init_handler_timeout: Duration,

    /// Maximum duration to wait for the server to send the init packet.
    ///
    /// This timeout starts after the WebSocket connection is established. If the
    /// init packet is not received in time, the session setup fails.
    pub(crate) init_packet_timeout: Duration,

    /// Optional handler invoked when a session closes.
    ///
    /// The handler is awaited with `on_session_close_handler_timeout`.
    pub(crate) on_session_close_handler: Option<BoxAsyncUnaryResultHandler<WsIoClientSession>>,

    /// Maximum duration allowed for `on_session_close_handler` to execute.
    pub(crate) on_session_close_handler_timeout: Duration,

    /// Optional handler invoked after the session has become ready.
    ///
    /// This handler is spawned asynchronously after the ready state is reached; it
    /// is not part of the blocking handshake path.
    pub(crate) on_session_ready_handler: Option<ArcAsyncUnaryResultHandler<WsIoClientSession>>,

    /// Packet codec used to encode and decode ws.io protocol packets.
    ///
    /// It must match the server namespace codec and controls whether encoded
    /// packets use WebSocket text or binary messages.
    pub(crate) packet_codec: WsIoPacketCodec,

    /// Interval between client heartbeat frames sent after the WebSocket session
    /// is created.
    ///
    /// The heartbeat is a one-byte binary WebSocket frame; the server ignores
    /// single-byte binary frames before protocol packet decoding.
    pub(crate) ping_interval: Duration,

    /// Maximum duration to wait for the ready packet from the server.
    ///
    /// The client sends its init response first, then waits for the server to mark
    /// the session ready. If the ready packet is not received in time, setup fails.
    pub(crate) ready_packet_timeout: Duration,

    /// Delay before attempting to reconnect after a disconnected session.
    pub(crate) reconnect_delay: Duration,

    /// Optional async modifier for the WebSocket HTTP request.
    ///
    /// Use this to adjust headers or other request metadata before
    /// `connect_async_with_config` is called.
    pub(crate) request_modifier: Option<RequestModifier>,

    /// Tungstenite WebSocket transport limits and buffer sizes.
    ///
    /// This config is passed into the client connection and is also used to size
    /// internal session channels from the configured max-write/write-buffer ratio.
    pub(crate) websocket_config: WebSocketConfig,
}
