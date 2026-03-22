use std::sync::Arc;

use anyhow::Result;
use arc_swap::ArcSwap;
use futures_util::{
    SinkExt,
    StreamExt,
};
use http::{
    HeaderMap,
    Uri,
};
use hyper::upgrade::{
    OnUpgrade,
    Upgraded,
};
use hyper_util::rt::TokioIo;
use kikiutils::{
    atomic::enum_cell::AtomicEnumCell,
    types::fx_collections::FxDashMap,
};
use num_enum::{
    IntoPrimitive,
    TryFromPrimitive,
};
use roaring::RoaringTreemap;
use serde::Serialize;
use tokio::{
    join,
    select,
    spawn,
    sync::Mutex,
    task::JoinSet,
    time::timeout,
};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{
        Message,
        protocol::Role,
    },
};

pub(crate) mod builder;
mod config;
pub mod operators;

use self::{
    config::WsIoServerNamespaceConfig,
    operators::broadcast::WsIoServerNamespaceBroadcastOperator,
};
use crate::{
    WsIoServer,
    connection::WsIoServerConnection,
    core::packet::WsIoPacket,
    runtime::{
        WsIoServerRuntime,
        WsIoServerRuntimeStatus,
    },
};

// Enums
#[repr(u8)]
#[derive(Debug, Eq, IntoPrimitive, PartialEq, TryFromPrimitive)]
enum NamespaceStatus {
    Running,
    Stopped,
    Stopping,
}

// Structs
pub struct WsIoServerNamespace {
    pub(crate) config: WsIoServerNamespaceConfig,
    connection_ids: ArcSwap<RoaringTreemap>,
    connections: FxDashMap<u64, Arc<WsIoServerConnection>>,
    connection_task_set: Mutex<JoinSet<()>>,
    rooms: FxDashMap<String, RoaringTreemap>,
    runtime: Arc<WsIoServerRuntime>,
    status: AtomicEnumCell<NamespaceStatus>,
}

impl WsIoServerNamespace {
    fn new(config: WsIoServerNamespaceConfig, runtime: Arc<WsIoServerRuntime>) -> Arc<Self> {
        Arc::new(Self {
            config,
            connection_ids: ArcSwap::new(Arc::new(RoaringTreemap::new())),
            connections: FxDashMap::default(),
            connection_task_set: Mutex::new(JoinSet::new()),
            rooms: FxDashMap::default(),
            runtime,
            status: AtomicEnumCell::new(NamespaceStatus::Running),
        })
    }

    // Private methods
    async fn handle_upgraded_request(
        self: &Arc<Self>,
        headers: HeaderMap,
        request_uri: Uri,
        upgraded: Upgraded,
    ) -> Result<()> {
        // Create ws stream
        let mut ws_stream =
            WebSocketStream::from_raw_socket(TokioIo::new(upgraded), Role::Server, Some(self.config.websocket_config))
                .await;

        // Check runtime and namespace status
        if !self.runtime.status.is(WsIoServerRuntimeStatus::Running) || !self.status.is(NamespaceStatus::Running) {
            ws_stream
                .send((*self.encode_packet_to_message(&WsIoPacket::new_disconnect())?).clone())
                .await?;

            let _ = ws_stream.close(None).await;
            return Ok(());
        }

        // Create connection
        let (connection, mut message_rx) = WsIoServerConnection::new(headers, self.clone(), request_uri);

        // Split ws stream and spawn read and write tasks
        let (mut ws_stream_writer, mut ws_stream_reader) = ws_stream.split();
        let connection_clone = connection.clone();
        let mut read_ws_stream_task = spawn(async move {
            while let Some(message) = ws_stream_reader.next().await {
                if match message {
                    Ok(Message::Binary(bytes)) => {
                        // Treat any single-byte binary frame as a client heartbeat and ignore it
                        if bytes.len() == 1 {
                            continue;
                        }

                        connection_clone.handle_incoming_packet(&bytes).await
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(Message::Text(text)) => connection_clone.handle_incoming_packet(text.as_bytes()).await,
                    Err(_) => break,
                    _ => Ok(()),
                }
                .is_err()
                {
                    break;
                }
            }
        });

        let mut write_ws_stream_task = spawn(async move {
            while let Some(message) = message_rx.recv().await {
                let message = (*message).clone();
                let is_close = matches!(message, Message::Close(_));
                if ws_stream_writer.send(message).await.is_err() {
                    break;
                }

                if is_close {
                    let _ = ws_stream_writer.close().await;
                    break;
                }
            }
        });

        // Try to init connection
        match connection.init().await {
            Ok(_) => {
                // Wait for either read or write task to finish
                select! {
                    _ = &mut read_ws_stream_task => {
                        write_ws_stream_task.abort();
                    },
                    _ = &mut write_ws_stream_task => {
                        read_ws_stream_task.abort();
                    },
                }
            }
            Err(_) => {
                // Close connection
                read_ws_stream_task.abort();
                connection.close();
                let _ = join!(read_ws_stream_task, write_ws_stream_task);
            }
        }

        // Cleanup connection
        connection.cleanup().await;
        Ok(())
    }

    // Protected methods
    #[inline]
    pub(crate) fn add_connection_id_to_room(&self, room_name: &str, connection_id: u64) {
        self.rooms.entry(room_name.into()).or_default().insert(connection_id);
    }

    #[inline]
    pub(crate) fn encode_packet_to_message(&self, packet: &WsIoPacket) -> Result<Arc<Message>> {
        let bytes = self.config.packet_codec.encode(packet)?;
        Ok(Arc::new(match self.config.packet_codec.is_text() {
            true => Message::Text(unsafe { String::from_utf8_unchecked(bytes).into() }),
            false => Message::Binary(bytes.into()),
        }))
    }

    pub(crate) async fn handle_on_upgrade_request(
        self: &Arc<Self>,
        headers: HeaderMap,
        on_upgrade: OnUpgrade,
        request_uri: Uri,
    ) {
        let namespace = self.clone();
        self.connection_task_set.lock().await.spawn(async move {
            if let Ok(Ok(upgraded)) = timeout(namespace.config.http_request_upgrade_timeout, on_upgrade).await {
                let _ = namespace.handle_upgraded_request(headers, request_uri, upgraded).await;
            }
        });
    }

    #[inline]
    pub(crate) fn insert_connection(&self, connection: Arc<WsIoServerConnection>) {
        self.connections.insert(connection.id(), connection.clone());
        self.runtime.insert_connection_id(connection.id());
        self.connection_ids.rcu(|old_connection_ids| {
            let mut new_connection_ids = (**old_connection_ids).clone();
            new_connection_ids.insert(connection.id());
            new_connection_ids
        });
    }

    #[inline]
    pub(crate) fn remove_connection(&self, id: u64) {
        self.connections.remove(&id);
        self.runtime.remove_connection_id(id);
        self.connection_ids.rcu(|old_connection_ids| {
            let mut new_connection_ids = (**old_connection_ids).clone();
            new_connection_ids.remove(id);
            new_connection_ids
        });
    }

    #[inline]
    pub(crate) fn remove_connection_id_from_room(&self, room_name: &str, connection_id: u64) {
        if let Some(mut entry) = self.rooms.get_mut(room_name) {
            entry.remove(connection_id);
        }

        self.rooms.remove_if(room_name, |_, entry| entry.is_empty());
    }

    // Public methods
    pub async fn close_all(self: &Arc<Self>) {
        WsIoServerNamespaceBroadcastOperator::new(self.clone()).close().await;
    }

    #[inline]
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    pub async fn disconnect_all(self: &Arc<Self>) -> Result<()> {
        WsIoServerNamespaceBroadcastOperator::new(self.clone())
            .disconnect()
            .await
    }

    pub async fn emit<D: Serialize>(self: &Arc<Self>, event: impl AsRef<str>, data: Option<&D>) -> Result<()> {
        WsIoServerNamespaceBroadcastOperator::new(self.clone())
            .emit(event, data)
            .await
    }

    #[inline]
    pub fn except(
        self: &Arc<Self>,
        room_names: impl IntoIterator<Item = impl Into<String>>,
    ) -> WsIoServerNamespaceBroadcastOperator {
        WsIoServerNamespaceBroadcastOperator::new(self.clone()).except(room_names)
    }

    #[inline]
    pub fn path(&self) -> &str {
        &self.config.path
    }

    #[inline]
    pub fn server(&self) -> WsIoServer {
        WsIoServer(self.runtime.clone())
    }

    pub async fn shutdown(self: &Arc<Self>) {
        match self.status.get() {
            NamespaceStatus::Stopped => return,
            NamespaceStatus::Running => self.status.store(NamespaceStatus::Stopping),
            _ => unreachable!(),
        }

        self.close_all().await;
        let mut connection_task_set = self.connection_task_set.lock().await;
        while connection_task_set.join_next().await.is_some() {}

        self.status.store(NamespaceStatus::Stopped);
    }

    #[inline]
    pub fn to(
        self: &Arc<Self>,
        room_names: impl IntoIterator<Item = impl Into<String>>,
    ) -> WsIoServerNamespaceBroadcastOperator {
        WsIoServerNamespaceBroadcastOperator::new(self.clone()).to(room_names)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

    use super::*;
    use crate::{
        config::WsIoServerConfig,
        core::packet::codecs::WsIoPacketCodec,
    };

    fn create_test_namespace() -> Arc<WsIoServerNamespace> {
        let runtime = WsIoServerRuntime::new(WsIoServerConfig {
            broadcast_concurrency_limit: 16,
            http_request_upgrade_timeout: Duration::from_secs(3),
            init_request_handler_timeout: Duration::from_secs(3),
            init_response_handler_timeout: Duration::from_secs(3),
            init_response_timeout: Duration::from_secs(3),
            middleware_execution_timeout: Duration::from_secs(3),
            on_close_handler_timeout: Duration::from_secs(3),
            on_connect_handler_timeout: Duration::from_secs(3),
            packet_codec: WsIoPacketCodec::SerdeJson,
            request_path: "/socket".into(),
            websocket_config: WebSocketConfig::default(),
        });
        runtime.new_namespace_builder("/test").register().unwrap()
    }

    #[tokio::test]
    async fn test_namespace_new() {
        let namespace = create_test_namespace();
        assert_eq!(namespace.path(), "/test");
        assert_eq!(namespace.connection_count(), 0);
    }

    #[tokio::test]
    async fn test_namespace_connection_count() {
        let namespace = create_test_namespace();
        assert_eq!(namespace.connection_count(), 0);
    }

    #[tokio::test]
    async fn test_namespace_server() {
        let namespace = create_test_namespace();
        namespace.server();
    }

    #[tokio::test]
    async fn test_namespace_to_broadcast_operator() {
        let namespace = create_test_namespace();
        namespace.to(["room1", "room2"]);
    }

    #[tokio::test]
    async fn test_namespace_except_broadcast_operator() {
        let namespace = create_test_namespace();
        namespace.except(["room1", "room2"]);
    }

    #[tokio::test]
    async fn test_namespace_add_remove_connection_id_to_room() {
        let namespace = create_test_namespace();
        namespace.add_connection_id_to_room("room1", 1);
        namespace.add_connection_id_to_room("room1", 2);
        namespace.add_connection_id_to_room("room2", 3);

        // Remove should work
        namespace.remove_connection_id_from_room("room1", 1);
        namespace.remove_connection_id_from_room("room1", 2);
        namespace.remove_connection_id_from_room("room2", 3);
    }

    #[tokio::test]
    async fn test_namespace_remove_connection_id_from_empty_room() {
        let namespace = create_test_namespace();
        // Removing from non-existent room should not panic
        namespace.remove_connection_id_from_room("nonexistent", 1);
    }

    #[tokio::test]
    async fn test_namespace_encode_packet_to_message() {
        let namespace = create_test_namespace();
        let packet = WsIoPacket::new_disconnect();
        namespace.encode_packet_to_message(&packet).unwrap();
    }

    #[tokio::test]
    async fn test_namespace_shutdown_idempotent() {
        let namespace = create_test_namespace();
        namespace.clone().shutdown().await;
        // Shutting down again should be safe
        namespace.shutdown().await;
    }

    #[tokio::test]
    async fn test_broadcast_operator_new() {
        let namespace = create_test_namespace();
        // Just verify we can create an operator
        namespace.to(["room1", "room2"]);
    }

    #[tokio::test]
    async fn test_broadcast_operator_to_chaining() {
        let namespace = create_test_namespace();
        // Chaining should work - just verify it doesn't panic
        namespace.to(["room1"]).to(["room2"]);
    }

    #[tokio::test]
    async fn test_broadcast_operator_except_chaining() {
        let namespace = create_test_namespace();
        // Chaining should work - just verify it doesn't panic
        namespace.except(["room1"]).except(["room2"]);
    }

    #[tokio::test]
    async fn test_broadcast_operator_except_connection_ids() {
        let namespace = create_test_namespace();
        // except_connection_ids is on the broadcast operator, not namespace
        namespace
            .clone()
            .except([1.to_string()])
            .except_connection_ids([1, 2, 3]);
    }

    #[tokio::test]
    async fn test_broadcast_operator_to_with_empty_rooms() {
        let namespace = create_test_namespace();
        // Empty rooms - should still work (broadcast to all)
        namespace.to(Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_broadcast_operator_combined() {
        let namespace = create_test_namespace();
        // Combined chaining should work without panicking
        namespace
            .to(["room1", "room2"])
            .except(["room3"])
            .except_connection_ids([100]);
    }

    #[tokio::test]
    async fn test_broadcast_operator_disconnect_with_no_connections() {
        let namespace = create_test_namespace();
        // disconnect with no connections should return Ok
        let op = namespace.to(["room1"]);
        let result = op.clone().disconnect().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_broadcast_operator_emit_requires_running() {
        let namespace = create_test_namespace();
        // Shutdown to make status invalid
        namespace.clone().shutdown().await;

        let op = namespace.to(["room1"]);
        let result = op.emit("event", Option::<&()>::None).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("invalid status"));
    }

    #[tokio::test]
    async fn test_broadcast_operator_close_is_noop_when_empty() {
        let namespace = create_test_namespace();
        // close with no connections should not panic
        let op = namespace.to(["room1"]);
        op.clone().close().await;
    }
}
