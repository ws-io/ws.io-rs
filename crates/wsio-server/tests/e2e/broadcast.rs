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
use wsio_client::WsIoClient;

use super::setup_server;

#[tokio::test]
async fn test_e2e_broadcast_and_rooms() {
    // 1. Setup Server
    let (server_task, server, ws_url) = setup_server().await;

    // Register a default namespace "/socket"
    let namespace_builder = server.new_namespace_builder("/socket");
    let server_namespace = namespace_builder
        .on_connect(|ctx| async move {
            // When client sends "join_room", server puts them into the specified room
            ctx.on("join_room", |event_ctx, room: Arc<String>| async move {
                event_ctx.join([room.as_str()]);
                event_ctx.emit::<()>("joined", None).await.unwrap();
                Ok(())
            });

            Ok(())
        })
        .register()
        .unwrap();

    // 2. Setup Clients A, B, C
    let client_a = WsIoClient::builder(ws_url.as_str()).unwrap().build();
    let client_b = WsIoClient::builder(ws_url.as_str()).unwrap().build();
    let client_c = WsIoClient::builder(ws_url.as_str()).unwrap().build();

    let a_received_broadcast = Arc::new(AtomicUsize::new(0));
    let b_received_broadcast = Arc::new(AtomicUsize::new(0));
    let c_received_broadcast = Arc::new(AtomicUsize::new(0));

    let a_received_room = Arc::new(AtomicUsize::new(0));
    let b_received_room = Arc::new(AtomicUsize::new(0));
    let c_received_room = Arc::new(AtomicUsize::new(0));

    // Common setup for each client
    for (client, broadcast_count, room_count) in [
        (&client_a, a_received_broadcast.clone(), a_received_room.clone()),
        (&client_b, b_received_broadcast.clone(), b_received_room.clone()),
        (&client_c, c_received_broadcast.clone(), c_received_room.clone()),
    ] {
        let broadcast_count_clone = broadcast_count.clone();
        client.on("broadcast_msg", move |_ctx, _data: Arc<()>| {
            let count = broadcast_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        });

        let room_count_clone = room_count.clone();
        client.on("room_msg", move |_ctx, _data: Arc<()>| {
            let count = room_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        });

        // Acknowledgement for joining a room
        client.on("joined", |_ctx, _data: Arc<()>| async { Ok(()) });

        client.connect().await;
    }

    // Wait until all clients are completely ready
    while !client_a.is_session_ready() || !client_b.is_session_ready() || !client_c.is_session_ready() {
        sleep(Duration::from_millis(10)).await;
    }

    // A and B join "gaming" room
    client_a.emit("join_room", Some(&"gaming")).await.unwrap();
    client_b.emit("join_room", Some(&"gaming")).await.unwrap();

    // Wait for the server to process the join room events (mocking real time delay)
    sleep(Duration::from_millis(50)).await;

    // 3. Test Room Broadcast
    server_namespace
        .to(["gaming"])
        .emit::<()>("room_msg", None)
        .await
        .unwrap();

    sleep(Duration::from_millis(50)).await;

    // Verify only A and B received the room message
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

    // 4. Test Global Broadcast
    server_namespace.emit::<()>("broadcast_msg", None).await.unwrap();
    sleep(Duration::from_millis(50)).await;

    // Verify all clients received the global broadcast
    assert_eq!(
        a_received_broadcast.load(Ordering::SeqCst),
        1,
        "Client A should receive global broadcast"
    );

    assert_eq!(
        b_received_broadcast.load(Ordering::SeqCst),
        1,
        "Client B should receive global broadcast"
    );

    assert_eq!(
        c_received_broadcast.load(Ordering::SeqCst),
        1,
        "Client C should receive global broadcast"
    );

    // Cleanup
    client_a.disconnect().await;
    client_b.disconnect().await;
    client_c.disconnect().await;
    server_task.abort();
    let _ = server_task.await;
}
