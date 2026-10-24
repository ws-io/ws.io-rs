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

fn namespace(limit: usize, transformer: WsIoPacketTransformer) -> (WsIoServer, Arc<WsIoServerNamespace>) {
    let server = WsIoServer::builder().build().unwrap();
    let namespace = server
        .new_namespace_builder("/test")
        .max_outgoing_message_size(limit)
        .packet_transformer(transformer)
        .register()
        .unwrap();

    (server, namespace)
}

fn tagged(max_input: usize) -> WsIoPacketTransformer {
    WsIoPacketTransformer::custom(Arc::new(TaggedTransformer { max_input }))
}

#[tokio::test]
async fn namespace_path_spellings_share_one_identity() {
    let server = WsIoServer::builder().build().unwrap();
    for (variants, expected) in [
        (["", "/", "///"], "/"),
        (["test", "/test/", "//test//"], "/test"),
        (["a/b", "/a//b/", "//a///b//"], "/a/b"),
    ] {
        for registered in variants {
            let namespace = server.new_namespace_builder(registered).register().unwrap();
            assert_eq!(namespace.path(), expected);
            for lookup in variants {
                assert!(Arc::ptr_eq(&namespace, &server.of(lookup).unwrap()));
                assert!(server.new_namespace_builder(lookup).register().is_err());
            }

            assert_eq!(server.namespace_count(), 1);
            // Remove through a spelling different from the registered one.
            let removal = variants.into_iter().find(|&lookup| lookup != registered).unwrap();
            server.remove_namespace(removal).await;
            assert_eq!(server.namespace_count(), 0);
            for lookup in variants {
                assert!(server.of(lookup).is_none());
            }
        }
    }
}

#[tokio::test]
async fn room_connection_count_tracks_membership_across_namespaces() {
    let server = WsIoServer::builder().build().unwrap();
    assert_eq!(server.room_connection_count("shared"), 0);
    let first = server.new_namespace_builder("/first").register().unwrap();
    let second = server.new_namespace_builder("/second").register().unwrap();
    let (a, _arx) = ready_connection(&first).await;
    let (b, _brx) = ready_connection(&first).await;
    let (c, _crx) = ready_connection(&second).await;

    a.join(["shared", "shared", "other"]);
    b.join(["shared"]);
    c.join(["shared"]);
    assert_eq!(first.room_connection_count("shared"), 2);
    assert_eq!(second.room_connection_count("shared"), 1);
    assert_eq!(server.room_connection_count("shared"), 3);
    assert_eq!(server.room_connection_count("other"), 1);
    assert_eq!(first.room_connection_count("missing"), 0);
    assert_eq!(server.room_connection_count("missing"), 0);

    a.leave(["shared", "shared"]);
    assert_eq!(first.room_connection_count("shared"), 1);
    assert_eq!(server.room_connection_count("shared"), 2);
    b.cleanup().await;
    assert_eq!(first.room_connection_count("shared"), 0);
    assert_eq!(server.room_connection_count("shared"), 1);
    c.cleanup().await;
    assert_eq!(server.room_connection_count("shared"), 0);
    a.cleanup().await;
    assert_eq!(server.room_connection_count("other"), 0);
}

#[tokio::test]
async fn outgoing_limit_is_checked_after_transforming_before_broadcast_enqueue() {
    let payload = vec![7_u8; 8];
    let packet = WsIoPacket::new_event("e", Some(WsIoPacketCodec::Msgpack.encode_data(&payload).unwrap()));
    let mut wire = vec![0xa5];
    wire.extend_from_slice(&WsIoPacketCodec::Msgpack.encode(&packet).unwrap());
    for limit in [wire.len() - 1, wire.len(), wire.len() + 1] {
        let (_server, namespace) = namespace(limit, tagged(usize::MAX));
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
    let (_server, namespace) = namespace(
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
    let (_server, namespace) = namespace(64, tagged(16));
    let (connection, mut messages) = ready_connection(&namespace).await;
    let error = namespace.emit("e", Some(&vec![0_u8; 16])).await.unwrap_err();
    assert_eq!(error.to_string(), "custom input too large");
    assert_eq!(messages.try_recv(), Err(TryRecvError::Empty));
    connection.cleanup().await;
}
