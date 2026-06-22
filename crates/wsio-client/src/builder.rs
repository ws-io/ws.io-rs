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
    core::packet::codecs::WsIoPacketCodec,
    runtime::WsIoClientRuntime,
    session::WsIoClientSession,
};

// Structs

/// Builder for configuring and creating a [`WsIoClient`].
///
/// The URL passed to the client constructor selects the namespace from its path,
/// while the actual WebSocket request path defaults to `/ws.io`.
#[derive(Debug)]
pub struct WsIoClientBuilder {
    config: WsIoClientConfig,
    connect_url: Url,
}

impl WsIoClientBuilder {
    pub(crate) fn new(mut url: Url) -> Result<Self> {
        if !matches!(url.scheme(), "ws" | "wss") {
            bail!("Invalid URL scheme: {}", url.scheme());
        }

        let mut query_pairs = url.query_pairs().collect::<Vec<_>>();
        query_pairs.retain(|(k, _)| k != "namespace");
        query_pairs.push(("namespace".into(), Self::normalize_url_path(url.path()).into()));
        let query = query_pairs
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");

        url.set_query(Some(&query));
        url.set_path("ws.io");
        Ok(Self {
            config: WsIoClientConfig {
                disconnect_timeout: Duration::from_secs(5),
                init_handler: None,
                init_handler_timeout: Duration::from_secs(3),
                init_packet_timeout: Duration::from_secs(5),
                on_session_close_handler: None,
                on_session_close_handler_timeout: Duration::from_secs(2),
                on_session_ready_handler: None,
                packet_codec: WsIoPacketCodec::SerdeJson,
                ping_interval: Duration::from_secs(25),
                ready_packet_timeout: Duration::from_secs(5),
                reconnect_delay: Duration::from_secs(1),
                request_modifier: None,
                websocket_config: WebSocketConfig::default()
                    .max_frame_size(Some(8 * 1024 * 1024))
                    .max_message_size(Some(16 * 1024 * 1024))
                    .max_write_buffer_size(2 * 1024 * 1024)
                    .read_buffer_size(8 * 1024)
                    .write_buffer_size(8 * 1024),
            },
            connect_url: url,
        })
    }

    // Private methods
    fn normalize_url_path(path: &str) -> String {
        format!(
            "/{}",
            path.split('/').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("/")
        )
    }

    // Public methods

    /// Builds a [`WsIoClient`] with the accumulated configuration.
    pub fn build(self) -> WsIoClient {
        WsIoClient(WsIoClientRuntime::new(self.config, self.connect_url))
    }

    /// Sets how long `disconnect().await` waits for graceful WebSocket
    /// shutdown before aborting the connection read/write tasks.
    pub fn disconnect_timeout(mut self, duration: Duration) -> Self {
        self.config.disconnect_timeout = duration;
        self
    }

    /// Sets the maximum duration allowed for the init handler to run.
    ///
    /// The init handler is registered with [`Self::with_init_handler`] and is
    /// invoked after the server sends the init packet.
    pub fn init_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.init_handler_timeout = duration;
        self
    }

    /// Sets how long the client waits for the server init packet after the
    /// WebSocket connection is established.
    ///
    /// If the init packet is not received before this timeout, the session is
    /// closed and the runtime may reconnect according to [`Self::reconnect_delay`].
    pub fn init_packet_timeout(mut self, duration: Duration) -> Self {
        self.config.init_packet_timeout = duration;
        self
    }

    /// Registers a handler that runs when a session closes.
    ///
    /// The handler is awaited during session cleanup and is bounded by
    /// [`Self::on_session_close_handler_timeout`].
    pub fn on_session_close<H, Fut>(mut self, handler: H) -> Self
    where
        H: Fn(Arc<WsIoClientSession>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        self.config.on_session_close_handler = Some(Box::new(move |session| Box::pin(handler(session))));
        self
    }

    /// Sets the maximum duration allowed for the session-close handler to run.
    pub fn on_session_close_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.on_session_close_handler_timeout = duration;
        self
    }

    /// Registers a handler that runs after a session becomes ready.
    ///
    /// The handler is spawned asynchronously after the ready packet is received,
    /// so it does not block the connection handshake.
    pub fn on_session_ready<H, Fut>(mut self, handler: H) -> Self
    where
        H: Fn(Arc<WsIoClientSession>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        self.config.on_session_ready_handler = Some(Arc::new(move |session| Box::pin(handler(session))));
        self
    }

    /// Sets the packet codec used to encode and decode ws.io protocol packets.
    ///
    /// This must match the server namespace codec.
    pub fn packet_codec(mut self, packet_codec: WsIoPacketCodec) -> Self {
        self.config.packet_codec = packet_codec;
        self
    }

    /// Sets the interval for client heartbeat frames.
    ///
    /// After session initialization starts, the client periodically sends a
    /// one-byte binary WebSocket frame. The server treats single-byte binary
    /// frames as heartbeats and ignores them before packet decoding.
    pub fn ping_interval(mut self, duration: Duration) -> Self {
        self.config.ping_interval = duration;
        self
    }

    /// Sets how long the client waits for the server ready packet.
    ///
    /// The ready timeout starts after the client handles the server init packet
    /// and sends its init response.
    pub fn ready_packet_timeout(mut self, duration: Duration) -> Self {
        self.config.ready_packet_timeout = duration;
        self
    }

    /// Sets the delay before the runtime attempts another connection.
    ///
    /// This delay is used after a connection attempt/session ends while the client
    /// runtime is still running.
    pub fn reconnect_delay(mut self, delay: Duration) -> Self {
        self.config.reconnect_delay = delay;
        self
    }

    /// Registers an async modifier for the WebSocket HTTP request.
    ///
    /// Use this to add headers or adjust request metadata before
    /// `connect_async_with_config` is called.
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
    /// Paths are normalized to a single leading slash with empty path segments
    /// removed. This controls the request URI path, not the namespace query value
    /// inferred from the original URL passed to the builder.
    pub fn request_path(mut self, request_path: impl AsRef<str>) -> Self {
        self.connect_url
            .set_path(&Self::normalize_url_path(request_path.as_ref()));

        self
    }

    /// Replaces the full Tungstenite WebSocket configuration.
    ///
    /// This controls transport limits and buffer sizes passed to the WebSocket
    /// connection. It is also used to derive internal channel capacity from the
    /// configured max-write/write-buffer ratio.
    pub fn websocket_config(mut self, websocket_config: WebSocketConfig) -> Self {
        self.config.websocket_config = websocket_config;
        self
    }

    /// Mutates the current Tungstenite WebSocket configuration in place.
    ///
    /// Prefer this when you want to adjust one or two fields while keeping the
    /// builder defaults for the rest.
    pub fn websocket_config_mut<F: FnOnce(&mut WebSocketConfig)>(mut self, f: F) -> Self {
        f(&mut self.config.websocket_config);
        self
    }

    /// Registers the client-side init handler.
    ///
    /// The handler receives the session and the optional server init payload
    /// decoded as `D`. Its optional return value is encoded as `R` and sent back
    /// to the server as the client init response.
    pub fn with_init_handler<H, Fut, D, R>(mut self, handler: H) -> WsIoClientBuilder
    where
        H: Fn(Arc<WsIoClientSession>, Option<D>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Option<R>>> + Send + 'static,
        D: DeserializeOwned + Send + 'static,
        R: Serialize + Send + 'static,
    {
        let handler = Arc::new(handler);
        self.config.init_handler = Some(Box::new(move |session, bytes, packet_codec| {
            let handler = handler.clone();
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
mod tests {
    use tokio_tungstenite::tungstenite::http::HeaderValue;

    use super::*;

    const TEST_URL: &str = "ws://localhost:8080/socket";

    fn test_builder() -> WsIoClientBuilder {
        WsIoClientBuilder::new(Url::parse(TEST_URL).unwrap()).unwrap()
    }

    #[test]
    fn test_builder_new_valid_ws_url_sets_default_request_path_and_namespace_query() {
        let builder = test_builder();

        assert_eq!(builder.connect_url.path(), "/ws.io");
        assert_eq!(
            builder
                .connect_url
                .query_pairs()
                .find(|(key, _)| key == "namespace")
                .map(|(_, value)| value.into_owned()),
            Some("/socket".into())
        );
    }

    #[test]
    fn test_builder_new_valid_wss_url() {
        let result = WsIoClientBuilder::new(Url::parse("wss://localhost:8080/socket").unwrap());
        assert!(result.is_ok());
    }

    #[test]
    fn test_builder_new_invalid_scheme() {
        let result = WsIoClientBuilder::new(Url::parse("http://localhost:8080/socket").unwrap());
        assert!(result.is_err());
        if let Err(e) = result {
            let err_msg = format!("{e}");
            assert!(err_msg.contains("Invalid URL scheme"));
        }
    }

    #[test]
    fn test_builder_configuration_chaining_updates_runtime_config() {
        let builder = test_builder()
            .disconnect_timeout(Duration::from_secs(20))
            .init_handler_timeout(Duration::from_secs(10))
            .init_packet_timeout(Duration::from_secs(15))
            .on_session_close_handler_timeout(Duration::from_secs(5))
            .packet_codec(WsIoPacketCodec::SerdeJson)
            .ping_interval(Duration::from_secs(30))
            .ready_packet_timeout(Duration::from_secs(10))
            .reconnect_delay(Duration::from_secs(5))
            .request_path("/custom/path");

        assert_eq!(builder.connect_url.path(), "/custom/path");

        let client = builder.build();

        let config = &client.0.config;
        assert_eq!(config.disconnect_timeout, Duration::from_secs(20));
        assert_eq!(config.init_handler_timeout, Duration::from_secs(10));
        assert_eq!(config.init_packet_timeout, Duration::from_secs(15));
        assert_eq!(config.on_session_close_handler_timeout, Duration::from_secs(5));
        assert!(matches!(config.packet_codec, WsIoPacketCodec::SerdeJson));
        assert_eq!(config.ping_interval, Duration::from_secs(30));
        assert_eq!(config.ready_packet_timeout, Duration::from_secs(10));
        assert_eq!(config.reconnect_delay, Duration::from_secs(5));
    }

    #[test]
    fn test_builder_request_path_normalizes() {
        let builder = test_builder().request_path("/multiple//slashes///path/");

        assert_eq!(builder.connect_url.path(), "/multiple/slashes/path");
    }

    #[test]
    fn test_builder_websocket_config_override() {
        let client = test_builder()
            .websocket_config_mut(|config| {
                *config = config.max_frame_size(Some(1024 * 1024));
            })
            .build();

        assert_eq!(client.0.config.websocket_config.max_frame_size, Some(1024 * 1024));
    }

    #[test]
    fn test_builder_websocket_config_replaces_defaults() {
        let config = WebSocketConfig::default().max_frame_size(Some(42));
        let client = test_builder().websocket_config(config).build();

        assert_eq!(client.0.config.websocket_config.max_frame_size, Some(42));
    }

    #[test]
    fn test_builder_with_init_and_session_handlers_registers_callbacks() {
        let client = test_builder()
            .with_init_handler(|_session, _data: Option<String>| async { Ok(Some("response".to_string())) })
            .on_session_ready(|_session| async { Ok(()) })
            .on_session_close(|_session| async { Ok(()) })
            .build();

        assert!(client.0.config.init_handler.is_some());
        assert!(client.0.config.on_session_ready_handler.is_some());
        assert!(client.0.config.on_session_close_handler.is_some());
    }

    #[test]
    fn test_builder_request_modifier_registers_async_callback() {
        let client = test_builder()
            .request_modifier(|mut request| async move {
                request
                    .headers_mut()
                    .insert("x-wsio-test", HeaderValue::from_static("enabled"));

                Ok(request)
            })
            .build();

        assert!(client.0.config.request_modifier.is_some());
    }

    #[test]
    fn test_builder_all_timeout_configurations() {
        let client = test_builder()
            .disconnect_timeout(Duration::from_millis(500))
            .init_handler_timeout(Duration::from_secs(1))
            .init_packet_timeout(Duration::from_secs(2))
            .on_session_close_handler_timeout(Duration::from_secs(3))
            .ready_packet_timeout(Duration::from_secs(4))
            .build();

        assert_eq!(client.0.config.disconnect_timeout, Duration::from_millis(500));
        assert_eq!(client.0.config.init_handler_timeout, Duration::from_secs(1));
        assert_eq!(client.0.config.init_packet_timeout, Duration::from_secs(2));
        assert_eq!(client.0.config.on_session_close_handler_timeout, Duration::from_secs(3));
        assert_eq!(client.0.config.ready_packet_timeout, Duration::from_secs(4));
    }

    #[test]
    fn test_builder_reconnect_delay_configuration() {
        let client = test_builder().reconnect_delay(Duration::from_millis(500)).build();

        assert_eq!(client.0.config.reconnect_delay, Duration::from_millis(500));
    }
}
