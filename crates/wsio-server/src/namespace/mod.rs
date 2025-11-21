use std::sync::Arc;

use anyhow::Result;
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
    types::fx_collections::{
        FxDashMap,
        FxDashSet,
    },
};
use num_enum::{
    IntoPrimitive,
    TryFromPrimitive,
};
use serde::Serialize;
use tokio::{
    join,
    select,
    spawn,
    sync::Mutex,
    task::JoinSet,
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
    connections: FxDashMap<u64, Arc<WsIoServerConnection>>,
    connection_task_set: Mutex<JoinSet<()>>,
    rooms: FxDashMap<String, Arc<FxDashSet<u64>>>,
    runtime: Arc<WsIoServerRuntime>,
    status: AtomicEnumCell<NamespaceStatus>,
}

impl WsIoServerNamespace {
    fn new(config: WsIoServerNamespaceConfig, runtime: Arc<WsIoServerRuntime>) -> Arc<Self> {
        Arc::new(Self {
            config,
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
        self.rooms
            .entry(room_name.into())
            .or_default()
            .clone()
            .insert(connection_id);
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
            if let Ok(upgraded) = on_upgrade.await {
                let _ = namespace.handle_upgraded_request(headers, request_uri, upgraded).await;
            }
        });
    }

    #[inline]
    pub(crate) fn insert_connection(&self, connection: Arc<WsIoServerConnection>) {
        self.connections.insert(connection.id(), connection.clone());
        self.runtime.insert_connection_id(connection.id());
    }

    #[inline]
    pub(crate) fn remove_connection(&self, id: u64) {
        self.connections.remove(&id);
        self.runtime.remove_connection_id(id);
    }

    #[inline]
    pub(crate) fn remove_connection_id_from_room(&self, room_name: &str, connection_id: u64) {
        if let Some(room) = self.rooms.get(room_name).map(|entry| entry.clone()) {
            room.remove(&connection_id);
            if room.is_empty() {
                self.rooms.remove(room_name);
            }
        }
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
        room_names: impl IntoIterator<Item = impl AsRef<str>>,
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
        room_names: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> WsIoServerNamespaceBroadcastOperator {
        WsIoServerNamespaceBroadcastOperator::new(self.clone()).to(room_names)
    }
}
