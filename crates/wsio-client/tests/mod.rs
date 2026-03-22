use std::{
    sync::{
        Arc,
        atomic::{
            AtomicUsize,
            Ordering,
        },
    },
    time::Duration,
};

use wsio_client::{
    WsIoClient,
    core::packet::codecs::WsIoPacketCodec,
};

#[tokio::test]
async fn test_client_builder_new_valid_ws_url() {
    let result = WsIoClient::builder("ws://localhost:8080/socket");
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_client_builder_new_valid_wss_url() {
    let result = WsIoClient::builder("wss://localhost:8080/socket");
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_client_builder_new_invalid_scheme() {
    let result = WsIoClient::builder("http://localhost:8080/socket");
    assert!(result.is_err());
    assert!(result.err().unwrap().to_string().contains("Invalid URL scheme"));
}

#[tokio::test]
async fn test_client_builder_configuration_chaining() {
    WsIoClient::builder("ws://localhost:8080/socket")
        .unwrap()
        .init_handler_timeout(Duration::from_secs(10))
        .init_packet_timeout(Duration::from_secs(15))
        .on_session_close_handler_timeout(Duration::from_secs(5))
        .packet_codec(WsIoPacketCodec::Msgpack)
        .ping_interval(Duration::from_secs(30))
        .ready_packet_timeout(Duration::from_secs(10))
        .reconnect_delay(Duration::from_secs(5))
        .request_path("/custom/path")
        .build();
}

#[tokio::test]
async fn test_client_builder_request_path_normalizes() {
    // Test that request_path properly normalizes paths
    WsIoClient::builder("ws://localhost:8080/socket")
        .unwrap()
        .request_path("/multiple//slashes///path/")
        .build();
}

#[tokio::test]
async fn test_client_builder_websocket_config_override() {
    WsIoClient::builder("ws://localhost:8080/socket")
        .unwrap()
        .websocket_config_mut(|config| {
            *config = config.max_frame_size(Some(1024 * 1024));
        })
        .build();
}

#[tokio::test]
async fn test_client_builder_with_init_handler() {
    let call_count = Arc::new(AtomicUsize::new(0));
    let call_count_clone = call_count.clone();

    WsIoClient::builder("ws://localhost:8080/socket")
        .unwrap()
        .with_init_handler(move |_session, _data: Option<String>| {
            let count = call_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(Some("response".to_string()))
            }
        })
        .build();
}

#[tokio::test]
async fn test_client_builder_all_timeout_configurations() {
    let _client = WsIoClient::builder("ws://localhost:8080/socket")
        .unwrap()
        .init_handler_timeout(Duration::from_secs(1))
        .init_packet_timeout(Duration::from_secs(2))
        .on_session_close_handler_timeout(Duration::from_secs(3))
        .ready_packet_timeout(Duration::from_secs(4))
        .build();
}

#[tokio::test]
async fn test_client_builder_reconnect_delay_configuration() {
    let _client = WsIoClient::builder("ws://localhost:8080/socket")
        .unwrap()
        .reconnect_delay(Duration::from_millis(500))
        .build();
}
