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
    time::sleep,
};
use wsio_client::WsIoClient;
use wsio_server::WsIoServer;

mod broadcast;
mod ping_pong;
mod reconnect;

pub(self) async fn setup_server() -> (JoinHandle<()>, Arc<WsIoServer>, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    let ws_url = format!("ws://{}/socket", local_addr);

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

// ============================================================================
// Client Helpers
// ============================================================================

/// Create and connect a client, waiting for session to be ready
pub(self) async fn create_connected_client(ws_url: &str) -> WsIoClient {
    let client = WsIoClient::builder(ws_url).unwrap().build();
    client.connect().await;
    while !client.is_session_ready() {
        sleep(Duration::from_millis(10)).await;
    }
    client
}

/// Wait for all given clients to have their sessions NOT ready (disconnected)
/// Returns the number of clients confirmed disconnected
pub(self) async fn wait_for_clients_disconnected(clients: &[WsIoClient]) -> usize {
    for _ in 0..100 {
        if clients.iter().all(|c| !c.is_session_ready()) {
            return clients.len();
        }
        sleep(Duration::from_millis(10)).await;
    }

    0 // timeout
}

/// Disconnect all clients and abort server task
pub(self) async fn cleanup_e2e(clients: Vec<WsIoClient>, server_task: JoinHandle<()>) {
    for client in clients {
        client.disconnect().await;
    }

    server_task.abort();
    let _ = server_task.await;
}
