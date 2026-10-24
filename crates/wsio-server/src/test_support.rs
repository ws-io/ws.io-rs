use std::{
    future::pending,
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use http::{
    HeaderMap,
    Uri,
};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::{
    connection::WsIoServerConnection,
    core::packet::WsIoPacket,
    namespace::WsIoServerNamespace,
};

// Constants/Statics
pub(crate) const TEST_TIMEOUT: Duration = Duration::from_secs(2);

// Functions

/// Reports that it started, then stays pending while holding `alive` until dropped.
pub(crate) async fn park_until_dropped(
    started: mpsc::UnboundedSender<()>,
    _alive: mpsc::UnboundedSender<()>,
) -> Result<()> {
    started.send(()).unwrap();
    pending().await
}

/// Completes the server handshake through the namespace codec and transformer.
pub(crate) async fn ready_connection(
    namespace: &Arc<WsIoServerNamespace>,
) -> (Arc<WsIoServerConnection>, mpsc::Receiver<Arc<Message>>) {
    let (connection, mut messages, _events) =
        WsIoServerConnection::new(HeaderMap::new(), Arc::clone(namespace), Uri::from_static("/ws.io"));

    connection.init().await.unwrap();
    messages.try_recv().unwrap();
    let response = namespace
        .config
        .packet_codec
        .encode(&WsIoPacket::new_init(None))
        .unwrap();

    let response = namespace.config.packet_transformer.encode(response).await.unwrap();
    connection.handle_incoming_packet(response).await.unwrap();
    messages.try_recv().unwrap();
    (connection, messages)
}
