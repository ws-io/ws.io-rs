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

use tokio::time::sleep;

use super::{
    cleanup_e2e,
    create_connected_client,
    setup_server,
    wait_for_clients_disconnected,
};

#[tokio::test]
async fn test_e2e_disconnect_all() {
    let (server_task, server, ws_url) = setup_server().await;

    server.new_namespace_builder("/socket").register().unwrap();

    let client_a = create_connected_client(&ws_url).await;
    let client_b = create_connected_client(&ws_url).await;

    let clients = vec![client_a.clone(), client_b.clone()];

    // Disconnect all
    server.disconnect_all().await;

    // Wait for clients to disconnect
    let disconnected = wait_for_clients_disconnected(&clients).await;
    assert_eq!(disconnected, 2, "Both clients should be disconnected");

    cleanup_e2e(clients, server_task).await;
}

#[tokio::test]
async fn test_e2e_multiple_namespaces() {
    let (server_task, server, _ws_url) = setup_server().await;

    let _ns1 = server.new_namespace_builder("/namespace1").register().unwrap();
    let _ns2 = server.new_namespace_builder("/namespace2").register().unwrap();

    assert_eq!(server.namespace_count(), 2);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn test_e2e_broadcast_and_rooms() {
    let (server_task, server, ws_url) = setup_server().await;

    let server_namespace = server
        .new_namespace_builder("/socket")
        .on_connect(|ctx| async move {
            ctx.on("join_room", |event_ctx, room: Arc<String>| async move {
                event_ctx.join([room.as_str()]);
                event_ctx.emit::<()>("joined", None).await.unwrap();
                Ok(())
            });
            Ok(())
        })
        .register()
        .unwrap();

    // Setup Clients A, B, C
    let client_a = create_connected_client(&ws_url).await;
    let client_b = create_connected_client(&ws_url).await;
    let client_c = create_connected_client(&ws_url).await;

    let a_received_broadcast = Arc::new(AtomicUsize::new(0));
    let b_received_broadcast = Arc::new(AtomicUsize::new(0));
    let c_received_broadcast = Arc::new(AtomicUsize::new(0));

    let a_received_room = Arc::new(AtomicUsize::new(0));
    let b_received_room = Arc::new(AtomicUsize::new(0));
    let c_received_room = Arc::new(AtomicUsize::new(0));

    // Register handlers
    client_a.on("broadcast_msg", {
        let c = a_received_broadcast.clone();
        move |_ctx, _data: Arc<()>| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }
    });
    client_b.on("broadcast_msg", {
        let c = b_received_broadcast.clone();
        move |_ctx, _data: Arc<()>| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }
    });
    client_c.on("broadcast_msg", {
        let c = c_received_broadcast.clone();
        move |_ctx, _data: Arc<()>| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }
    });

    client_a.on("room_msg", {
        let c = a_received_room.clone();
        move |_ctx, _data: Arc<()>| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }
    });
    client_b.on("room_msg", {
        let c = b_received_room.clone();
        move |_ctx, _data: Arc<()>| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }
    });
    client_c.on("room_msg", {
        let c = c_received_room.clone();
        move |_ctx, _data: Arc<()>| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }
    });

    client_a.on("joined", |_ctx, _data: Arc<()>| async { Ok(()) });
    client_b.on("joined", |_ctx, _data: Arc<()>| async { Ok(()) });
    client_c.on("joined", |_ctx, _data: Arc<()>| async { Ok(()) });

    // A and B join "gaming" room
    client_a.emit("join_room", Some(&"gaming")).await.unwrap();
    client_b.emit("join_room", Some(&"gaming")).await.unwrap();

    sleep(Duration::from_millis(50)).await;

    // Test Room Broadcast
    server_namespace
        .to(["gaming"])
        .emit::<()>("room_msg", None)
        .await
        .unwrap();

    sleep(Duration::from_millis(50)).await;

    assert_eq!(
        a_received_room.load(Ordering::SeqCst),
        1,
        "Client A should receive room msg"
    );
    assert_eq!(
        b_received_room.load(Ordering::SeqCst),
        1,
        "Client B should receive room msg"
    );
    assert_eq!(
        c_received_room.load(Ordering::SeqCst),
        0,
        "Client C should NOT receive room msg"
    );

    // Test Global Broadcast
    server_namespace.emit::<()>("broadcast_msg", None).await.unwrap();
    sleep(Duration::from_millis(50)).await;

    assert_eq!(a_received_broadcast.load(Ordering::SeqCst), 1);
    assert_eq!(b_received_broadcast.load(Ordering::SeqCst), 1);
    assert_eq!(c_received_broadcast.load(Ordering::SeqCst), 1);

    cleanup_e2e(vec![client_a, client_b, client_c], server_task).await;
}

#[tokio::test]
async fn test_e2e_emit_with_data() {
    let (server_task, server, ws_url) = setup_server().await;

    let server_namespace = server.new_namespace_builder("/socket").register().unwrap();

    #[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
    struct Payload {
        message: String,
        count: u32,
    }

    let received = Arc::new(AtomicUsize::new(0));
    let received_clone = received.clone();

    let client = create_connected_client(&ws_url).await;
    client.on("data_event", move |_ctx, data: Arc<Payload>| {
        let count = received_clone.clone();
        async move {
            assert_eq!(data.message, "hello");
            assert_eq!(data.count, 42);
            count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    });

    server_namespace
        .emit(
            "data_event",
            Some(&Payload {
                message: "hello".into(),
                count: 42,
            }),
        )
        .await
        .unwrap();

    sleep(Duration::from_millis(50)).await;
    assert_eq!(received.load(Ordering::SeqCst), 1);

    cleanup_e2e(vec![client], server_task).await;
}

#[tokio::test]
async fn test_e2e_close_all() {
    let (server_task, server, ws_url) = setup_server().await;

    server.new_namespace_builder("/socket").register().unwrap();

    let client_a = create_connected_client(&ws_url).await;
    let client_b = create_connected_client(&ws_url).await;

    let clients = vec![client_a.clone(), client_b.clone()];

    // close_all should close all connections
    server.close_all().await;

    let disconnected = wait_for_clients_disconnected(&clients).await;
    assert_eq!(disconnected, 2);

    cleanup_e2e(clients, server_task).await;
}

#[tokio::test]
async fn test_e2e_remove_namespace() {
    let (server_task, server, _ws_url) = setup_server().await;

    let _ns = server.new_namespace_builder("/test").register().unwrap();
    assert_eq!(server.namespace_count(), 1);
    assert_eq!(server.of("/test").unwrap().path(), "/test");

    server.remove_namespace("/test").await;
    assert_eq!(server.namespace_count(), 0);
    assert!(server.of("/test").is_none());

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn test_e2e_client_disconnect() {
    let (server_task, server, ws_url) = setup_server().await;

    server.new_namespace_builder("/socket").register().unwrap();

    let client = create_connected_client(&ws_url).await;

    assert_eq!(server.connection_count(), 1);

    client.disconnect().await;

    // Wait for connection to be cleaned up
    let mut cleaned = false;
    for _ in 0..100 {
        if server.connection_count() == 0 {
            cleaned = true;
            break;
        }
        sleep(Duration::from_millis(10)).await;
    }
    assert!(cleaned, "Connection should be cleaned up after client disconnect");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn test_e2e_broadcast_chaining() {
    let (server_task, server, ws_url) = setup_server().await;

    server.new_namespace_builder("/socket").register().unwrap();

    let client_a = create_connected_client(&ws_url).await;
    let client_b = create_connected_client(&ws_url).await;

    let a_received = Arc::new(AtomicUsize::new(0));
    let b_received = Arc::new(AtomicUsize::new(0));

    client_a.on("msg", {
        let c = a_received.clone();
        move |_ctx, _data: Arc<()>| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }
    });
    client_b.on("msg", {
        let c = b_received.clone();
        move |_ctx, _data: Arc<()>| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }
    });

    // Test chaining: .to().emit()
    server
        .of("/socket")
        .unwrap()
        .to(["room1"])
        .emit::<()>("msg", None)
        .await
        .unwrap();

    sleep(Duration::from_millis(50)).await;
    // Neither should receive since they're not in room1
    assert_eq!(a_received.load(Ordering::SeqCst), 0);
    assert_eq!(b_received.load(Ordering::SeqCst), 0);

    cleanup_e2e(vec![client_a, client_b], server_task).await;
}

#[tokio::test]
async fn test_e2e_on_ready_handler() {
    let (server_task, server, ws_url) = setup_server().await;

    let ready_called = Arc::new(AtomicUsize::new(0));
    let ready_called_clone = ready_called.clone();

    let mut namespace_builder = server.new_namespace_builder("/socket");
    namespace_builder = namespace_builder.on_ready(move |_ctx| {
        let c = ready_called_clone.clone();
        async move {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    });
    let _ns = namespace_builder.register().unwrap();

    let client = create_connected_client(&ws_url).await;

    // Give on_ready time to execute
    sleep(Duration::from_millis(50)).await;
    assert_eq!(ready_called.load(Ordering::SeqCst), 1);

    cleanup_e2e(vec![client], server_task).await;
}

#[tokio::test]
async fn test_e2e_on_close_handler() {
    let (server_task, server, ws_url) = setup_server().await;

    let close_called = Arc::new(AtomicUsize::new(0));
    let close_called_clone = close_called.clone();

    let mut namespace_builder = server.new_namespace_builder("/socket");
    namespace_builder = namespace_builder.on_connect(move |ctx| {
        let c = close_called_clone.clone();
        async move {
            ctx.on_close(move |_ctx| {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            })
            .await;
            Ok(())
        }
    });
    let _ns = namespace_builder.register().unwrap();

    let client = create_connected_client(&ws_url).await;

    client.disconnect().await;

    // Wait for close handler to be called
    let mut called = false;
    for _ in 0..100 {
        if close_called.load(Ordering::SeqCst) == 1 {
            called = true;
            break;
        }
        sleep(Duration::from_millis(10)).await;
    }
    assert!(called, "on_close handler should be called");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn test_e2e_except_room_broadcast() {
    let (server_task, server, ws_url) = setup_server().await;

    let server_namespace = server
        .new_namespace_builder("/socket")
        .on_connect(|ctx| async move {
            ctx.on("join", |event_ctx, room: Arc<String>| async move {
                event_ctx.join([room.as_str()]);
                Ok(())
            });
            Ok(())
        })
        .register()
        .unwrap();

    let client_a = create_connected_client(&ws_url).await;
    let client_b = create_connected_client(&ws_url).await;
    let client_c = create_connected_client(&ws_url).await;

    let a_received = Arc::new(AtomicUsize::new(0));
    let b_received = Arc::new(AtomicUsize::new(0));
    let c_received = Arc::new(AtomicUsize::new(0));

    for (client, count) in [
        (&client_a, a_received.clone()),
        (&client_b, b_received.clone()),
        (&client_c, c_received.clone()),
    ] {
        let c = count.clone();
        client.on("msg", move |_ctx, _data: Arc<()>| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        });
    }

    // A and B join room1, C joins room2
    client_a.emit("join", Some(&"room1")).await.unwrap();
    client_b.emit("join", Some(&"room1")).await.unwrap();
    client_c.emit("join", Some(&"room2")).await.unwrap();

    sleep(Duration::from_millis(50)).await;

    // Broadcast to room1, except room2
    server_namespace
        .to(["room1"])
        .except(["room2"])
        .emit::<()>("msg", None)
        .await
        .unwrap();

    sleep(Duration::from_millis(50)).await;
    assert_eq!(a_received.load(Ordering::SeqCst), 1);
    assert_eq!(b_received.load(Ordering::SeqCst), 1);
    assert_eq!(c_received.load(Ordering::SeqCst), 0);

    cleanup_e2e(vec![client_a, client_b, client_c], server_task).await;
}

#[tokio::test]
async fn test_e2e_namespace_connection_count() {
    let (server_task, server, ws_url) = setup_server().await;

    server.new_namespace_builder("/socket").register().unwrap();

    assert_eq!(server.connection_count(), 0);

    let client = create_connected_client(&ws_url).await;

    assert_eq!(server.connection_count(), 1);

    cleanup_e2e(vec![client], server_task).await;
}

#[tokio::test]
async fn test_e2e_server_connection_count() {
    let (server_task, server, ws_url) = setup_server().await;

    // Register /socket namespace so clients can connect
    server.new_namespace_builder("/socket").register().unwrap();

    let client1 = create_connected_client(&ws_url).await;
    let client2 = create_connected_client(&ws_url).await;

    assert_eq!(server.connection_count(), 2);

    cleanup_e2e(vec![client1, client2], server_task).await;
}
