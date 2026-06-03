use std::{
    sync::Arc,
    time::Duration,
};

use axum::{
    Router,
    serve,
};
use tokio::{
    net::TcpListener,
    spawn,
    task::JoinHandle,
    time::{
        sleep,
        timeout,
    },
};
use wsio_client::WsIoClient;
use wsio_server::{
    WsIoServer,
    namespace::WsIoServerNamespace,
};

mod broadcast;
mod ping_pong;
mod reconnect;

const CLIENT_STATE_TIMEOUT: Duration = Duration::from_secs(2);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const TEST_NAMESPACE: &str = "/socket";

async fn setup_server() -> (JoinHandle<()>, Arc<WsIoServer>, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    let ws_url = format!("ws://{}{}", local_addr, TEST_NAMESPACE);

    let server_builder = WsIoServer::builder();
    let server = Arc::new(server_builder.build());

    // Create Axum Router and attach the WsIoServer Layer
    let app = Router::new().layer(server.layer());

    // Accept connections in the background
    let server_task = spawn(async move {
        serve(listener, app).await.unwrap();
    });

    (server_task, server, ws_url)
}

fn register_test_namespace(server: &WsIoServer) -> Arc<WsIoServerNamespace> {
    server.new_namespace_builder(TEST_NAMESPACE).register().unwrap()
}

async fn create_connected_client(ws_url: &str) -> WsIoClient {
    let client = WsIoClient::builder(ws_url).unwrap().build();
    client.connect().await;
    wait_for_client_ready(&client).await;
    client
}

async fn wait_for_client_ready(client: &WsIoClient) {
    wait_for_condition(|| client.is_session_ready())
        .await
        .expect("client session should become ready before timeout");
}

async fn wait_for_clients_disconnected(clients: &[WsIoClient]) -> usize {
    if wait_for_condition(|| clients.iter().all(|client| !client.is_session_ready()))
        .await
        .is_ok()
    {
        clients.len()
    } else {
        0
    }
}

async fn cleanup_e2e(clients: Vec<WsIoClient>, server_task: JoinHandle<()>) {
    for client in clients {
        client.disconnect().await;
    }

    cleanup_server_task(server_task).await;
}

async fn cleanup_server_task(server_task: JoinHandle<()>) {
    server_task.abort();
    let _ = server_task.await;
}

async fn wait_for_condition(mut condition: impl FnMut() -> bool) -> Result<(), tokio::time::error::Elapsed> {
    timeout(CLIENT_STATE_TIMEOUT, async {
        while !condition() {
            sleep(POLL_INTERVAL).await;
        }
    })
    .await
}
