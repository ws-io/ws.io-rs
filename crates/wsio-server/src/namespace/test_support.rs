use http::{
    HeaderMap,
    Uri,
};
use tokio::sync::mpsc;

use super::*;

/// Completes the server handshake through the namespace codec and transformer.
pub(super) async fn ready_connection(
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
