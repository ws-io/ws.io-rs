use std::sync::Arc;

use axum::{
    Router,
    serve,
};
use tokio::{
    net::TcpListener,
    spawn,
    task::JoinHandle,
};
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
