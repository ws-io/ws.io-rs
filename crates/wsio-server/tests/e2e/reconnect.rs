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
async fn test_e2e_client_reconnect() {
    // 1. Setup Server
    let (server_task, server, ws_url) = setup_server().await;

    // Register a default namespace "/socket"
    let namespace_builder = server.new_namespace_builder("/socket");
    let survivor_msg_count = Arc::new(AtomicUsize::new(0));
    let survivor_msg_count_clone = survivor_msg_count.clone();
    namespace_builder
        .on_connect(move |ctx| {
            let survivor_msg_count_clone = survivor_msg_count_clone.clone();
            async move {
                ctx.on("survivor_msg", move |_ctx, _data: Arc<()>| {
                    let count = survivor_msg_count_clone.clone();
                    async move {
                        count.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    }
                });

                Ok(())
            }
        })
        .register()
        .unwrap();

    // 2. Setup Client
    // Set a very aggressive reconnect delay for fast testing
    let client = WsIoClient::builder(ws_url.as_str())
        .unwrap()
        .reconnect_delay(Duration::from_millis(250))
        .build();

    client.connect().await;

    // Phase 1: Wait until the client session is ready for the first time
    while !client.is_session_ready() {
        sleep(Duration::from_millis(10)).await;
    }

    // Phase 2: Forced Disconnect
    // The server forcibly drops the tcp connection by calling close_all
    server.close_all().await;

    // Give it a moment for the client's internal sockets to detect closure
    // We loop and wait until it is definitively offline. Timeout if it takes too long.
    let mut off_loops = 0;
    while client.is_session_ready() {
        sleep(Duration::from_millis(10)).await;
        off_loops += 1;
        if off_loops > 100 {
            panic!("Client did not disconnect as expected after server drop.");
        }
    }

    // Phase 3: Buffering
    // Immedately emit an event while the client is still offline and trying to reconnect
    // The WsIoClientRuntime's underlying send_event_message_task should buffer this and block
    client.emit::<()>("survivor_msg", None).await.unwrap();

    // Phase 4: Recovery
    // Await the client's connection_loop_task to rebuild the session
    while !client.is_session_ready() {
        sleep(Duration::from_millis(10)).await;
    }

    // Give the client event sender task a moment to flush the buffered event.
    sleep(Duration::from_millis(50)).await;

    // Phase 5: Verification
    // Verify that the server successfully received the buffered message AFTER the reconnect
    assert_eq!(
        survivor_msg_count.load(Ordering::SeqCst),
        1,
        "Server should receive the buffered 'survivor_msg' after client reconnects"
    );

    // Cleanup
    client.disconnect().await;
    server_task.abort();
    let _ = server_task.await;
}
