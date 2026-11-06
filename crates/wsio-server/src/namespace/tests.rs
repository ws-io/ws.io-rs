use std::{
    convert::Infallible,
    future::pending,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
    time::Duration,
};

use anyhow::bail;
use async_trait::async_trait;
use bytes::Bytes;
use http::{
    Request,
    StatusCode,
};
use http_body::{
    Body,
    Frame,
};
use hyper::{
    body::Incoming,
    server::conn::http1,
    service::service_fn,
};
use tokio::{
    io::duplex,
    sync::mpsc::{
        self,
        error::TryRecvError,
    },
};
use tokio_tungstenite::client_async;

use super::*;
use crate::{
    core::packet::{
        WsIoPacketType,
        codecs::WsIoPacketCodec,
        transformers::{
            WsIoPacketTransformer,
            custom::WsIoPacketCustomTransformer,
        },
    },
    request::dispatch_request,
    test_support::{
        TEST_TIMEOUT,
        ready_connection,
    },
};

#[derive(Default)]
struct EmptyBody;

impl Body for EmptyBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        Poll::Ready(None)
    }
}

/// Reports a receiver that closes when the pending Init encoding future is dropped.
struct StalledInitTransformer {
    started_tx: mpsc::UnboundedSender<mpsc::UnboundedReceiver<()>>,
}

#[async_trait]
impl WsIoPacketCustomTransformer for StalledInitTransformer {
    async fn decode(&self, bytes: &[u8]) -> Result<Bytes> {
        Ok(Bytes::copy_from_slice(bytes))
    }

    async fn encode(&self, bytes: &[u8]) -> Result<Bytes> {
        if matches!(WsIoPacketCodec::Msgpack.decode(bytes)?.r#type, WsIoPacketType::Init) {
            let (alive_tx, alive_rx) = mpsc::unbounded_channel::<()>();
            self.started_tx.send(alive_rx).unwrap();
            let _alive = alive_tx;
            pending::<()>().await;
        }

        Ok(Bytes::copy_from_slice(bytes))
    }
}

#[test]
fn registration_rejects_namespace_outside_whitelist() {
    let server = WsIoServer::builder().build().unwrap();
    for path in ["/世界", "/a b", "/a/../b"] {
        assert!(server.new_namespace_builder(path).register().is_err(), "{path:?}");
    }

    assert_eq!(server.namespace_count(), 0);
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
async fn room_membership_survives_duplicate_join_leave_and_cleanup() {
    let server = WsIoServer::builder().build().unwrap();
    let first_namespace = server.new_namespace_builder("/first").register().unwrap();
    let second_namespace = server.new_namespace_builder("/second").register().unwrap();
    let (first, _first_messages) = ready_connection(&first_namespace).await;
    let (second, _second_messages) = ready_connection(&first_namespace).await;
    let (third, _third_messages) = ready_connection(&second_namespace).await;
    let only_second = [second.id()].into_iter().collect::<RoaringTreemap>();

    first.join(["shared", "shared", "solo"]);
    second.join(["shared"]);
    third.join(["shared"]);
    assert_eq!(first_namespace.room_connection_count("shared"), 2);
    assert_eq!(second_namespace.room_connection_count("shared"), 1);
    assert_eq!(server.room_connection_count("shared"), 3);
    assert_eq!(server.room_connection_count("solo"), 1);
    assert_eq!(server.room_connection_count("missing"), 0);

    first.leave(["shared", "shared", "missing"]);
    assert_eq!(*first_namespace.rooms.get("shared").unwrap(), only_second);
    assert_eq!(server.room_connection_count("shared"), 2);
    first.leave(["solo"]);
    assert!(!first_namespace.rooms.contains_key("solo"));

    // Joins after close must not survive cleanup.
    first.join(["shared", "solo"]);
    first.close();
    first.join(["late"]);
    first.cleanup().await;
    assert!(!first_namespace.rooms.contains_key("late"));
    assert!(!first_namespace.rooms.contains_key("solo"));
    assert_eq!(*first_namespace.rooms.get("shared").unwrap(), only_second);
    assert_eq!(first_namespace.connection_count(), 1);

    second.cleanup().await;
    assert!(first_namespace.rooms.is_empty());
    assert_eq!(server.room_connection_count("shared"), 1);
    third.cleanup().await;
    assert_eq!(server.room_connection_count("shared"), 0);
    assert_eq!(server.connection_count(), 0);
}

#[tokio::test]
async fn init_response_failure_drops_pending_init_and_allows_shutdown() {
    let (started_tx, mut started_rx) = mpsc::unbounded_channel();
    let (init_response_tx, mut init_response_rx) = mpsc::unbounded_channel();
    let server = WsIoServer::builder()
        .disconnect_timeout(Duration::ZERO)
        .build()
        .unwrap();

    let namespace = server
        .new_namespace_builder("/test")
        .init_response_timeout(Duration::from_secs(60))
        .packet_codec(WsIoPacketCodec::Msgpack)
        .packet_transformer(WsIoPacketTransformer::custom(Arc::new(StalledInitTransformer {
            started_tx,
        })))
        .with_init_response(move |_, _: Option<()>| {
            init_response_tx.send(()).unwrap();
            async { bail!("init response rejected") }
        })
        .register()
        .unwrap();

    let (server_io, client_io) = duplex(4096);
    let runtime = Arc::clone(&server.0);
    let http_task = spawn(async move {
        http1::Builder::new()
            .serve_connection(
                TokioIo::new(server_io),
                service_fn(move |mut request: Request<Incoming>| {
                    let runtime = Arc::clone(&runtime);
                    async move { dispatch_request::<_, EmptyBody, Infallible>(&mut request, &runtime) }
                }),
            )
            .with_upgrades()
            .await
            .unwrap();
    });

    let (mut client, response) = timeout(
        TEST_TIMEOUT,
        client_async("ws://localhost/ws.io?namespace=/test", client_io),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
    let mut init_alive_rx = timeout(TEST_TIMEOUT, started_rx.recv()).await.unwrap().unwrap();
    assert_eq!(init_alive_rx.try_recv(), Err(TryRecvError::Empty));
    assert_eq!(namespace.connection_count(), 0);

    // An early Init disarms the watchdog and fails the reader while server Init is still pending.
    let init = WsIoPacketCodec::Msgpack.encode(&WsIoPacket::new_init(None)).unwrap();
    timeout(TEST_TIMEOUT, client.send(Message::Binary(init)))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(timeout(TEST_TIMEOUT, init_response_rx.recv()).await.unwrap(), Some(()));

    // Prove the reader failure drops initialization, without shutdown cancellation masking the bug.
    assert_eq!(
        timeout(TEST_TIMEOUT, init_alive_rx.recv())
            .await
            .expect("reader failure must drop pending Init before shutdown"),
        None
    );

    timeout(TEST_TIMEOUT, namespace.shutdown()).await.unwrap();
    assert_eq!(namespace.connection_count(), 0);
    assert_eq!(server.connection_count(), 0);

    drop(client);
    timeout(TEST_TIMEOUT, http_task).await.unwrap().unwrap();
}
