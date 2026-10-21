use anyhow::{
    bail,
    ensure,
};
use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::mpsc::error::TryRecvError;

use super::{
    test_support::ready_connection,
    *,
};
#[cfg(feature = "packet-transformer-zstd")]
use crate::core::packet::transformers::zstd::WsIoPacketZstdTransformerConfig;
use crate::core::packet::{
    codecs::WsIoPacketCodec,
    transformers::{
        WsIoPacketTransformer,
        custom::WsIoPacketCustomTransformer,
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

fn namespace(limit: usize, transformer: WsIoPacketTransformer) -> Arc<WsIoServerNamespace> {
    WsIoServer::builder()
        .build()
        .unwrap()
        .new_namespace_builder("/test")
        .max_outgoing_message_size(limit)
        .packet_transformer(transformer)
        .register()
        .unwrap()
}

fn tagged(max_input: usize) -> WsIoPacketTransformer {
    WsIoPacketTransformer::custom(Arc::new(TaggedTransformer { max_input }))
}

#[tokio::test]
async fn outgoing_limit_is_checked_after_transforming_before_broadcast_enqueue() {
    let payload = vec![7_u8; 8];
    let packet = WsIoPacket::new_event("e", Some(WsIoPacketCodec::Msgpack.encode_data(&payload).unwrap()));
    let mut wire = vec![0xa5];
    wire.extend_from_slice(&WsIoPacketCodec::Msgpack.encode(&packet).unwrap());
    for limit in [wire.len() - 1, wire.len(), wire.len() + 1] {
        let namespace = namespace(limit, tagged(usize::MAX));
        let (connection, mut messages) = ready_connection(&namespace).await;
        let result = namespace.emit("e", Some(&payload)).await;
        if limit < wire.len() {
            assert!(result.unwrap_err().to_string().contains("max_outgoing_message_size"));
        } else {
            result.unwrap();
            assert_eq!(
                messages.try_recv().unwrap().as_ref(),
                &Message::Binary(Bytes::copy_from_slice(&wire))
            );
        }

        assert_eq!(messages.try_recv(), Err(TryRecvError::Empty));
        connection.cleanup().await;
    }
}

#[cfg(feature = "packet-transformer-zstd")]
#[tokio::test]
async fn compression_can_make_an_oversized_codec_packet_fit_the_outgoing_limit() {
    let payload = vec![0_u8; 256];
    let packet = WsIoPacket::new_event("e", Some(WsIoPacketCodec::Msgpack.encode_data(&payload).unwrap()));
    let encoded = WsIoPacketCodec::Msgpack.encode(&packet).unwrap();
    assert!(encoded.len() > 64);
    let namespace = namespace(
        64,
        WsIoPacketTransformer::zstd(WsIoPacketZstdTransformerConfig {
            compression_threshold: 0,
            ..WsIoPacketZstdTransformerConfig::default()
        }),
    );

    let (connection, mut messages) = ready_connection(&namespace).await;
    namespace.emit("e", Some(&payload)).await.unwrap();
    let message = messages.try_recv().unwrap();
    let Message::Binary(wire) = message.as_ref() else {
        panic!("expected binary message")
    };

    assert!(wire.len() <= 64);
    assert_eq!(
        namespace.config.packet_transformer.decode(wire.clone()).await.unwrap(),
        encoded
    );

    connection.cleanup().await;
}

#[tokio::test]
async fn transformer_failure_is_propagated_before_broadcast_enqueue() {
    // Handshake packets fit the input limit; the event payload does not.
    let namespace = namespace(64, tagged(16));
    let (connection, mut messages) = ready_connection(&namespace).await;
    let error = namespace.emit("e", Some(&vec![0_u8; 16])).await.unwrap_err();
    assert_eq!(error.to_string(), "custom input too large");
    assert_eq!(messages.try_recv(), Err(TryRecvError::Empty));
    connection.cleanup().await;
}
