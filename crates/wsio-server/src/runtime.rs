use std::sync::Arc;

use anyhow::{
    Result,
    bail,
};
use arc_swap::ArcSwap;
use futures_util::future::join_all;
use kikiutils::{
    atomic::enum_cell::AtomicEnumCell,
    types::fx_collections::FxHashMap,
};
use num_enum::{
    IntoPrimitive,
    TryFromPrimitive,
};
use parking_lot::RwLock;
use roaring::RoaringTreemap;
use serde::Serialize;

use crate::{
    config::WsIoServerConfig,
    namespace::{
        WsIoServerNamespace,
        builder::WsIoServerNamespaceBuilder,
    },
};

// Enums
#[repr(u8)]
#[derive(Debug, Eq, IntoPrimitive, PartialEq, TryFromPrimitive)]
pub(crate) enum WsIoServerRuntimeStatus {
    Running,
    Stopped,
    Stopping,
}

// Structs
pub(crate) struct WsIoServerRuntime {
    pub(crate) config: WsIoServerConfig,
    connection_ids: ArcSwap<RoaringTreemap>,
    namespaces: RwLock<FxHashMap<String, Arc<WsIoServerNamespace>>>,
    pub(crate) status: AtomicEnumCell<WsIoServerRuntimeStatus>,
}

impl WsIoServerRuntime {
    pub(crate) fn new(config: WsIoServerConfig) -> Arc<Self> {
        Arc::new(Self {
            config,
            connection_ids: ArcSwap::new(Arc::new(RoaringTreemap::new())),
            namespaces: RwLock::new(FxHashMap::default()),
            status: AtomicEnumCell::new(WsIoServerRuntimeStatus::Running),
        })
    }

    // Private methods
    #[inline]
    fn clone_namespaces(&self) -> Vec<Arc<WsIoServerNamespace>> {
        self.namespaces.read().values().cloned().collect()
    }

    // Protected methods
    #[inline]
    pub(crate) fn connection_count(&self) -> usize {
        self.connection_ids.load().len() as usize
    }

    pub(crate) async fn close_all(&self) {
        join_all(self.clone_namespaces().iter().map(|namespace| namespace.close_all())).await;
    }

    pub(crate) async fn emit<D: Serialize>(&self, event: &str, data: Option<&D>) -> Result<()> {
        self.status.ensure(WsIoServerRuntimeStatus::Running, |status| {
            format!("Cannot emit in invalid status: {status:?}",)
        })?;

        join_all(
            self.clone_namespaces()
                .iter()
                .map(|namespace| namespace.emit(event, data)),
        )
        .await;

        Ok(())
    }

    pub(crate) async fn disconnect_all(&self) {
        join_all(
            self.clone_namespaces()
                .iter()
                .map(|namespace| namespace.disconnect_all()),
        )
        .await;
    }

    #[inline]
    pub(crate) fn get_namespace(&self, path: &str) -> Option<Arc<WsIoServerNamespace>> {
        self.namespaces.read().get(path).cloned()
    }

    #[inline]
    pub(crate) fn insert_connection_id(&self, id: u64) {
        self.connection_ids.rcu(|old_connection_ids| {
            let mut new_connection_ids = (**old_connection_ids).clone();
            new_connection_ids.insert(id);
            new_connection_ids
        });
    }

    #[inline]
    pub(crate) fn insert_namespace(&self, namespace: Arc<WsIoServerNamespace>) -> Result<()> {
        if self.namespaces.read().contains_key(namespace.path()) {
            bail!("Namespace {} already exists", namespace.path());
        }

        self.namespaces.write().insert(namespace.path().into(), namespace);
        Ok(())
    }

    #[inline]
    pub(crate) fn namespace_count(&self) -> usize {
        self.namespaces.read().len()
    }

    #[inline]
    pub(crate) fn new_namespace_builder(self: &Arc<Self>, path: &str) -> WsIoServerNamespaceBuilder {
        WsIoServerNamespaceBuilder::new(path, self.clone())
    }

    #[inline]
    pub(crate) fn remove_connection_id(&self, id: u64) {
        self.connection_ids.rcu(|old_connection_ids| {
            let mut new_connection_ids = (**old_connection_ids).clone();
            new_connection_ids.remove(id);
            new_connection_ids
        });
    }

    pub(crate) async fn remove_namespace(&self, path: &str) {
        let Some(namespace) = self.namespaces.write().remove(path) else {
            return;
        };

        namespace.shutdown().await;
    }

    pub(crate) async fn shutdown(&self) {
        match self.status.get() {
            WsIoServerRuntimeStatus::Stopped => return,
            WsIoServerRuntimeStatus::Running => self.status.store(WsIoServerRuntimeStatus::Stopping),
            _ => unreachable!(),
        }

        join_all(self.clone_namespaces().iter().map(|namespace| namespace.shutdown())).await;
        self.status.store(WsIoServerRuntimeStatus::Stopped);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

    use super::*;
    use crate::core::packet::codecs::WsIoPacketCodec;

    fn create_test_config() -> WsIoServerConfig {
        WsIoServerConfig {
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
        }
    }

    #[tokio::test]
    async fn test_runtime_new() {
        let runtime = WsIoServerRuntime::new(create_test_config());
        assert_eq!(runtime.namespace_count(), 0);
        assert_eq!(runtime.connection_count(), 0);
    }

    #[tokio::test]
    async fn test_runtime_get_namespace_not_found() {
        let runtime = WsIoServerRuntime::new(create_test_config());
        assert!(runtime.get_namespace("/nonexistent").is_none());
    }

    #[tokio::test]
    async fn test_runtime_insert_namespace_and_get() {
        let runtime = WsIoServerRuntime::new(create_test_config());
        runtime.new_namespace_builder("/test").register().unwrap();
        assert_eq!(runtime.namespace_count(), 1);
        assert_eq!(runtime.get_namespace("/test").unwrap().path(), "/test");
    }

    #[tokio::test]
    async fn test_runtime_insert_duplicate_namespace_fails() {
        let runtime = WsIoServerRuntime::new(create_test_config());
        let namespace = runtime.new_namespace_builder("/test").register().unwrap();
        let result = runtime.insert_namespace(namespace);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn test_runtime_connection_id_tracking() {
        let runtime = WsIoServerRuntime::new(create_test_config());
        runtime.insert_connection_id(1);
        runtime.insert_connection_id(2);
        assert_eq!(runtime.connection_count(), 2);

        runtime.remove_connection_id(1);
        assert_eq!(runtime.connection_count(), 1);
    }

    #[tokio::test]
    async fn test_runtime_remove_namespace_not_found() {
        let runtime = WsIoServerRuntime::new(create_test_config());
        // Removing non-existent namespace should not panic
        runtime.remove_namespace("/nonexistent").await;
        assert_eq!(runtime.namespace_count(), 0);
    }

    #[tokio::test]
    async fn test_runtime_new_namespace_builder() {
        let runtime = WsIoServerRuntime::new(create_test_config());
        let builder = runtime.new_namespace_builder("/test");
        let namespace = builder.register().unwrap();
        assert_eq!(namespace.path(), "/test");
        assert_eq!(runtime.namespace_count(), 1);
    }

    #[tokio::test]
    async fn test_runtime_emit_invalid_status() {
        let runtime = WsIoServerRuntime::new(create_test_config());
        // Insert namespace first to make emit work (namespace-level emit checks runtime status)
        let namespace = runtime.new_namespace_builder("/test").register().unwrap();

        // Shutdown runtime to make it invalid for emit
        runtime.shutdown().await;

        let result = namespace.emit("test_event", Option::<&()>::None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid status"));
    }
}
