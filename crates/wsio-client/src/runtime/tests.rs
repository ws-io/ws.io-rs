use std::{
    future::pending,
    time::Duration,
};

use anyhow::bail;
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::poll;

use super::*;
use crate::{
    WsIoClient,
    builder::WsIoClientBuilder,
    core::packet::{
        WsIoPacketType,
        codecs::WsIoPacketCodec,
        transformers::{
            WsIoPacketTransformer,
            custom::WsIoPacketCustomTransformer,
        },
    },
    test_support::TEST_TIMEOUT,
};

/// Reports a receiver that closes when the pending Event encoding future is dropped.
struct StalledEventTransformer {
    started_tx: tokio::sync::mpsc::UnboundedSender<tokio::sync::mpsc::UnboundedReceiver<()>>,
}

#[async_trait]
impl WsIoPacketCustomTransformer for StalledEventTransformer {
    async fn decode(&self, bytes: &[u8]) -> Result<Bytes> {
        Ok(Bytes::copy_from_slice(bytes))
    }

    async fn encode(&self, bytes: &[u8]) -> Result<Bytes> {
        if matches!(WsIoPacketCodec::Msgpack.decode(bytes)?.r#type, WsIoPacketType::Event) {
            let (alive_tx, alive_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
            self.started_tx.send(alive_rx).unwrap();
            let _alive = alive_tx;
            pending::<()>().await;
        }

        Ok(Bytes::copy_from_slice(bytes))
    }
}

fn new_client(configure: impl FnOnce(WsIoClientBuilder) -> WsIoClientBuilder) -> WsIoClient {
    let builder = WsIoClient::builder("ws://localhost/test")
        .unwrap()
        .channel_capacity(1)
        .disconnect_timeout(Duration::ZERO)
        .packet_codec(WsIoPacketCodec::Msgpack)
        .reconnect_delay(Duration::ZERO)
        .request_modifier(|_| pending());

    configure(builder).build().unwrap()
}

#[tokio::test]
async fn disconnect_cancels_pending_event_encoding_across_reconnect() {
    let (started_tx, mut started_rx) = tokio::sync::mpsc::unbounded_channel();
    let client = new_client(|builder| {
        builder.packet_transformer(WsIoPacketTransformer::custom(Arc::new(StalledEventTransformer {
            started_tx,
        })))
    });

    timeout(TEST_TIMEOUT, client.connect()).await.unwrap();
    let mut pending_emit = Box::pin(client.emit::<()>("pending", None));
    assert!(poll!(&mut pending_emit).is_pending());
    let mut event_alive_rx = timeout(TEST_TIMEOUT, started_rx.recv()).await.unwrap().unwrap();
    assert_eq!(
        event_alive_rx.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    );

    timeout(TEST_TIMEOUT, client.disconnect()).await.unwrap();
    timeout(TEST_TIMEOUT, client.connect()).await.unwrap();
    assert!(
        timeout(TEST_TIMEOUT, pending_emit)
            .await
            .expect("old emit must not wait for its transformer after disconnect")
            .is_err()
    );

    assert_eq!(
        event_alive_rx.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Disconnected)
    );

    timeout(TEST_TIMEOUT, client.disconnect()).await.unwrap();
}

#[tokio::test]
async fn disconnect_discards_queued_events_and_rejects_old_pending_sends() {
    let client = new_client(|builder| builder);
    timeout(TEST_TIMEOUT, client.connect()).await.unwrap();

    // The forwarder holds the first event while no session exists; the second fills the queue.
    for event in ["held", "queued"] {
        timeout(TEST_TIMEOUT, client.emit::<()>(event, None))
            .await
            .unwrap()
            .unwrap();
    }

    let mut pending_emit = Box::pin(client.emit::<()>("pending", None));
    assert!(poll!(&mut pending_emit).is_pending());
    timeout(TEST_TIMEOUT, client.disconnect()).await.unwrap();
    timeout(TEST_TIMEOUT, client.connect()).await.unwrap();

    // Resume the old producer only after the next lifecycle has started.
    assert!(timeout(TEST_TIMEOUT, pending_emit).await.unwrap().is_err());

    // The new forwarder and queue must have room for two fresh events, not old buffered work.
    for event in ["new-held", "new-queued"] {
        timeout(TEST_TIMEOUT, client.emit::<()>(event, None))
            .await
            .unwrap()
            .unwrap();
    }

    timeout(TEST_TIMEOUT, client.disconnect()).await.unwrap();
}

#[tokio::test]
async fn automatic_reconnect_preserves_queue_backpressure() {
    let (attempt_tx, mut attempt_rx) = tokio::sync::mpsc::unbounded_channel();
    let client = new_client(|builder| {
        builder.request_modifier(move |_| {
            let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
            attempt_tx.send(release_tx).unwrap();
            async move {
                release_rx.await?;
                bail!("connection attempt rejected")
            }
        })
    });

    timeout(TEST_TIMEOUT, client.connect()).await.unwrap();
    let first_attempt = timeout(TEST_TIMEOUT, attempt_rx.recv()).await.unwrap().unwrap();
    for event in ["held", "queued"] {
        timeout(TEST_TIMEOUT, client.emit::<()>(event, None))
            .await
            .unwrap()
            .unwrap();
    }

    let mut pending_emit = Box::pin(client.emit::<()>("pending", None));
    assert!(poll!(&mut pending_emit).is_pending());
    first_attempt.send(()).unwrap();
    let _second_attempt = timeout(TEST_TIMEOUT, attempt_rx.recv()).await.unwrap().unwrap();

    // A real retry occurred, but the buffered events still occupy this lifecycle's queue.
    assert!(poll!(&mut pending_emit).is_pending());
    drop(pending_emit);
    timeout(TEST_TIMEOUT, client.disconnect()).await.unwrap();
}
