use std::{
    future::pending,
    sync::{
        Mutex as StdMutex,
        atomic::AtomicUsize,
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
use crate::namespace::builder::WsIoServerNamespaceBuilder;

fn builder() -> WsIoServerNamespaceBuilder {
    WsIoServer::builder()
        .channel_capacity(1)
        .build()
        .unwrap()
        .new_namespace_builder("/test")
}

fn fixture(
    builder: WsIoServerNamespaceBuilder,
) -> (
    Arc<WsIoServerConnection>,
    mpsc::Receiver<Arc<Message>>,
    mpsc::Receiver<WsIoPacket>,
) {
    WsIoServerConnection::new(
        HeaderMap::new(),
        builder.register().unwrap(),
        Uri::from_static("/ws.io"),
    )
}

async fn incoming(connection: &Arc<WsIoServerConnection>, packet: WsIoPacket) -> Result<()> {
    connection
        .handle_incoming_packet(connection.namespace.config.packet_codec.encode(&packet)?)
        .await
}

fn decode_message(connection: &WsIoServerConnection, message: &Message) -> WsIoPacket {
    let Message::Binary(bytes) = message else {
        panic!("expected a binary packet")
    };

    connection.namespace.config.packet_codec.decode(bytes).unwrap()
}

async fn ready(connection: &Arc<WsIoServerConnection>, messages: &mut mpsc::Receiver<Arc<Message>>) {
    connection.init().await.unwrap();
    messages.try_recv().unwrap();
    incoming(connection, WsIoPacket::new_init(None)).await.unwrap();
    let packet = decode_message(connection, &messages.try_recv().unwrap());
    assert!(matches!(packet.r#type, WsIoPacketType::Ready));
}

async fn stall_or_fail(stalls: bool, message: &'static str) -> Result<()> {
    if stalls {
        pending::<()>().await;
    }

    bail!(message)
}

#[tokio::test]
async fn handshake_rejects_out_of_order_packets_and_gates_events_until_ready() {
    let (connection, mut messages, mut events) = fixture(builder());
    assert!(incoming(&connection, WsIoPacket::new_init(None)).await.is_err());
    // Ready is a server-to-client packet: receiving it must not activate a server connection.
    incoming(&connection, WsIoPacket::new_ready()).await.unwrap();
    assert_eq!(connection.state.get(), ConnectionState::Created);
    connection.init().await.unwrap();
    messages.try_recv().unwrap();
    assert!(connection.init().await.is_err());
    incoming(&connection, WsIoPacket::new_event("early", None))
        .await
        .unwrap();

    assert!(events.try_recv().is_err());
    incoming(&connection, WsIoPacket::new_init(None)).await.unwrap();
    messages.try_recv().unwrap();
    assert!(connection.is_ready());
    assert_eq!(connection.namespace.connection_count(), 1);
    assert!(incoming(&connection, WsIoPacket::new_init(None)).await.is_err());
    incoming(&connection, WsIoPacket::new_event("accepted", None))
        .await
        .unwrap();

    assert_eq!(events.try_recv().unwrap().key.as_deref(), Some("accepted"));
    connection.close();
    assert!(connection.emit("e", None::<&()>).await.is_err());
    assert!(incoming(&connection, WsIoPacket::new_init(None)).await.is_err());
    connection.cleanup().await;
    assert_eq!(connection.namespace.connection_count(), 0);
    assert_eq!(connection.server().connection_count(), 0);
}

#[tokio::test(start_paused = true)]
async fn missing_init_response_expires_but_ready_disarms_watchdog() {
    let (connection, mut messages, _) = fixture(builder().init_response_timeout(Duration::from_secs(1)));
    connection.init().await.unwrap();
    messages.try_recv().unwrap();
    timeout(Duration::from_secs(2), connection.cancel_token.cancelled())
        .await
        .unwrap();

    assert_eq!(connection.state.get(), ConnectionState::Closing);
    connection.cleanup().await;
    let (connection, mut messages, _) = fixture(builder().init_response_timeout(Duration::from_secs(1)));
    ready(&connection, &mut messages).await;
    advance(Duration::from_secs(2)).await;
    yield_now().await;
    assert!(connection.is_ready());
    assert!(!connection.cancel_token.is_cancelled());
    assert!(connection.init_timeout_task.lock().await.is_none());
    connection.cleanup().await;
}

#[tokio::test(start_paused = true)]
async fn setup_errors_and_timeouts_do_not_register_connections() {
    // Every awaited setup phase has its own failure/timeout boundary.
    for stage in 0..4 {
        for stalls in [false, true] {
            let namespace_builder = builder()
                .init_request_handler_timeout(Duration::from_secs(1))
                .init_response_handler_timeout(Duration::from_secs(1))
                .middleware_execution_timeout(Duration::from_secs(1))
                .on_connect_handler_timeout(Duration::from_secs(1));

            let namespace_builder = match stage {
                0 => namespace_builder.with_init_request(move |_| async move {
                    stall_or_fail(stalls, "init request failed").await.map(|()| None::<()>)
                }),
                1 => namespace_builder
                    .with_init_response(move |_, _: Option<()>| stall_or_fail(stalls, "init response failed")),
                2 => namespace_builder.with_middleware(move |_| stall_or_fail(stalls, "middleware failed")),
                _ => namespace_builder.on_connect(move |_| stall_or_fail(stalls, "on connect failed")),
            };

            let (connection, mut messages, _) = fixture(namespace_builder);
            if stage == 0 {
                assert!(
                    timeout(Duration::from_secs(2), connection.init())
                        .await
                        .unwrap()
                        .is_err()
                );
            } else {
                connection.init().await.unwrap();
                messages.try_recv().unwrap();
                assert!(
                    timeout(
                        Duration::from_secs(2),
                        incoming(&connection, WsIoPacket::new_init(None))
                    )
                    .await
                    .unwrap()
                    .is_err()
                );
            }

            assert!(!connection.is_ready());
            assert!(messages.try_recv().is_err());
            assert_eq!(connection.namespace.connection_count(), 0);
            assert_eq!(connection.server().connection_count(), 0);
            connection.cleanup().await;
        }
    }
}

#[tokio::test]
async fn closing_inside_middleware_cannot_resurrect_connection() {
    let (connection, mut messages, _) = fixture(builder().with_middleware(|connection| async move {
        connection.close();
        Ok(())
    }));

    connection.init().await.unwrap();
    messages.try_recv().unwrap();
    assert!(incoming(&connection, WsIoPacket::new_init(None)).await.is_err());
    assert_eq!(connection.state.get(), ConnectionState::Closing);
    assert_eq!(connection.namespace.connection_count(), 0);
    connection.cleanup().await;
}

#[tokio::test(start_paused = true)]
async fn full_queues_do_not_block_disconnect_or_event_waiters() {
    let (connection, mut messages, mut events) = fixture(builder().disconnect_timeout(Duration::from_secs(1)));
    ready(&connection, &mut messages).await;
    connection
        .send_message(Arc::new(Message::Binary(Bytes::new())))
        .await
        .unwrap();

    incoming(&connection, WsIoPacket::new_event("first", None))
        .await
        .unwrap();

    let mut blocked = Box::pin(incoming(&connection, WsIoPacket::new_event("blocked", None)));
    assert!(poll!(&mut blocked).is_pending());
    timeout(Duration::from_secs(2), connection.disconnect()).await.unwrap();
    timeout(Duration::from_secs(1), blocked).await.unwrap().unwrap();
    assert!(connection.cancel_token.is_cancelled());
    assert_eq!(events.try_recv().unwrap().key.as_deref(), Some("first"));
    assert!(events.try_recv().is_err());
    connection.cleanup().await;
}

#[tokio::test]
async fn cleanup_stops_dispatch_and_removes_membership_before_close_hook() {
    let (connection, mut messages, events) = fixture(builder());
    ready(&connection, &mut messages).await;
    connection.join(["room"]);
    let closes = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&closes);
    connection
        .on_close(move |connection| {
            let observed = Arc::clone(&observed);
            async move {
                assert_eq!(connection.namespace.connection_count(), 0);
                assert!(connection.joined_rooms.lock().is_empty());
                observed.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .await;

    let (started_tx, mut started_rx) = mpsc::unbounded_channel();
    let (alive_tx, mut alive_rx) = mpsc::unbounded_channel::<()>();
    let id = connection.on("e", move |_, _: Arc<()>| {
        let started = started_tx.clone();
        let alive = alive_tx.clone();
        async move {
            started.send(()).unwrap();
            pending::<()>().await;
            drop(alive);
            Ok(())
        }
    });

    connection.start_event_dispatcher(events).await;
    incoming(&connection, WsIoPacket::new_event("e", None)).await.unwrap();
    timeout(Duration::from_secs(2), started_rx.recv())
        .await
        .unwrap()
        .unwrap();

    connection.off_by_handler_id("e", id);
    // Transport failure can enter cleanup without close() running first.
    assert!(!connection.cancel_token.is_cancelled());
    timeout(Duration::from_secs(2), connection.cleanup()).await.unwrap();
    assert!(connection.cancel_token.is_cancelled());
    assert_eq!(alive_rx.try_recv(), Err(TryRecvError::Disconnected));

    connection.cleanup().await;
    assert_eq!(closes.load(Ordering::SeqCst), 1);
    assert_eq!(connection.server().connection_count(), 0);
    assert!(connection.event_dispatcher_task.lock().await.is_none());
    assert!(connection.init_timeout_task.lock().await.is_none());
    assert_eq!(connection.state.get(), ConnectionState::Closed);
}

#[tokio::test(start_paused = true)]
async fn stalled_close_hook_is_bounded_by_timeout() {
    let (connection, _, _) = fixture(builder().on_close_handler_timeout(Duration::from_secs(1)));
    connection
        .on_close(|_| async {
            pending::<()>().await;
            Ok(())
        })
        .await;

    timeout(Duration::from_secs(2), connection.cleanup()).await.unwrap();
    assert_eq!(connection.state.get(), ConnectionState::Closed);
}

#[tokio::test]
async fn setup_hooks_run_in_order_and_ready_hook_observes_published_connection() {
    let order = Arc::new(StdMutex::new(Vec::new()));
    let init_order = Arc::clone(&order);
    let middleware_order = Arc::clone(&order);
    let connect_order = Arc::clone(&order);
    let (ready_tx, mut ready_rx) = mpsc::unbounded_channel();
    let (connection, mut messages, _) = fixture(
        builder()
            .with_init_request(|_| async { Ok(Some(7_u32)) })
            .with_init_response(move |_, data: Option<u32>| {
                let order = Arc::clone(&init_order);
                async move {
                    assert_eq!(data, Some(8));
                    order.lock().unwrap().push("init");
                    Ok(())
                }
            })
            .with_middleware(move |_| {
                let order = Arc::clone(&middleware_order);
                async move {
                    order.lock().unwrap().push("middleware");
                    Ok(())
                }
            })
            .on_connect(move |_| {
                let order = Arc::clone(&connect_order);
                async move {
                    order.lock().unwrap().push("connect");
                    Ok(())
                }
            })
            .on_ready(move |connection| {
                let ready_tx = ready_tx.clone();
                async move {
                    ready_tx
                        .send((connection.is_ready(), connection.namespace.connection_count()))
                        .unwrap();

                    Ok(())
                }
            }),
    );

    connection.init().await.unwrap();
    let packet = decode_message(&connection, &messages.try_recv().unwrap());
    assert_eq!(
        connection
            .namespace
            .config
            .packet_codec
            .decode_data::<u32>(&packet.data.unwrap())
            .unwrap(),
        7
    );

    incoming(
        &connection,
        WsIoPacket::new_init(Some(
            connection.namespace.config.packet_codec.encode_data(&8_u32).unwrap(),
        )),
    )
    .await
    .unwrap();

    messages.try_recv().unwrap();
    assert_eq!(
        timeout(Duration::from_secs(2), ready_rx.recv()).await.unwrap().unwrap(),
        (true, 1)
    );

    assert_eq!(*order.lock().unwrap(), ["init", "middleware", "connect"]);
    assert!(incoming(&connection, WsIoPacket::new_init(None)).await.is_err());
    connection.cleanup().await;
    assert!(ready_rx.try_recv().is_err());
    assert_eq!(*order.lock().unwrap(), ["init", "middleware", "connect"]);
}

#[tokio::test]
async fn close_cancels_managed_work_before_cleanup() {
    let (connection, _, _) = fixture(builder());
    let (started_tx, started_rx) = oneshot::channel();
    let (alive_tx, alive_rx) = oneshot::channel::<()>();
    connection.spawn_task(async move {
        started_tx.send(()).unwrap();
        pending::<()>().await;
        drop(alive_tx);
        Ok(())
    });

    timeout(Duration::from_secs(2), started_rx).await.unwrap().unwrap();
    connection.close();
    assert!(timeout(Duration::from_secs(2), alive_rx).await.unwrap().is_err());
    connection.cleanup().await;
}

#[tokio::test]
async fn cleanup_aborts_pending_init_response_watchdog() {
    let (connection, mut messages, _) = fixture(builder());
    connection.init().await.unwrap();
    messages.try_recv().unwrap();
    let watchdog = connection
        .init_timeout_task
        .lock()
        .await
        .as_ref()
        .unwrap()
        .abort_handle();

    assert!(!watchdog.is_finished());
    connection.cleanup().await;
    assert!(connection.init_timeout_task.lock().await.is_none());
    timeout(Duration::from_secs(2), async {
        while !watchdog.is_finished() {
            yield_now().await;
        }
    })
    .await
    .unwrap();

    assert_eq!(connection.state.get(), ConnectionState::Closed);
}
