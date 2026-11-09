use std::{
    convert::Infallible,
    future::pending,
    pin::Pin,
    sync::Arc,
    task::{
        Context,
        Poll,
    },
    time::Duration,
};

use anyhow::Result;
use bytes::Bytes;
use http::{
    HeaderMap,
    Uri,
};
use http_body::{
    Body,
    Frame,
};
use tokio_tungstenite::tungstenite::Message;

use crate::{
    connection::WsIoServerConnection,
    core::packet::WsIoPacket,
    namespace::WsIoServerNamespace,
};

// Constants/Statics
pub(crate) const TEST_TIMEOUT: Duration = Duration::from_secs(2);

// Structs
#[derive(Debug, Default)]
pub(crate) struct TestBody(pub(crate) Option<Bytes>);

impl Body for TestBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        Poll::Ready(self.0.take().map(|bytes| Ok(Frame::data(bytes))))
    }
}

// Functions

/// Reports that it started, then stays pending while holding `alive` until dropped.
pub(crate) async fn park_until_dropped(
    started: tokio::sync::mpsc::UnboundedSender<()>,
    _alive: tokio::sync::mpsc::UnboundedSender<()>,
) -> Result<()> {
    started.send(()).unwrap();
    pending().await
}

/// Completes the server handshake through the namespace codec and transformer.
pub(crate) async fn ready_connection(
    namespace: &Arc<WsIoServerNamespace>,
) -> (Arc<WsIoServerConnection>, tokio::sync::mpsc::Receiver<Arc<Message>>) {
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
