use anyhow::{
    bail,
    ensure,
};
use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::mpsc::error::TryRecvError;

use super::*;
#[cfg(feature = "packet-transformer-zstd")]
use crate::core::packet::transformers::zstd::WsIoPacketZstdTransformerConfig;
use crate::{
    WsIoClient,
    core::packet::{
        codecs::WsIoPacketCodec,
        transformers::{
            WsIoPacketTransformer,
            custom::WsIoPacketCustomTransformer,
        },
    },
};

/// Prefixes one tag byte and rejects inputs larger than `max_input`.
struct TaggedTransformer {
    max_input: usize,
}

#[async_trait]
impl WsIoPacketCustomTransformer for TaggedTransformer {
    async fn decode(&self, bytes: &[u8]) -> Result<Bytes> {
        let Some((0xa5, payload)) = bytes.split_first() else {
            bail!("invalid custom tag")
        };

        Ok(Bytes::copy_from_slice(payload))
    }

    async fn encode(&self, bytes: &[u8]) -> Result<Bytes> {
        ensure!(bytes.len() <= self.max_input, "custom input too large");
        let mut output = vec![0xa5];
        output.extend_from_slice(bytes);
        Ok(output.into())
    }
}

fn runtime(limit: usize, transformer: WsIoPacketTransformer) -> Arc<WsIoClientRuntime> {
    let client = WsIoClient::builder("ws://localhost/test")
        .unwrap()
        .max_outgoing_message_size(limit)
        .packet_transformer(transformer)
        .build()
        .unwrap();

    // Exercise the outgoing queue without starting any network/background tasks.
    client.0.status.store(RuntimeStatus::Running);
    client.0
}

fn tagged(max_input: usize) -> WsIoPacketTransformer {
    WsIoPacketTransformer::custom(Arc::new(TaggedTransformer { max_input }))
}

#[tokio::test]
async fn outgoing_limit_is_checked_after_transforming_before_enqueue() {
    let payload = vec![7_u8; 8];
    let packet = WsIoPacket::new_event("e", Some(WsIoPacketCodec::Msgpack.encode_data(&payload).unwrap()));
    let mut wire = vec![0xa5];
    wire.extend_from_slice(&WsIoPacketCodec::Msgpack.encode(&packet).unwrap());
    for limit in [wire.len() - 1, wire.len(), wire.len() + 1] {
        let runtime = runtime(limit, tagged(usize::MAX));
        let result = runtime.emit("e", Some(&payload)).await;
        let mut queue = runtime.send_event_message_rx.lock().await;
        if limit < wire.len() {
            assert!(result.unwrap_err().to_string().contains("max_outgoing_message_size"));
        } else {
            result.unwrap();
            assert_eq!(
                queue.try_recv().unwrap().as_ref(),
                &Message::Binary(Bytes::copy_from_slice(&wire))
            );
        }

        assert_eq!(queue.try_recv(), Err(TryRecvError::Empty));
    }
}

#[cfg(feature = "packet-transformer-zstd")]
#[tokio::test]
async fn compression_can_make_an_oversized_codec_packet_fit_the_outgoing_limit() {
    let payload = vec![0_u8; 256];
    let packet = WsIoPacket::new_event("e", Some(WsIoPacketCodec::Msgpack.encode_data(&payload).unwrap()));
    let encoded = WsIoPacketCodec::Msgpack.encode(&packet).unwrap();
    assert!(encoded.len() > 64);
    let runtime = runtime(
        64,
        WsIoPacketTransformer::zstd(WsIoPacketZstdTransformerConfig {
            compression_threshold: 0,
            ..WsIoPacketZstdTransformerConfig::default()
        }),
    );

    runtime.emit("e", Some(&payload)).await.unwrap();
    let message = runtime.send_event_message_rx.lock().await.try_recv().unwrap();
    let Message::Binary(wire) = message.as_ref() else {
        panic!("expected binary message")
    };

    assert!(wire.len() <= 64);
    assert_eq!(
        runtime.config.packet_transformer.decode(wire.clone()).await.unwrap(),
        encoded
    );
}

#[tokio::test]
async fn transformer_failure_is_propagated_before_enqueue() {
    let runtime = runtime(64, tagged(16));
    let error = runtime.emit("e", Some(&vec![0_u8; 16])).await.unwrap_err();
    assert_eq!(error.to_string(), "custom input too large");
    assert_eq!(
        runtime.send_event_message_rx.lock().await.try_recv(),
        Err(TryRecvError::Empty)
    );
}
