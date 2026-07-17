use std::sync::{
    Arc,
    atomic::{
        AtomicUsize,
        Ordering,
    },
};

use serde::{
    Deserialize,
    Serialize,
};
use wsio_client::WsIoClient;

use super::{
    TEST_NAMESPACE,
    assert_counters_stay_at,
    cleanup_e2e,
    cleanup_server_task,
    create_connected_client,
    register_test_namespace,
    setup_server,
    wait_for_clients_disconnected,
    wait_for_condition,
    wait_for_counter,
};

fn register_unit_counter(client: &WsIoClient, event: &str, counter: Arc<AtomicUsize>) {
    client.on(event, move |_ctx, _data: Arc<()>| {
        let counter = counter.clone();
        async move {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    });
}

#[tokio::test]
async fn test_e2e_disconnect_all() {
    let (server_task, server, ws_url) = setup_server().await;

    register_test_namespace(&server);

    let client_a = create_connected_client(&ws_url).await;
    let client_b = create_connected_client(&ws_url).await;

    let clients = vec![client_a, client_b];

    // Disconnect all
    server.disconnect_all().await;

    // Wait for clients to disconnect
    wait_for_clients_disconnected(&clients).await;

    cleanup_e2e(clients, server_task).await;
}

#[tokio::test]
async fn test_e2e_broadcast_and_rooms() {
    let (server_task, server, ws_url) = setup_server().await;

    let server_namespace = server
        .new_namespace_builder(TEST_NAMESPACE)
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
    register_unit_counter(&client_a, "broadcast_msg", a_received_broadcast.clone());
    register_unit_counter(&client_b, "broadcast_msg", b_received_broadcast.clone());
    register_unit_counter(&client_c, "broadcast_msg", c_received_broadcast.clone());
    register_unit_counter(&client_a, "room_msg", a_received_room.clone());
    register_unit_counter(&client_b, "room_msg", b_received_room.clone());
    register_unit_counter(&client_c, "room_msg", c_received_room.clone());

    let joined = Arc::new(AtomicUsize::new(0));
    register_unit_counter(&client_a, "joined", joined.clone());
    register_unit_counter(&client_b, "joined", joined.clone());

    // A and B join "gaming" room
    client_a.emit("join_room", Some(&"gaming")).await.unwrap();
    client_b.emit("join_room", Some(&"gaming")).await.unwrap();

    wait_for_counter(&joined, 2).await;

    // Test Room Broadcast
    server_namespace
        .to(["gaming"])
        .emit::<()>("room_msg", None)
        .await
        .unwrap();

    wait_for_counter(&a_received_room, 1).await;
    wait_for_counter(&b_received_room, 1).await;

    assert_counters_stay_at(&[&a_received_room, &b_received_room], 1).await;
    assert_eq!(c_received_room.load(Ordering::SeqCst), 0);

    // Test Global Broadcast
    server_namespace.emit::<()>("broadcast_msg", None).await.unwrap();
    wait_for_counter(&a_received_broadcast, 1).await;
    wait_for_counter(&b_received_broadcast, 1).await;
    wait_for_counter(&c_received_broadcast, 1).await;

    assert_counters_stay_at(
        &[&a_received_broadcast, &b_received_broadcast, &c_received_broadcast],
        1,
    )
    .await;
    cleanup_e2e(vec![client_a, client_b, client_c], server_task).await;
}

#[tokio::test]
async fn test_e2e_emit_with_data() {
    #[derive(Debug, Deserialize, Serialize)]
    struct Payload {
        message: String,
        count: u32,
    }

    let (server_task, server, ws_url) = setup_server().await;

    let server_namespace = register_test_namespace(&server);

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

    wait_for_counter(&received, 1).await;
    assert_counters_stay_at(&[&received], 1).await;

    cleanup_e2e(vec![client], server_task).await;
}

#[tokio::test]
async fn test_e2e_close_all() {
    let (server_task, server, ws_url) = setup_server().await;

    register_test_namespace(&server);

    let client_a = create_connected_client(&ws_url).await;
    let client_b = create_connected_client(&ws_url).await;

    let clients = vec![client_a, client_b];

    // close_all should close all connections
    server.close_all().await;

    wait_for_clients_disconnected(&clients).await;

    cleanup_e2e(clients, server_task).await;
}

#[tokio::test]
async fn test_e2e_client_disconnect() {
    let (server_task, server, ws_url) = setup_server().await;

    register_test_namespace(&server);

    let client = create_connected_client(&ws_url).await;

    assert_eq!(server.connection_count(), 1);

    client.disconnect().await;

    // Wait for connection to be cleaned up
    wait_for_condition(|| server.connection_count() == 0)
        .await
        .expect("connection should be cleaned up after client disconnect");

    cleanup_server_task(server_task).await;
}

#[tokio::test]
async fn test_e2e_on_ready_handler() {
    let (server_task, server, ws_url) = setup_server().await;

    let ready_called = Arc::new(AtomicUsize::new(0));
    let ready_called_clone = ready_called.clone();

    server
        .new_namespace_builder(TEST_NAMESPACE)
        .on_ready(move |_ctx| {
            let c = ready_called_clone.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .register()
        .unwrap();

    let client = create_connected_client(&ws_url).await;

    wait_for_counter(&ready_called, 1).await;
    assert_counters_stay_at(&[&ready_called], 1).await;

    cleanup_e2e(vec![client], server_task).await;
}

#[tokio::test]
async fn test_e2e_on_close_handler() {
    let (server_task, server, ws_url) = setup_server().await;

    let close_called = Arc::new(AtomicUsize::new(0));
    let close_called_clone = close_called.clone();

    server
        .new_namespace_builder(TEST_NAMESPACE)
        .on_connect(move |ctx| {
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
        })
        .register()
        .unwrap();

    let client = create_connected_client(&ws_url).await;

    client.disconnect().await;

    // Wait for close handler to be called
    wait_for_condition(|| close_called.load(Ordering::SeqCst) == 1)
        .await
        .expect("on_close handler should be called");

    cleanup_server_task(server_task).await;
}

#[tokio::test]
async fn test_e2e_server_connection_count() {
    let (server_task, server, ws_url) = setup_server().await;

    // Register /socket namespace so clients can connect
    register_test_namespace(&server);

    assert_eq!(server.connection_count(), 0);

    let client1 = create_connected_client(&ws_url).await;
    assert_eq!(server.connection_count(), 1);

    let client2 = create_connected_client(&ws_url).await;

    assert_eq!(server.connection_count(), 2);

    cleanup_e2e(vec![client1, client2], server_task).await;
}
