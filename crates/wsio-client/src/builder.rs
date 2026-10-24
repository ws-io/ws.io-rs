use std::{
    sync::Arc,
    time::Duration,
};

use anyhow::{
    Result,
    bail,
};
use serde::{
    Serialize,
    de::DeserializeOwned,
};
use tokio_tungstenite::tungstenite::{
    http::Request,
    protocol::WebSocketConfig,
};
use url::Url;

use crate::{
    WsIoClient,
    config::WsIoClientConfig,
    core::{
        packet::{
            codecs::WsIoPacketCodec,
            transformers::WsIoPacketTransformer,
        },
        transport::{
            DEFAULT_CHANNEL_CAPACITY,
            DEFAULT_MAX_OUTGOING_MESSAGE_SIZE,
            default_websocket_config,
            validate_transport_config,
        },
        utils::path::normalize_path,
    },
    runtime::WsIoClientRuntime,
    session::WsIoClientSession,
};

// Structs

/// A builder for configuring and creating a [`WsIoClient`].
///
/// The input URL path selects the namespace, normalized to one leading slash
/// with repeated and trailing slashes removed. The root namespace is `/`.
/// The WebSocket request path defaults to `/ws.io`.
///
/// Configuration methods consume the builder and return the updated builder.
/// Registering a callback replaces the previous callback of that kind. Callbacks
/// return asynchronous work and may run concurrently for different sessions.
/// No ordering is guaranteed across sessions. A timeout drops unfinished callback
/// work without rolling back completed side effects.
#[derive(Debug)]
#[must_use]
pub struct WsIoClientBuilder {
    config: WsIoClientConfig,
    connect_url: Url,
}

impl WsIoClientBuilder {
    pub(crate) fn new(mut url: Url) -> Result<Self> {
        if !matches!(url.scheme(), "ws" | "wss") {
            bail!("Invalid URL scheme: {}", url.scheme());
        }

        let query_pairs = url
            .query_pairs()
            .filter(|(key, _)| key != "namespace")
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect::<Vec<_>>();

        let namespace = normalize_path(url.path());
        url.query_pairs_mut()
            .clear()
            .extend_pairs(query_pairs)
            .append_pair("namespace", &namespace);

        url.set_path("ws.io");
        Ok(Self {
            config: WsIoClientConfig {
                channel_capacity: DEFAULT_CHANNEL_CAPACITY,
                connect_timeout: Some(Duration::from_secs(10)),
                disconnect_timeout: Duration::from_secs(5),
                init_handler: None,
                init_handler_timeout: Duration::from_secs(3),
                init_packet_timeout: Duration::from_secs(5),
                max_outgoing_message_size: DEFAULT_MAX_OUTGOING_MESSAGE_SIZE,
                on_session_close_handler: None,
                on_session_close_handler_timeout: Duration::from_secs(2),
                on_session_ready_handler: None,
                packet_codec: WsIoPacketCodec::Msgpack,
                packet_transformer: WsIoPacketTransformer::default(),
                ping_interval: Duration::from_secs(25),
                ready_packet_timeout: Duration::from_secs(5),
                reconnect_delay: Duration::from_secs(1),
                request_modifier: None,
                websocket_config: default_websocket_config(),
            },
            connect_url: url,
        })
    }

    // Public methods

    /// Builds a [`WsIoClient`] from this builder's configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if channel capacity is invalid, the outgoing limit is zero,
    /// or the write buffer cannot accommodate the outgoing limit and required overhead.
    pub fn build(self) -> Result<WsIoClient> {
        validate_transport_config(
            &self.config.websocket_config,
            self.config.channel_capacity,
            self.config.max_outgoing_message_size,
        )?;

        Ok(WsIoClient(WsIoClientRuntime::new(self.config, self.connect_url)))
    }

    /// Sets the message capacity of each internal channel.
    ///
    /// Defaults to `2048`. Must be between 1 and [`tokio::sync::Semaphore::MAX_PERMITS`].
    /// This limits message count, not bytes, and is independent of WebSocket buffer sizes.
    pub fn channel_capacity(mut self, channel_capacity: usize) -> Self {
        self.config.channel_capacity = channel_capacity;
        self
    }

    /// Sets the maximum duration for a WebSocket connection attempt.
    ///
    /// This covers the transport connection and HTTP upgrade, but not ws.io
    /// protocol initialization. Use [`Self::init_packet_timeout`] and
    /// [`Self::ready_packet_timeout`] for initialization. Pass `None` to disable
    /// the timeout.
    pub fn connect_timeout(mut self, duration: impl Into<Option<Duration>>) -> Self {
        self.config.connect_timeout = duration.into();
        self
    }

    /// Sets the maximum duration for graceful WebSocket shutdown.
    ///
    /// If the read/write tasks do not finish within this duration, they are
    /// aborted.
    pub fn disconnect_timeout(mut self, duration: Duration) -> Self {
        self.config.disconnect_timeout = duration;
        self
    }

    /// Sets the maximum duration for the client init handler.
    ///
    /// The handler is registered with [`Self::with_init_handler`] and runs after
    /// the server sends the init packet.
    pub fn init_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.init_handler_timeout = duration;
        self
    }

    /// Sets the maximum duration for waiting for the server init packet.
    ///
    /// This starts after the WebSocket connection is established. If the packet
    /// is not received in time, the session closes and the runtime may reconnect
    /// according to [`Self::reconnect_delay`].
    pub fn init_packet_timeout(mut self, duration: Duration) -> Self {
        self.config.init_packet_timeout = duration;
        self
    }

    /// Sets the maximum outgoing Binary payload size after packet transformation, excluding frame headers.
    ///
    /// Defaults to 2 MiB; zero is rejected. The limit applies after codec encoding
    /// and transformation, before queueing. The write buffer must fit this limit
    /// plus its configured write threshold and [`OUTGOING_FRAME_OVERHEAD`](crate::core::transport::OUTGOING_FRAME_OVERHEAD) bytes.
    /// Messages are not fragmented; the peer must accept a frame of this size.
    /// Large messages can leave a large retained write buffer on the connection.
    pub fn max_outgoing_message_size(mut self, max_outgoing_message_size: usize) -> Self {
        self.config.max_outgoing_message_size = max_outgoing_message_size;
        self
    }

    /// Registers a handler that runs when a client session closes.
    ///
    /// The handler receives the closing session once during cleanup. Its returned
    /// future is awaited; returned errors are ignored. A timeout drops the future
    /// and is logged when the `tracing` feature is enabled.
    ///
    /// The handler runs during session cleanup and is bounded by
    /// [`Self::on_session_close_handler_timeout`].
    pub fn on_session_close<H, Fut>(mut self, handler: H) -> Self
    where
        H: Fn(Arc<WsIoClientSession>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        self.config.on_session_close_handler = Some(Box::new(move |session| Box::pin(handler(session))));
        self
    }

    /// Sets the maximum duration for the session-close handler.
    pub fn on_session_close_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.on_session_close_handler_timeout = duration;
        self
    }

    /// Registers a handler that runs after a client session becomes ready.
    ///
    /// The handler receives the ready session once in a background task. Session
    /// cancellation drops unfinished work. Returned errors are logged when the
    /// `tracing` feature is enabled and do not fail the handshake.
    ///
    /// The handler is spawned after the ready packet is received and does not
    /// block the connection handshake.
    pub fn on_session_ready<H, Fut>(mut self, handler: H) -> Self
    where
        H: Fn(Arc<WsIoClientSession>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        self.config.on_session_ready_handler = Some(Arc::new(move |session| Box::pin(handler(session))));
        self
    }

    /// Sets the packet codec for ws.io protocol packets.
    ///
    /// The codec must match the server namespace codec.
    pub fn packet_codec(mut self, packet_codec: WsIoPacketCodec) -> Self {
        self.config.packet_codec = packet_codec;
        self
    }

    /// Sets the packet transformer for all client protocol packets.
    pub fn packet_transformer(mut self, packet_transformer: WsIoPacketTransformer) -> Self {
        self.config.packet_transformer = packet_transformer;
        self
    }

    /// Sets the interval between client heartbeat frames.
    ///
    /// After session initialization starts, the client periodically sends a
    /// one-byte binary WebSocket frame. The server treats these frames as
    /// heartbeats: they are skipped before packet decoding but keep the server's
    /// connection idle timeout (90 seconds by default) from expiring, so keep
    /// this interval well below it.
    pub fn ping_interval(mut self, duration: Duration) -> Self {
        self.config.ping_interval = duration;
        self
    }

    /// Sets the maximum duration for waiting for the server ready packet.
    ///
    /// This starts after the client handles the server init packet, before its
    /// init response is queued for sending.
    pub fn ready_packet_timeout(mut self, duration: Duration) -> Self {
        self.config.ready_packet_timeout = duration;
        self
    }

    /// Sets the delay before reconnecting.
    ///
    /// The delay applies after a connection attempt or session ends while the
    /// client runtime is still running.
    pub fn reconnect_delay(mut self, delay: Duration) -> Self {
        self.config.reconnect_delay = delay;
        self
    }

    /// Registers an async modifier for the WebSocket HTTP request.
    ///
    /// The modifier receives the owned request once per connection attempt and
    /// returns the request to send. Its future is awaited before the transport
    /// connection timeout starts. An error ends the attempt; the runtime may retry
    /// after the configured reconnect delay. Cancellation does not undo completed
    /// side effects.
    ///
    /// The modifier can add headers or adjust request metadata before
    /// [`tokio_tungstenite::connect_async_with_config`] is called. An in-flight modifier future is
    /// canceled when the client disconnects and may run again after reconnect.
    pub fn request_modifier<M, Fut>(mut self, modifier: M) -> Self
    where
        M: Fn(Request<()>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Request<()>>> + Send + 'static,
    {
        self.config.request_modifier = Some(Box::new(move |request| Box::pin(modifier(request))));
        self
    }

    /// Sets the WebSocket HTTP request path.
    ///
    /// The path is normalized to one leading slash with empty segments removed.
    /// This controls the request URI path, not the namespace query value derived
    /// from the input URL.
    pub fn request_path(mut self, request_path: impl AsRef<str>) -> Self {
        self.connect_url.set_path(&normalize_path(request_path.as_ref()));

        self
    }

    /// Replaces the full Tungstenite WebSocket configuration.
    ///
    /// The configuration controls transport limits and buffer sizes independently
    /// of [`Self::channel_capacity`].
    pub fn websocket_config(mut self, websocket_config: WebSocketConfig) -> Self {
        self.config.websocket_config = websocket_config;
        self
    }

    /// Mutates the current Tungstenite WebSocket configuration in place.
    ///
    /// The closure `f` runs once synchronously with a mutable borrow of the configuration.
    /// This preserves fields that the closure does not change.
    pub fn websocket_config_mut<F: FnOnce(&mut WebSocketConfig)>(mut self, f: F) -> Self {
        f(&mut self.config.websocket_config);
        self
    }

    /// Registers the client-side init handler.
    ///
    /// The handler runs once per server init packet accepted during session setup.
    /// Its future is awaited within [`Self::init_handler_timeout`]. Missing payload
    /// data is passed as `None`; returning `None` sends an init response without
    /// payload data. Payload decoding, handler, or response encoding errors fail
    /// session setup.
    ///
    /// The handler receives the session and the optional server init payload
    /// decoded as `D`. An optional `R` result is encoded and sent as the client
    /// init response.
    pub fn with_init_handler<H, Fut, D, R>(mut self, handler: H) -> Self
    where
        H: Fn(Arc<WsIoClientSession>, Option<D>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Option<R>>> + Send + 'static,
        D: DeserializeOwned + Send + 'static,
        R: Serialize + Send + 'static,
    {
        let handler = Arc::new(handler);
        self.config.init_handler = Some(Box::new(move |session, bytes, packet_codec| {
            let handler = Arc::clone(&handler);
            Box::pin(async move {
                handler(session, bytes.map(|bytes| packet_codec.decode_data(bytes)).transpose()?)
                    .await?
                    .map(|data| packet_codec.encode_data(&data))
                    .transpose()
            })
        }));

        self
    }
}

#[cfg(test)]
mod tests;
