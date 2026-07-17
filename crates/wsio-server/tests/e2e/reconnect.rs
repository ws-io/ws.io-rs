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

use wsio_client::WsIoClient;

use super::{
    TEST_NAMESPACE,
    assert_counters_stay_at,
    cleanup_e2e,
    setup_server,
    wait_for_client_ready,
    wait_for_condition,
    wait_for_counter,
};

#[tokio::test]
async fn test_e2e_client_reconnect() {
    // 1. Setup Server
    let (server_task, server, ws_url) = setup_server().await;

    // Register the default test namespace.
    let survivor_msg_count = Arc::new(AtomicUsize::new(0));
    let survivor_msg_count_clone = survivor_msg_count.clone();
    server
        .new_namespace_builder(TEST_NAMESPACE)
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

    wait_for_client_ready(&client).await;

    // Phase 2: Forced Disconnect
    // The server forcibly drops the tcp connection by calling close_all
    server.close_all().await;

    wait_for_condition(|| !client.is_session_ready())
        .await
        .expect("client should disconnect after server close_all");

    // Phase 3: Buffering
    // Immediately emit an event while the client is still offline and trying to reconnect.
    // The WsIoClientRuntime's underlying send_event_message_task should buffer this and block
    client.emit::<()>("survivor_msg", None).await.unwrap();

    wait_for_client_ready(&client).await;

    // Phase 5: Verification
    // The server received the buffered message exactly once after the reconnect.
    wait_for_counter(&survivor_msg_count, 1).await;
    assert_counters_stay_at(&[&survivor_msg_count], 1).await;

    cleanup_e2e(vec![client], server_task).await;
}
