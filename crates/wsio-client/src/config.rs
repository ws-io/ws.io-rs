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
use tokio_tungstenite::tungstenite::{
    http::Request,
    protocol::WebSocketConfig,
};

use crate::{
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
    session::WsIoClientSession,
};

// Types
type InitHandler = Box<
    dyn for<'a> Fn(
            Arc<WsIoClientSession>,
            Option<&'a [u8]>,
            &'a WsIoPacketCodec,
        ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>>> + Send + 'a>>
        + Send
        + Sync
        + 'static,
>;

type RequestModifier =
    Box<dyn Fn(Request<()>) -> Pin<Box<dyn Future<Output = Result<Request<()>>> + Send>> + Send + Sync + 'static>;

// Structs
pub(crate) struct WsIoClientConfig {
    /// Maximum duration for a WebSocket connection attempt.
    ///
    /// This covers the transport connection and HTTP upgrade performed by
    /// [`tokio_tungstenite::connect_async_with_config`]. `None` allows the attempt to wait
    /// indefinitely.
    ///
    /// Defaults to `Some(Duration::from_secs(10))`.
    pub(crate) connect_timeout: Option<Duration>,

    /// Maximum duration for graceful WebSocket shutdown after [`disconnect`](crate::WsIoClient::disconnect).
    ///
    /// If the read/write tasks do not finish in time, they are aborted.
    ///
    /// Defaults to 5 seconds.
    /// `Duration::ZERO` does not disable the timeout.
    pub(crate) disconnect_timeout: Duration,

    /// Optional client-side init handler for the server handshake.
    ///
    /// When the server sends an init packet, the handler receives its optional
    /// decoded payload and may return optional response data.
    ///
    /// Defaults to `None`.
    pub(crate) init_handler: Option<InitHandler>,

    /// Maximum duration for `init_handler`.
    ///
    /// Defaults to 3 seconds.
    /// `Duration::ZERO` does not disable the timeout.
    pub(crate) init_handler_timeout: Duration,

    /// Maximum duration for waiting for the server init packet.
    ///
    /// This starts after the WebSocket connection is established. If the packet
    /// is not received in time, session setup fails.
    ///
    /// Defaults to 5 seconds.
    /// `Duration::ZERO` does not disable the timeout.
    pub(crate) init_packet_timeout: Duration,

    /// Optional handler invoked when a client session closes.
    ///
    /// The handler runs during session cleanup and is bounded by
    /// `on_session_close_handler_timeout`.
    ///
    /// Defaults to `None`.
    pub(crate) on_session_close_handler: Option<BoxAsyncUnaryResultHandler<WsIoClientSession>>,

    /// Maximum duration for `on_session_close_handler`.
    ///
    /// Defaults to 2 seconds.
    /// `Duration::ZERO` does not disable the timeout.
    pub(crate) on_session_close_handler_timeout: Duration,

    /// Optional handler invoked after a client session becomes ready.
    ///
    /// The handler is spawned after the ready state is reached and does not block
    /// the handshake.
    ///
    /// Defaults to `None`.
    pub(crate) on_session_ready_handler: Option<ArcAsyncUnaryResultHandler<WsIoClientSession>>,

    /// Packet codec for ws.io protocol packets.
    ///
    /// The codec must match the server namespace codec. All supported codecs use
    /// binary WebSocket messages.
    ///
    /// Defaults to [`WsIoPacketCodec::Msgpack`].
    pub(crate) packet_codec: WsIoPacketCodec,

    /// Transformer applied to complete encoded WebSocket packets.
    ///
    /// Defaults to the no-op transformer.
    pub(crate) packet_transformer: WsIoPacketTransformer,

    /// Interval between client heartbeat frames after the WebSocket session is
    /// created.
    ///
    /// The heartbeat is a one-byte binary WebSocket frame. The server ignores
    /// these frames before protocol packet decoding.
    ///
    /// Defaults to 25 seconds.
    /// `Duration::ZERO` introduces no intentional delay.
    pub(crate) ping_interval: Duration,

    /// Maximum duration for waiting for the server ready packet.
    ///
    /// The timeout starts after init handling, before the client init response is
    /// queued for sending. If the ready packet is not received in time, setup fails.
    ///
    /// Defaults to 5 seconds.
    /// `Duration::ZERO` does not disable the timeout.
    pub(crate) ready_packet_timeout: Duration,

    /// Delay before reconnecting after a connection attempt or session ends.
    ///
    /// Defaults to 1 second.
    /// `Duration::ZERO` introduces no intentional delay.
    pub(crate) reconnect_delay: Duration,

    /// Optional async modifier for the WebSocket HTTP request.
    ///
    /// The modifier can adjust headers or other request metadata before
    /// [`tokio_tungstenite::connect_async_with_config`] is called.
    ///
    /// Defaults to `None`.
    pub(crate) request_modifier: Option<RequestModifier>,

    /// Tungstenite WebSocket transport limits and buffer sizes.
    ///
    /// The configuration is passed to the client connection and derives internal
    /// session channel capacity from the configured max-write/write-buffer ratio.
    ///
    /// The defaults are 8 MiB per frame, 16 MiB per message, a 2 MiB maximum write
    /// buffer, and 8 KiB read and write buffers; other settings use
    /// [`WebSocketConfig::default`].
    pub(crate) websocket_config: WebSocketConfig,
}

impl FmtDebug for WsIoClientConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("WsIoClientConfig")
            .field("connect_timeout", &self.connect_timeout)
            .field("disconnect_timeout", &self.disconnect_timeout)
            .field("init_handler", &self.init_handler.as_ref().map(|_| "<handler>"))
            .field("init_handler_timeout", &self.init_handler_timeout)
            .field("init_packet_timeout", &self.init_packet_timeout)
            .field(
                "on_session_close_handler",
                &self.on_session_close_handler.as_ref().map(|_| "<handler>"),
            )
            .field(
                "on_session_close_handler_timeout",
                &self.on_session_close_handler_timeout,
            )
            .field(
                "on_session_ready_handler",
                &self.on_session_ready_handler.as_ref().map(|_| "<handler>"),
            )
            .field("packet_codec", &self.packet_codec)
            .field("packet_transformer", &"<transformer>")
            .field("ping_interval", &self.ping_interval)
            .field("ready_packet_timeout", &self.ready_packet_timeout)
            .field("reconnect_delay", &self.reconnect_delay)
            .field(
                "request_modifier",
                &self.request_modifier.as_ref().map(|_| "<modifier>"),
            )
            .field("websocket_config", &self.websocket_config)
            .finish()
    }
}
