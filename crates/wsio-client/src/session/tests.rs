use std::{
    future::pending,
    sync::atomic::{
        AtomicUsize,
        Ordering,
    },
    time::Duration,
};

use futures_util::poll;
use tokio::{
    sync::{
        mpsc::error::TryRecvError,
        oneshot,
    },
    task::yield_now,
    time::advance,
};

use super::*;
use crate::builder::WsIoClientBuilder;

fn builder() -> WsIoClientBuilder {
    WsIoClient::builder("ws://localhost/test").unwrap().channel_capacity(1)
}

fn fixture(
    builder: WsIoClientBuilder,
) -> (
    Arc<WsIoClientSession>,
    mpsc::Receiver<Arc<Message>>,
    mpsc::Receiver<WsIoPacket>,
) {
    WsIoClientSession::new(builder.build().unwrap().0)
}

async fn incoming(session: &Arc<WsIoClientSession>, packet: WsIoPacket) -> Result<()> {
    session
        .handle_incoming_packet(session.runtime.config.packet_codec.encode(&packet)?)
        .await
}

fn decode_message(session: &WsIoClientSession, message: &Message) -> WsIoPacket {
    let Message::Binary(bytes) = message else {
        panic!("expected a binary packet")
    };

    session.runtime.config.packet_codec.decode(bytes).unwrap()
}

async fn ready(session: &Arc<WsIoClientSession>, messages: &mut mpsc::Receiver<Arc<Message>>) {
    session.init().await;
    incoming(session, WsIoPacket::new_init(None)).await.unwrap();
    let packet = decode_message(session, &messages.try_recv().unwrap());
    assert!(matches!(packet.r#type, WsIoPacketType::Init));
    incoming(session, WsIoPacket::new_ready()).await.unwrap();
}

#[tokio::test]
async fn handshake_rejects_out_of_order_packets_and_gates_events_until_ready() {
    let (session, mut messages, mut events) = fixture(builder());
    assert!(incoming(&session, WsIoPacket::new_init(None)).await.is_err());
    assert!(incoming(&session, WsIoPacket::new_ready()).await.is_err());
    session.init().await;
    assert!(incoming(&session, WsIoPacket::new_ready()).await.is_err());
    incoming(&session, WsIoPacket::new_event("early", None)).await.unwrap();
    assert!(events.try_recv().is_err());
    incoming(&session, WsIoPacket::new_init(None)).await.unwrap();
    assert_eq!(session.state.get(), SessionState::AwaitingReady);
    messages.try_recv().unwrap();
    assert!(incoming(&session, WsIoPacket::new_init(None)).await.is_err());
    incoming(&session, WsIoPacket::new_ready()).await.unwrap();
    assert!(session.is_ready());
    assert!(incoming(&session, WsIoPacket::new_ready()).await.is_err());
    incoming(&session, WsIoPacket::new_event("accepted", None))
        .await
        .unwrap();

    assert_eq!(events.try_recv().unwrap().key.as_deref(), Some("accepted"));
    session.close();
    assert!(
        session
            .emit_event_message(Arc::new(Message::Binary(Bytes::new())))
            .await
            .is_err()
    );

    assert!(incoming(&session, WsIoPacket::new_init(None)).await.is_err());
    assert!(incoming(&session, WsIoPacket::new_ready()).await.is_err());
    session.cleanup().await;
    assert_eq!(session.state.get(), SessionState::Closed);
}

#[tokio::test(start_paused = true)]
async fn missing_init_or_ready_expires_and_success_disarms_watchdogs() {
    for awaiting_ready in [false, true] {
        let (session, mut messages, _) = fixture(
            builder()
                .init_packet_timeout(Duration::from_secs(2))
                .ready_packet_timeout(Duration::from_secs(2)),
        );

        session.init().await;
        if awaiting_ready {
            incoming(&session, WsIoPacket::new_init(None)).await.unwrap();
            messages.try_recv().unwrap();
        }

        timeout(Duration::from_secs(3), session.cancel_token.cancelled())
            .await
            .unwrap();

        assert_eq!(session.state.get(), SessionState::Closing);
        session.cleanup().await;
    }

    let (session, mut messages, _) = fixture(
        builder()
            .init_packet_timeout(Duration::from_secs(2))
            .ready_packet_timeout(Duration::from_secs(2)),
    );

    ready(&session, &mut messages).await;
    advance(Duration::from_secs(3)).await;
    yield_now().await;
    assert!(session.is_ready());
    assert!(!session.cancel_token.is_cancelled());
    assert!(session.init_timeout_task.lock().await.is_none());
    assert!(session.ready_timeout_task.lock().await.is_none());
    session.cleanup().await;
}

#[tokio::test(start_paused = true)]
async fn init_handler_failure_and_timeout_never_publish_ready_or_a_response() {
    for stalls in [false, true] {
        let (session, mut messages, _) = fixture(
            builder()
                .init_handler_timeout(Duration::from_secs(1))
                .with_init_handler(move |_, _: Option<()>| async move {
                    if stalls {
                        pending::<()>().await;
                    }

                    Err::<Option<()>, _>(anyhow!("init failed"))
                }),
        );

        session.init().await;
        assert!(
            timeout(Duration::from_secs(2), incoming(&session, WsIoPacket::new_init(None)))
                .await
                .unwrap()
                .is_err()
        );

        assert!(!session.is_ready());
        assert!(messages.try_recv().is_err());
        assert!(session.ready_timeout_task.lock().await.is_none());
        session.cleanup().await;
    }
}

#[tokio::test]
async fn full_queues_do_not_block_close_or_event_waiters() {
    let (session, mut messages, mut events) = fixture(builder());
    ready(&session, &mut messages).await;
    session
        .send_message(Arc::new(Message::Binary(Bytes::new())))
        .await
        .unwrap();

    incoming(&session, WsIoPacket::new_event("first", None)).await.unwrap();
    let mut blocked = Box::pin(incoming(&session, WsIoPacket::new_event("blocked", None)));
    assert!(poll!(&mut blocked).is_pending());
    session.close();
    session.close();
    timeout(Duration::from_secs(2), blocked).await.unwrap().unwrap();
    assert!(session.cancel_token.is_cancelled());
    assert_eq!(events.try_recv().unwrap().key.as_deref(), Some("first"));
    assert!(events.try_recv().is_err());
    timeout(Duration::from_secs(2), session.cleanup()).await.unwrap();
    assert!(session.ping_task.lock().await.is_none());
}

#[tokio::test]
async fn cleanup_cancels_and_joins_dispatch_without_prior_close() {
    let closes = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&closes);
    let (session, mut messages, events) = fixture(builder().on_session_close(move |_| {
        let observed = Arc::clone(&observed);
        async move {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }));

    ready(&session, &mut messages).await;
    let (started_tx, mut started_rx) = mpsc::unbounded_channel();
    let (alive_tx, mut alive_rx) = mpsc::unbounded_channel::<()>();
    let id = session.runtime.event_registry.on("e", move |_, _: Arc<()>| {
        let started = started_tx.clone();
        let alive = alive_tx.clone();
        async move {
            started.send(()).unwrap();
            pending::<()>().await;
            drop(alive);
            Ok(())
        }
    });

    session.start_event_dispatcher(events).await;
    incoming(&session, WsIoPacket::new_event("e", None)).await.unwrap();
    timeout(Duration::from_secs(2), started_rx.recv())
        .await
        .unwrap()
        .unwrap();

    session.runtime.event_registry.off_by_handler_id("e", id);
    // Transport failure can enter cleanup without close() running first.
    assert!(!session.cancel_token.is_cancelled());
    timeout(Duration::from_secs(2), session.cleanup()).await.unwrap();
    assert!(session.cancel_token.is_cancelled());
    assert_eq!(alive_rx.try_recv(), Err(TryRecvError::Disconnected));

    assert_eq!(closes.load(Ordering::SeqCst), 1);
    assert!(session.event_dispatcher_task.lock().await.is_none());
    assert!(session.init_timeout_task.lock().await.is_none());
    assert!(session.ready_timeout_task.lock().await.is_none());
    assert!(session.ping_task.lock().await.is_none());
    assert_eq!(session.state.get(), SessionState::Closed);
}

#[tokio::test(start_paused = true)]
async fn stalled_close_hook_is_bounded_by_timeout() {
    let (session, _, _) = fixture(
        builder()
            .on_session_close_handler_timeout(Duration::from_secs(1))
            .on_session_close(|_| async {
                pending::<()>().await;
                Ok(())
            }),
    );

    timeout(Duration::from_secs(2), session.cleanup()).await.unwrap();
    assert_eq!(session.state.get(), SessionState::Closed);
}

#[tokio::test]
async fn init_payload_and_response_are_typed_and_ready_hook_runs_once() {
    let (ready_tx, mut ready_rx) = mpsc::unbounded_channel();
    let (session, mut messages, _) = fixture(
        builder()
            .with_init_handler(|_, data: Option<u32>| async move {
                assert_eq!(data, Some(7));
                Ok(Some(8_u32))
            })
            .on_session_ready(move |session| {
                let ready_tx = ready_tx.clone();
                async move {
                    ready_tx.send(session.is_ready()).unwrap();
                    Ok(())
                }
            }),
    );

    session.init().await;
    incoming(
        &session,
        WsIoPacket::new_init(Some(session.runtime.config.packet_codec.encode_data(&7_u32).unwrap())),
    )
    .await
    .unwrap();

    let packet = decode_message(&session, &messages.try_recv().unwrap());
    assert_eq!(
        session
            .runtime
            .config
            .packet_codec
            .decode_data::<u32>(&packet.data.unwrap())
            .unwrap(),
        8
    );

    incoming(&session, WsIoPacket::new_ready()).await.unwrap();
    assert!(timeout(Duration::from_secs(2), ready_rx.recv()).await.unwrap().unwrap());
    assert!(incoming(&session, WsIoPacket::new_ready()).await.is_err());
    session.cleanup().await;
    assert!(ready_rx.try_recv().is_err());
}

#[tokio::test]
async fn close_cancels_managed_work_before_cleanup() {
    let (session, _, _) = fixture(builder());
    let (started_tx, started_rx) = oneshot::channel();
    let (alive_tx, alive_rx) = oneshot::channel::<()>();
    session.spawn_task(async move {
        started_tx.send(()).unwrap();
        pending::<()>().await;
        drop(alive_tx);
        Ok(())
    });

    timeout(Duration::from_secs(2), started_rx).await.unwrap().unwrap();
    session.close();
    assert!(timeout(Duration::from_secs(2), alive_rx).await.unwrap().is_err());
    session.cleanup().await;
}

#[tokio::test(start_paused = true)]
async fn cleanup_aborts_pending_handshake_watchdogs_and_ping_task() {
    for awaiting_ready in [false, true] {
        let (session, mut messages, _) = fixture(builder());
        session.init().await;
        if awaiting_ready {
            incoming(&session, WsIoPacket::new_init(None)).await.unwrap();
            messages.try_recv().unwrap();
        }

        let watchdog = if awaiting_ready {
            &session.ready_timeout_task
        } else {
            &session.init_timeout_task
        };

        let watchdog_abort = watchdog.lock().await.as_ref().unwrap().abort_handle();
        let ping_abort = session.ping_task.lock().await.as_ref().unwrap().abort_handle();
        assert!(!watchdog_abort.is_finished());
        assert!(!ping_abort.is_finished());
        session.cleanup().await;
        assert!(watchdog.lock().await.is_none());
        assert!(session.ping_task.lock().await.is_none());
        // abort_locked_task requests cancellation; it does not promise to join.
        timeout(Duration::from_secs(1), async {
            while !watchdog_abort.is_finished() || !ping_abort.is_finished() {
                yield_now().await;
            }
        })
        .await
        .unwrap();

        assert_eq!(session.state.get(), SessionState::Closed);
    }
}
