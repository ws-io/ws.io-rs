use std::time::Duration;

use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use crate::core::packet::codecs::WsIoPacketCodec;

// Structs
#[derive(Debug)]
pub(crate) struct WsIoServerConfig {
    /// Maximum number of concurrent broadcast operations.
    ///
    /// Can be overridden by namespace-level configuration.
    pub(crate) broadcast_concurrency_limit: usize,

    /// Maximum duration allowed for the http request upgrade  to complete.
    ///
    /// Can be overridden by namespace-level configuration.
    pub(crate) http_request_upgrade_timeout: Duration,

    /// Maximum duration allowed for the init request handler to execute.
    ///
    /// Can be overridden by namespace-level configuration.
    pub(crate) init_request_handler_timeout: Duration,

    /// Maximum duration allowed for the init response handler to execute.
    ///
    /// Can be overridden by namespace-level configuration.
    pub(crate) init_response_handler_timeout: Duration,

    /// Maximum duration to wait for the client to send the init response packet.
    ///
    /// Can be overridden by namespace-level configuration.
    pub(crate) init_response_timeout: Duration,

    /// Maximum duration allowed for middleware execution.
    ///
    /// Can be overridden by namespace-level configuration.
    pub(crate) middleware_execution_timeout: Duration,

    /// Maximum duration allowed for the on_close handler to execute.
    ///
    /// Can be overridden by namespace-level configuration.
    pub(crate) on_close_handler_timeout: Duration,

    /// Maximum duration allowed for the on_connect handler to execute.
    ///
    /// Can be overridden by namespace-level configuration.
    pub(crate) on_connect_handler_timeout: Duration,

    /// Can be overridden by namespace-level configuration.
    pub(crate) packet_codec: WsIoPacketCodec,

    pub(crate) request_path: String,

    /// Can be overridden by namespace-level configuration.
    pub(crate) websocket_config: WebSocketConfig,
}
