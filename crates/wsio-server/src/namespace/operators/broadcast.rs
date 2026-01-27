use std::{
    collections::HashSet,
    sync::Arc,
};

use anyhow::Result;
use futures_util::{
    StreamExt,
    future::ready,
    stream::iter,
};
use roaring::RoaringTreemap;
use serde::Serialize;

use super::super::{
    NamespaceStatus,
    WsIoServerNamespace,
};
use crate::{
    connection::WsIoServerConnection,
    core::packet::WsIoPacket,
};

// Structs
#[derive(Clone)]
pub struct WsIoServerNamespaceBroadcastOperator {
    exclude_connection_ids: HashSet<u64>,
    exclude_rooms: HashSet<String>,
    include_rooms: HashSet<String>,
    namespace: Arc<WsIoServerNamespace>,
}

impl WsIoServerNamespaceBroadcastOperator {
    #[inline]
    pub(in super::super) fn new(namespace: Arc<WsIoServerNamespace>) -> Self {
        Self {
            exclude_connection_ids: HashSet::new(),
            exclude_rooms: HashSet::new(),
            include_rooms: HashSet::new(),
            namespace,
        }
    }

    // Private methods
    async fn for_each_target_connections<F, Fut>(&self, f: F)
    where
        F: Fn(Arc<WsIoServerConnection>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        let mut target_connection_ids = if self.include_rooms.is_empty() {
            (**self.namespace.connection_ids.load()).clone()
        } else {
            let mut connection_ids = RoaringTreemap::new();
            for room_name in &self.include_rooms {
                if let Some(room) = self.namespace.rooms.get(room_name) {
                    connection_ids |= room.value();
                }
            }

            connection_ids
        };

        for room_name in &self.exclude_rooms {
            if let Some(room) = self.namespace.rooms.get(room_name) {
                target_connection_ids -= room.value();
                if target_connection_ids.is_empty() {
                    break;
                }
            }
        }

        for exclude_connection_id in &self.exclude_connection_ids {
            target_connection_ids.remove(*exclude_connection_id);
        }

        if target_connection_ids.is_empty() {
            return;
        }

        iter(target_connection_ids)
            .filter_map(|target_connection_id| {
                ready(
                    self.namespace
                        .connections
                        .get(&target_connection_id)
                        .map(|entry| entry.value().clone()),
                )
            })
            .for_each_concurrent(self.namespace.config.broadcast_concurrency_limit, |connection| async {
                let _ = f(connection).await;
            })
            .await;
    }

    // Public methods
    pub async fn close(self) {
        self.for_each_target_connections(|connection| async move {
            connection.close();
            Ok(())
        })
        .await;
    }

    pub async fn disconnect(self) -> Result<()> {
        let message = self.namespace.encode_packet_to_message(&WsIoPacket::new_disconnect())?;
        self.for_each_target_connections(move |connection| {
            let message = message.clone();
            async move { connection.send_message(message).await }
        })
        .await;

        Ok(())
    }

    pub async fn emit<D: Serialize>(self, event: impl AsRef<str>, data: Option<&D>) -> Result<()> {
        self.namespace.status.ensure(NamespaceStatus::Running, |status| {
            format!("Cannot emit in invalid status: {status:?}")
        })?;

        let message = self.namespace.encode_packet_to_message(&WsIoPacket::new_event(
            event.as_ref(),
            data.map(|data| self.namespace.config.packet_codec.encode_data(data))
                .transpose()?,
        ))?;

        self.for_each_target_connections(move |connection| {
            let message = message.clone();
            async move { connection.emit_event_message(message).await }
        })
        .await;

        Ok(())
    }

    #[inline]
    pub fn except(mut self, room_names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.exclude_rooms.extend(room_names.into_iter().map(Into::into));
        self
    }

    pub fn except_connection_ids(mut self, connection_ids: impl IntoIterator<Item = u64>) -> Self {
        self.exclude_connection_ids.extend(connection_ids);
        self
    }

    #[inline]
    pub fn to(mut self, room_names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.include_rooms.extend(room_names.into_iter().map(Into::into));
        self
    }
}
