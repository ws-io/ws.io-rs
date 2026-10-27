use std::{
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use bytes::Bytes;
use derive_more::Debug;
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
#[derive(Debug)]
pub(crate) struct WsIoClientConfig {
    /// Number of messages permitted in each internal channel.
    ///
    /// This limits message count, not bytes.
    ///
    /// Defaults to `2048`.
    pub(crate) channel_capacity: usize,

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
    #[debug("{:?}", init_handler.as_ref().map(|_| "<handler>"))]
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

    /// Maximum outgoing Binary payload size after packet transformation, excluding frame headers.
    ///
    /// Oversized packets are rejected before queueing.
    ///
    /// Defaults to 2 MiB.
    pub(crate) max_outgoing_message_size: usize,

    /// Optional handler invoked when a client session closes.
    ///
    /// The handler runs during session cleanup and is bounded by
    /// `on_session_close_handler_timeout`.
    ///
    /// Defaults to `None`.
    #[debug("{:?}", on_session_close_handler.as_ref().map(|_| "<handler>"))]
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
    #[debug("{:?}", on_session_ready_handler.as_ref().map(|_| "<handler>"))]
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
    #[debug("{:?}", "<transformer>")]
    pub(crate) packet_transformer: WsIoPacketTransformer,

    /// Interval between client heartbeat frames after the WebSocket session is
    /// created.
    ///
    /// The heartbeat is a one-byte binary WebSocket frame. The server skips
    /// these frames before protocol packet decoding, but receiving them keeps the
    /// server's connection idle timeout from expiring. Keep this interval well
    /// below that timeout (90 seconds by default).
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
    #[debug("{:?}", request_modifier.as_ref().map(|_| "<modifier>"))]
    pub(crate) request_modifier: Option<RequestModifier>,

    /// Tungstenite WebSocket transport limits and buffer sizes.
    ///
    /// The configuration is passed to the client connection. Internal session
    /// channel capacity is configured independently.
    ///
    /// The receiving defaults are 8 MiB per frame and 16 MiB per message. The maximum
    /// write buffer is 2 MiB + 8 KiB + [`OUTGOING_FRAME_OVERHEAD`](crate::core::transport::OUTGOING_FRAME_OVERHEAD) bytes, with 8 KiB read and write buffers; other settings use
    /// [`WebSocketConfig::default`].
    pub(crate) websocket_config: WebSocketConfig,
}
