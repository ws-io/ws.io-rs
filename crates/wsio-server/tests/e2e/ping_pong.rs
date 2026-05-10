use std::{
    sync::Arc,
    time::Duration,
};

use tokio::{
    sync::{
        Mutex,
        oneshot::channel,
    },
    time::timeout,
};
use wsio_client::WsIoClient;

use super::{
    setup_server,
    wait_for_client_ready,
};

#[tokio::test]
async fn test_e2e_ping_pong() {
    // 1. Setup Server
    let (server_task, server, ws_url) = setup_server().await;

    // Register a default namespace "/socket"
    let namespace_builder = server.new_namespace_builder("/socket");
    namespace_builder
        .on_connect(|ctx| async move {
            ctx.on("ping", |event_ctx, _data: Arc<()>| async move {
                // Echo back a pong
                event_ctx.emit::<()>("pong", None).await.unwrap();
                Ok(())
            });

            Ok(())
        })
        .register()
        .unwrap();

    // 2. Setup Client
    let client = WsIoClient::builder(ws_url.as_str()).unwrap().build();

    let (tx, rx) = channel();
    let tx = Arc::new(Mutex::new(Some(tx)));

    client.on("pong", move |_ctx, _data: Arc<()>| {
        let tx = tx.clone();
        async move {
            if let Some(sender) = tx.lock().await.take() {
                let _ = sender.send(());
            }

            Ok(())
        }
    });

    // Connect Client
    client.connect().await;

    wait_for_client_ready(&client).await;

    // Emit ping
    client.emit::<()>("ping", None).await.unwrap();

    // 3. Verify
    // Await the pong event to fire the oneshot channel
    timeout(Duration::from_secs(2), rx)
        .await
        .expect("Test timed out waiting for pong")
        .expect("Channel closed");

    // Cleanup
    client.disconnect().await;
    server_task.abort();
    let _ = server_task.await;
}
