use std::sync::Arc;

use anyhow::{
    Result,
    anyhow,
    ensure,
};
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
    select,
    spawn,
    time::timeout,
};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{
        Message,
        protocol::Role,
    },
};
use tokio_util::task::TaskTracker;

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
    core::{
        packet::WsIoPacket,
        traits::task::spawner::TaskSpawner,
        transport::shutdown_websocket_stream_tasks,
        utils::lifecycle::WsIoLifecycleCompletionSlot,
    },
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
    connections: FxDashMap<u64, Arc<WsIoServerConnection>>,
    connection_task_tracker: TaskTracker,
    rooms: FxDashMap<String, RoaringTreemap>,
    runtime: Arc<WsIoServerRuntime>,
    shutdown_completion: Arc<WsIoLifecycleCompletionSlot>,
    status: AtomicEnumCell<NamespaceStatus>,
}

impl WsIoServerNamespace {
    fn new(config: WsIoServerNamespaceConfig, runtime: Arc<WsIoServerRuntime>) -> Arc<Self> {
        Arc::new(Self {
            config,
            connections: FxDashMap::default(),
            connection_task_tracker: TaskTracker::new(),
            rooms: FxDashMap::default(),
            runtime,
            shutdown_completion: Arc::new(WsIoLifecycleCompletionSlot::default()),
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
                .send((*self.encode_packet_to_message(&WsIoPacket::new_disconnect()).await?).clone())
                .await?;

            let _ = ws_stream.close(None).await;
            return Ok(());
        }

        // Create connection
        let (connection, mut message_rx, event_queue_rx) =
            WsIoServerConnection::new(headers, Arc::clone(self), request_uri);

        connection.start_event_dispatcher(event_queue_rx).await;

        #[cfg(feature = "tracing")]
        tracing::debug!(
            namespace = self.config.path,
            connection_id = connection.id(),
            "accepted WebSocket connection"
        );

        // Split ws stream and spawn read and write tasks
        let (mut ws_stream_writer, mut ws_stream_reader) = ws_stream.split();
        let connection_clone = Arc::clone(&connection);
        let connection_idle_timeout = self.config.connection_idle_timeout;
        let mut read_ws_stream_task = spawn(async move {
            loop {
                let message = if let Some(idle_timeout) = connection_idle_timeout
                    && connection_clone.is_ready()
                {
                    if let Ok(message) = timeout(idle_timeout, ws_stream_reader.next()).await {
                        message
                    } else {
                        #[cfg(feature = "tracing")]
                        tracing::debug!(connection_id = connection_clone.id(), "server connection idle timeout");
                        break;
                    }
                } else {
                    ws_stream_reader.next().await
                };

                let Some(message) = message else {
                    break;
                };

                if match message {
                    Ok(Message::Binary(bytes)) => {
                        // Treat any single-byte binary frame as a client heartbeat and ignore it;
                        // receiving it still refreshes the idle timeout
                        if bytes.len() == 1 {
                            continue;
                        }

                        connection_clone.handle_incoming_packet(bytes).await
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
                    Ok(Message::Text(_)) => Err(anyhow!("text WebSocket frames are not supported")),
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

        // Initialization must also yield to close requests (including stalled transformers).
        let cancel_token = connection.cancel_token();
        let init_result = select! {
            biased;
            () = cancel_token.cancelled() => Err(anyhow!("connection closed during initialization")),
            result = connection.init() => result,
        };

        if let Err(_err) = init_result {
            #[cfg(feature = "tracing")]
            tracing::debug!(connection_id = connection.id(), error = %_err, "server connection initialization failed");
            connection.close();
        } else {
            #[cfg(feature = "tracing")]
            tracing::debug!(connection_id = connection.id(), "server connection initialized");
        }

        select! {
            biased;
            () = cancel_token.cancelled() => {
                shutdown_websocket_stream_tasks(
                    read_ws_stream_task,
                    write_ws_stream_task,
                    self.config.disconnect_timeout,
                ).await;
            },
            _ = &mut read_ws_stream_task => {
                #[cfg(feature = "tracing")]
                tracing::debug!(connection_id = connection.id(), "server read task finished; aborting write task");
                write_ws_stream_task.abort();
                let _ = write_ws_stream_task.await;
            },
            _ = &mut write_ws_stream_task => {
                #[cfg(feature = "tracing")]
                tracing::debug!(connection_id = connection.id(), "server write task finished; aborting read task");
                read_ws_stream_task.abort();
                let _ = read_ws_stream_task.await;
            },
        }

        // Cleanup connection
        connection.cleanup().await;

        #[cfg(feature = "tracing")]
        tracing::debug!(connection_id = connection.id(), "server connection stopped");
        Ok(())
    }

    async fn shutdown_inner(self: &Arc<Self>) {
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
        self.connection_task_tracker.close();
        self.connection_task_tracker.wait().await;

        self.status.store(NamespaceStatus::Stopped);

        #[cfg(feature = "tracing")]
        tracing::info!(namespace = self.config.path, "namespace stopped");
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
    pub(crate) async fn encode_packet_to_message(&self, packet: &WsIoPacket) -> Result<Arc<Message>> {
        let bytes = self.config.packet_codec.encode(packet)?;
        let bytes = self.config.packet_transformer.encode(bytes).await?;
        ensure!(
            bytes.len() <= self.config.max_outgoing_message_size,
            "outgoing message exceeds max_outgoing_message_size ({} > {} bytes)",
            bytes.len(),
            self.config.max_outgoing_message_size
        );

        Ok(Arc::new(Message::Binary(bytes)))
    }

    #[inline]
    pub(crate) fn handle_on_upgrade_request(
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

        let namespace = Arc::clone(self);
        self.connection_task_tracker.spawn(async move {
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
    pub(crate) fn insert_connection(&self, connection: &Arc<WsIoServerConnection>) -> bool {
        if !self.status.is(NamespaceStatus::Running) {
            return false;
        }

        self.connections.entry(connection.id()).or_insert_with(|| {
            // Count the connection before publishing it, while the entry's write lock excludes removal.
            self.runtime.increment_connection_count();
            Arc::clone(connection)
        });

        // Shutdown may have taken its close_all() snapshot while this connection
        // was being registered. Remove it and let initialization close it if so.
        if self.status.is(NamespaceStatus::Running) {
            true
        } else {
            self.remove_connection(connection.id());
            false
        }
    }

    #[inline]
    pub(crate) fn remove_connection(&self, id: u64) {
        if self.connections.remove(&id).is_some() {
            self.runtime.decrement_connection_count();
        }
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
        WsIoServerNamespaceBroadcastOperator::new(Arc::clone(self))
            .close()
            .await;
    }

    #[inline]
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    pub async fn disconnect_all(self: &Arc<Self>) -> Result<()> {
        WsIoServerNamespaceBroadcastOperator::new(Arc::clone(self))
            .disconnect()
            .await
    }

    /// Encodes and queues an event without waiting for peer receipt.
    ///
    /// The transformed packet must fit the configured outgoing message limit.
    /// Success is not a delivery acknowledgement; disconnected broadcast targets may be skipped.
    pub async fn emit<D: Serialize>(self: &Arc<Self>, event: impl AsRef<str>, data: Option<&D>) -> Result<()> {
        WsIoServerNamespaceBroadcastOperator::new(Arc::clone(self))
            .emit(event, data)
            .await
    }

    #[inline]
    pub fn except(
        self: &Arc<Self>,
        room_names: impl IntoIterator<Item = impl Into<String>>,
    ) -> WsIoServerNamespaceBroadcastOperator {
        WsIoServerNamespaceBroadcastOperator::new(Arc::clone(self)).except(room_names)
    }

    #[inline]
    pub fn path(&self) -> &str {
        &self.config.path
    }

    /// Returns the number of connections in a room in this namespace, or zero if it does not exist.
    #[inline]
    pub fn room_connection_count(&self, room_name: impl AsRef<str>) -> u64 {
        self.rooms.get(room_name.as_ref()).map_or(0, |room| room.len())
    }

    #[inline]
    pub fn server(&self) -> WsIoServer {
        WsIoServer(Arc::clone(&self.runtime))
    }

    pub async fn shutdown(self: &Arc<Self>) {
        let namespace = Arc::clone(self);
        self.shutdown_completion
            .wait_or_spawn(async move || namespace.shutdown_inner().await)
            .await;
    }

    #[inline]
    pub fn to(
        self: &Arc<Self>,
        room_names: impl IntoIterator<Item = impl Into<String>>,
    ) -> WsIoServerNamespaceBroadcastOperator {
        WsIoServerNamespaceBroadcastOperator::new(Arc::clone(self)).to(room_names)
    }
}

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
