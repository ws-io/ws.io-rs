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
const EVENT_QUIET_PERIOD: Duration = Duration::from_millis(100);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const TEST_NAMESPACE: &str = "/socket";

async fn setup_server() -> (JoinHandle<()>, Arc<WsIoServer>, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    let ws_url = format!("ws://{}{}", local_addr, TEST_NAMESPACE);

    let server = Arc::new(WsIoServer::builder().build());

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

async fn wait_for_clients_disconnected(clients: &[WsIoClient]) {
    wait_for_condition(|| clients.iter().all(|client| !client.is_session_ready()))
        .await
        .expect("clients should disconnect before timeout");
}

async fn wait_for_counter(counter: &AtomicUsize, expected: usize) {
    wait_for_condition(|| counter.load(Ordering::SeqCst) >= expected)
        .await
        .expect("event counter should reach expected value before timeout");
}

async fn assert_counters_stay_at(counters: &[&AtomicUsize], expected: usize) {
    timeout(EVENT_QUIET_PERIOD, async {
        while counters
            .iter()
            .all(|counter| counter.load(Ordering::SeqCst) <= expected)
        {
            sleep(POLL_INTERVAL).await;
        }
    })
    .await
    .expect_err("event counters should not exceed expected value");

    for counter in counters {
        assert_eq!(counter.load(Ordering::SeqCst), expected);
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
