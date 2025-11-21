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
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use url::Url;

use crate::{
    WsIoClient,
    config::WsIoClientConfig,
    core::packet::codecs::WsIoPacketCodec,
    runtime::WsIoClientRuntime,
    session::WsIoClientSession,
};

// Structs
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
    pub fn build(self) -> WsIoClient {
        WsIoClient(WsIoClientRuntime::new(self.config, self.connect_url))
    }

    pub fn init_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.init_handler_timeout = duration;
        self
    }

    pub fn init_packet_timeout(mut self, duration: Duration) -> Self {
        self.config.init_packet_timeout = duration;
        self
    }

    pub fn on_session_close<H, Fut>(mut self, handler: H) -> Self
    where
        H: Fn(Arc<WsIoClientSession>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        self.config.on_session_close_handler = Some(Box::new(move |session| Box::pin(handler(session))));
        self
    }

    pub fn on_session_close_handler_timeout(mut self, duration: Duration) -> Self {
        self.config.on_session_close_handler_timeout = duration;
        self
    }

    pub fn on_session_ready<H, Fut>(mut self, handler: H) -> Self
    where
        H: Fn(Arc<WsIoClientSession>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        self.config.on_session_ready_handler = Some(Arc::new(move |session| Box::pin(handler(session))));
        self
    }

    pub fn packet_codec(mut self, packet_codec: WsIoPacketCodec) -> Self {
        self.config.packet_codec = packet_codec;
        self
    }

    pub fn ping_interval(mut self, duration: Duration) -> Self {
        self.config.ping_interval = duration;
        self
    }

    pub fn ready_packet_timeout(mut self, duration: Duration) -> Self {
        self.config.ready_packet_timeout = duration;
        self
    }

    pub fn reconnect_delay(mut self, delay: Duration) -> Self {
        self.config.reconnect_delay = delay;
        self
    }

    pub fn request_path(mut self, request_path: impl AsRef<str>) -> Self {
        self.connect_url
            .set_path(&Self::normalize_url_path(request_path.as_ref()));

        self
    }

    pub fn websocket_config(mut self, websocket_config: WebSocketConfig) -> Self {
        self.config.websocket_config = websocket_config;
        self
    }

    pub fn websocket_config_mut<F: FnOnce(&mut WebSocketConfig)>(mut self, f: F) -> Self {
        f(&mut self.config.websocket_config);
        self
    }

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
