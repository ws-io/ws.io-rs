use std::sync::{
    Arc,
    atomic::{
        AtomicUsize,
        Ordering,
    },
};

use anyhow::{
    Result,
    ensure,
};
use bytes::{
    Bytes,
    BytesMut,
};
use wsio_core::packet::transformers::{
    WsIoCustomPacketTransformer,
    WsIoPacketTransformer,
};

use super::{
    TEST_NAMESPACE,
    cleanup_e2e,
    create_connected_client_with_transformer,
    setup_server_with_transformer,
    wait_for_counter,
};

const TRANSFORMER_PREFIX: u8 = 0xa5;

#[derive(Default)]
struct CountingPrefixTransformer {
    encode_calls: AtomicUsize,
    decode_calls: AtomicUsize,
}

impl WsIoCustomPacketTransformer for CountingPrefixTransformer {
    fn decode(&self, bytes: &[u8]) -> Result<Bytes> {
        self.decode_calls.fetch_add(1, Ordering::SeqCst);
        ensure!(bytes.first() == Some(&TRANSFORMER_PREFIX), "missing transformer prefix");
        Ok(Bytes::copy_from_slice(&bytes[1..]))
    }

    fn encode(&self, bytes: &[u8]) -> Result<Bytes> {
        self.encode_calls.fetch_add(1, Ordering::SeqCst);

        let mut output = BytesMut::with_capacity(bytes.len() + 1);
        output.extend_from_slice(&[TRANSFORMER_PREFIX]);
        output.extend_from_slice(bytes);
        Ok(output.freeze())
    }
}

#[tokio::test]
async fn test_e2e_custom_packet_transformer_round_trip() {
    let transformer = Arc::new(CountingPrefixTransformer::default());
    let packet_transformer = WsIoPacketTransformer::custom(transformer.clone());
    let (server_task, server, ws_url) = setup_server_with_transformer(packet_transformer.clone()).await;

    let server_received = Arc::new(AtomicUsize::new(0));
    let server_received_clone = server_received.clone();
    let server_namespace = server
        .new_namespace_builder(TEST_NAMESPACE)
        .on_connect(move |connection| {
            let server_received = server_received_clone.clone();
            async move {
                connection.on("client_event", move |_ctx, _data: Arc<()>| {
                    let server_received = server_received.clone();
                    async move {
                        server_received.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    }
                });

                Ok(())
            }
        })
        .register()
        .unwrap();

    let client = create_connected_client_with_transformer(&ws_url, packet_transformer).await;

    let client_received = Arc::new(AtomicUsize::new(0));
    let client_received_clone = client_received.clone();
    client.on("server_event", move |_ctx, _data: Arc<()>| {
        let client_received = client_received_clone.clone();
        async move {
            client_received.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    });

    let encode_calls_before_events = transformer.encode_calls.load(Ordering::SeqCst);
    let decode_calls_before_events = transformer.decode_calls.load(Ordering::SeqCst);

    client.emit::<()>("client_event", None).await.unwrap();
    server_namespace.emit::<()>("server_event", None).await.unwrap();

    wait_for_counter(&server_received, 1).await;
    wait_for_counter(&client_received, 1).await;

    assert_eq!(
        transformer.encode_calls.load(Ordering::SeqCst) - encode_calls_before_events,
        2
    );

    assert_eq!(
        transformer.decode_calls.load(Ordering::SeqCst) - decode_calls_before_events,
        2
    );

    cleanup_e2e(vec![client], server_task).await;
}
