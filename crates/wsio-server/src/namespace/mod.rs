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
#[derive(Debug)]
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
        #[cfg(feature = "tracing")]
        tracing::debug!(
            namespace = self.config.path,
            request_path = request_uri.path(),
            "handling upgraded WebSocket request"
        );

        // Create ws stream
        let mut ws_stream =
            WebSocketStream::from_raw_socket(TokioIo::new(upgraded), Role::Server, Some(self.config.websocket_config))
                .await;

        // Check runtime and namespace status
        if !self.runtime.status.is(WsIoServerRuntimeStatus::Running) || !self.status.is(NamespaceStatus::Running) {
            #[cfg(feature = "tracing")]
            tracing::debug!(
                namespace = self.config.path,
                runtime_status = ?self.runtime.status.get(),
                namespace_status = ?self.status.get(),
                "rejecting upgraded request because server or namespace is not running"
            );

            ws_stream
                .send((*self.encode_packet_to_message(&WsIoPacket::new_disconnect())?).clone())
                .await?;

            let _ = ws_stream.close(None).await;
            return Ok(());
        }

        // Create connection
        let (connection, mut message_rx, event_queue_rx) =
            WsIoServerConnection::new(headers, self.clone(), request_uri);

        connection.start_event_dispatcher(event_queue_rx).await;

        #[cfg(feature = "tracing")]
        tracing::debug!(
            namespace = self.config.path,
            connection_id = connection.id(),
            "accepted WebSocket connection"
        );

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
                    },
                    Ok(Message::Close(_)) => {
                        #[cfg(feature = "tracing")]
                        tracing::debug!(connection_id = connection_clone.id(), "server read task received close frame");
                        break;
                    },
                    Err(_err) => {
                        #[cfg(feature = "tracing")]
                        tracing::debug!(connection_id = connection_clone.id(), error = %_err, "server read task failed");
                        break;
                    },
                    Ok(Message::Text(text)) => connection_clone.handle_incoming_packet(text.as_bytes()).await,
                    _ => Ok(()),
                }
                .is_err()
                {
                    #[cfg(feature = "tracing")]
                    tracing::debug!(
                        connection_id = connection_clone.id(),
                        "server read task stopped after packet handling error"
                    );

                    break;
                }
            }
        });

        let mut write_ws_stream_task = spawn(async move {
            while let Some(message) = message_rx.recv().await {
                let message = (*message).clone();
                let is_close = matches!(message, Message::Close(_));
                if ws_stream_writer.send(message).await.is_err() {
                    #[cfg(feature = "tracing")]
                    tracing::debug!("server write task failed to send message");
                    break;
                }

                if is_close {
                    #[cfg(feature = "tracing")]
                    tracing::debug!("server write task sent close frame");
                    let _ = ws_stream_writer.close().await;
                    break;
                }
            }
        });

        // Try to init connection
        match connection.init().await {
            Ok(()) => {
                #[cfg(feature = "tracing")]
                tracing::debug!(connection_id = connection.id(), "server connection initialized");
                // Wait for either read or write task to finish
                select! {
                    _ = &mut read_ws_stream_task => {
                        #[cfg(feature = "tracing")]
                        tracing::debug!(connection_id = connection.id(), "server read task finished; aborting write task");
                        write_ws_stream_task.abort();
                    },
                    _ = &mut write_ws_stream_task => {
                        #[cfg(feature = "tracing")]
                        tracing::debug!(connection_id = connection.id(), "server write task finished; aborting read task");
                        read_ws_stream_task.abort();
                    },
                }
            },
            Err(_err) => {
                #[cfg(feature = "tracing")]
                tracing::debug!(connection_id = connection.id(), error = %_err, "server connection initialization failed");
                // Close connection
                read_ws_stream_task.abort();
                connection.close();
                let _ = join!(read_ws_stream_task, write_ws_stream_task);
            },
        }

        // Cleanup connection
        connection.cleanup().await;

        #[cfg(feature = "tracing")]
        tracing::debug!(connection_id = connection.id(), "server connection stopped");
        Ok(())
    }

    // Protected methods
    #[inline]
    pub(crate) fn add_connection_id_to_room(&self, room_name: &str, connection_id: u64) {
        self.rooms
            .entry(room_name.to_owned())
            .or_default()
            .insert(connection_id);
    }

    #[inline]
    pub(crate) fn encode_packet_to_message(&self, packet: &WsIoPacket) -> Result<Arc<Message>> {
        let bytes = self.config.packet_codec.encode(packet)?;
        Ok(Arc::new(if self.config.packet_codec.is_text() {
            // SAFETY: text packet codecs only produce valid UTF-8 payloads.
            Message::Text(unsafe { String::from_utf8_unchecked(bytes) }.into())
        } else {
            Message::Binary(bytes.into())
        }))
    }

    pub(crate) async fn handle_on_upgrade_request(
        self: &Arc<Self>,
        headers: HeaderMap,
        on_upgrade: OnUpgrade,
        request_uri: Uri,
    ) {
        #[cfg(feature = "tracing")]
        tracing::trace!(
            namespace = self.config.path,
            request_path = request_uri.path(),
            "spawning WebSocket upgrade task"
        );

        let namespace = self.clone();
        self.connection_task_set.lock().await.spawn(async move {
            match timeout(namespace.config.http_request_upgrade_timeout, on_upgrade).await {
                Ok(Ok(upgraded)) => {
                    if let Err(_err) = namespace.handle_upgraded_request(headers, request_uri, upgraded).await {
                        #[cfg(feature = "tracing")]
                        tracing::debug!(namespace = namespace.config.path, error = %_err, "upgraded request handling failed");
                    }
                },
                Ok(Err(_err)) => {
                    #[cfg(feature = "tracing")]
                    tracing::warn!(namespace = namespace.config.path, error = %_err, "HTTP upgrade failed");
                },
                Err(_err) => {
                    #[cfg(feature = "tracing")]
                    tracing::warn!(
                        namespace = namespace.config.path,
                        error = %_err,
                        timeout_ms = u64::try_from(namespace.config.http_request_upgrade_timeout.as_millis())
                            .unwrap_or(u64::MAX),
                        "HTTP upgrade timed out"
                    );
                },
            }
        });
    }

    #[inline]
    pub(crate) fn insert_connection(&self, connection: &Arc<WsIoServerConnection>) {
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
            NamespaceStatus::Running => {
                #[cfg(feature = "tracing")]
                tracing::info!(namespace = self.config.path, "shutting down namespace");
                self.status.store(NamespaceStatus::Stopping);
            },
            NamespaceStatus::Stopping => unreachable!(),
        }

        self.close_all().await;
        let mut connection_task_set = self.connection_task_set.lock().await;
        while connection_task_set.join_next().await.is_some() {}

        self.status.store(NamespaceStatus::Stopped);

        #[cfg(feature = "tracing")]
        tracing::info!(namespace = self.config.path, "namespace stopped");
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
    use super::*;
    use crate::WsIoServer;

    fn create_test_namespace() -> Arc<WsIoServerNamespace> {
        WsIoServer::builder()
            .build()
            .new_namespace_builder("/test")
            .register()
            .unwrap()
    }

    #[test]
    fn test_namespace_new() {
        let namespace = create_test_namespace();
        assert_eq!(namespace.path(), "/test");
        assert_eq!(namespace.connection_count(), 0);
    }

    #[test]
    fn test_namespace_add_remove_connection_id_to_room() {
        let namespace = create_test_namespace();
        namespace.add_connection_id_to_room("room1", 1);
        namespace.add_connection_id_to_room("room1", 2);
        namespace.add_connection_id_to_room("room2", 3);

        assert_eq!(namespace.rooms.get("room1").unwrap().len(), 2);
        assert!(namespace.rooms.get("room1").unwrap().contains(1));
        assert!(namespace.rooms.get("room1").unwrap().contains(2));
        assert_eq!(namespace.rooms.get("room2").unwrap().len(), 1);

        namespace.remove_connection_id_from_room("room1", 1);
        assert_eq!(namespace.rooms.get("room1").unwrap().len(), 1);
        assert!(namespace.rooms.get("room1").unwrap().contains(2));

        namespace.remove_connection_id_from_room("room1", 2);
        namespace.remove_connection_id_from_room("room2", 3);

        assert!(!namespace.rooms.contains_key("room1"));
        assert!(!namespace.rooms.contains_key("room2"));
    }

    #[test]
    fn test_namespace_encode_packet_to_message() {
        let namespace = create_test_namespace();
        let packet = WsIoPacket::new_disconnect();
        let message = namespace.encode_packet_to_message(&packet).unwrap();

        assert!(matches!(&*message, Message::Text(_)));
    }

    #[tokio::test]
    async fn test_namespace_shutdown_idempotent() {
        let namespace = create_test_namespace();
        namespace.clone().shutdown().await;
        // Shutting down again should be safe
        namespace.shutdown().await;
    }

    #[tokio::test]
    async fn test_broadcast_operator_disconnect_with_no_connections() {
        let namespace = create_test_namespace();
        namespace.to(["room1"]).disconnect().await.unwrap();
    }

    #[tokio::test]
    async fn test_broadcast_operator_emit_requires_running() {
        let namespace = create_test_namespace();
        // Shutdown to make status invalid
        namespace.clone().shutdown().await;

        let err = namespace
            .to(["room1"])
            .emit("event", Option::<&()>::None)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("invalid status"));
    }
}
