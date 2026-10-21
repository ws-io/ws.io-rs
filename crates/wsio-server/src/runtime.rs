use std::{
    collections::hash_map::Entry,
    sync::{
        Arc,
        atomic::{
            AtomicUsize,
            Ordering,
        },
    },
};

use anyhow::{
    Result,
    bail,
};
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
use serde::Serialize;

use crate::{
    config::WsIoServerConfig,
    core::utils::lifecycle::WsIoLifecycleCompletionSlot,
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
#[derive(Debug)]
pub(crate) struct WsIoServerRuntime {
    pub(crate) config: WsIoServerConfig,
    connection_count: AtomicUsize,
    namespaces: RwLock<FxHashMap<String, Arc<WsIoServerNamespace>>>,
    shutdown_completion: Arc<WsIoLifecycleCompletionSlot>,
    pub(crate) status: AtomicEnumCell<WsIoServerRuntimeStatus>,
}

impl WsIoServerRuntime {
    pub(crate) fn new(config: WsIoServerConfig) -> Arc<Self> {
        Arc::new(Self {
            config,
            connection_count: AtomicUsize::new(0),
            namespaces: RwLock::new(FxHashMap::default()),
            shutdown_completion: Arc::new(WsIoLifecycleCompletionSlot::default()),
            status: AtomicEnumCell::new(WsIoServerRuntimeStatus::Running),
        })
    }

    // Private methods
    #[inline]
    fn clone_namespaces(&self) -> Vec<Arc<WsIoServerNamespace>> {
        self.namespaces.read().values().cloned().collect()
    }

    async fn shutdown_inner(&self) {
        match self.status.get() {
            WsIoServerRuntimeStatus::Stopped => {
                #[cfg(feature = "tracing")]
                tracing::trace!("server shutdown ignored because runtime is already stopped");
                return;
            },
            WsIoServerRuntimeStatus::Running => {
                #[cfg(feature = "tracing")]
                tracing::info!(namespace_count = self.namespace_count(), "shutting down server runtime");
                self.status.store(WsIoServerRuntimeStatus::Stopping);
            },
            WsIoServerRuntimeStatus::Stopping => unreachable!(),
        }

        join_all(self.clone_namespaces().iter().map(WsIoServerNamespace::shutdown)).await;
        self.status.store(WsIoServerRuntimeStatus::Stopped);

        #[cfg(feature = "tracing")]
        tracing::info!("server runtime stopped");
    }

    // Protected methods
    #[inline]
    pub(crate) fn connection_count(&self) -> usize {
        self.connection_count.load(Ordering::Relaxed)
    }

    pub(crate) async fn close_all(&self) {
        #[cfg(feature = "tracing")]
        tracing::debug!(
            namespace_count = self.namespace_count(),
            "closing all server connections"
        );

        join_all(self.clone_namespaces().iter().map(WsIoServerNamespace::close_all)).await;
    }

    #[inline]
    pub(crate) fn decrement_connection_count(&self) {
        self.connection_count.fetch_sub(1, Ordering::Relaxed);
    }

    pub(crate) async fn emit<D: Serialize>(&self, event: &str, data: Option<&D>) -> Result<()> {
        self.status.ensure(WsIoServerRuntimeStatus::Running, |status| {
            format!("Cannot emit in invalid status: {status:?}")
        })?;

        #[cfg(feature = "tracing")]
        tracing::trace!(
            event,
            namespace_count = self.namespace_count(),
            "broadcasting server event"
        );

        for result in join_all(
            self.clone_namespaces()
                .iter()
                .map(|namespace| namespace.emit(event, data)),
        )
        .await
        {
            result?;
        }

        Ok(())
    }

    pub(crate) async fn disconnect_all(&self) {
        #[cfg(feature = "tracing")]
        tracing::debug!(
            namespace_count = self.namespace_count(),
            "disconnecting all server connections"
        );

        join_all(self.clone_namespaces().iter().map(WsIoServerNamespace::disconnect_all)).await;
    }

    #[inline]
    pub(crate) fn get_namespace(&self, path: &str) -> Option<Arc<WsIoServerNamespace>> {
        self.namespaces.read().get(path).cloned()
    }

    #[inline]
    pub(crate) fn increment_connection_count(&self) {
        self.connection_count.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub(crate) fn insert_namespace(&self, namespace: Arc<WsIoServerNamespace>) -> Result<()> {
        match self.namespaces.write().entry(namespace.path().to_owned()) {
            Entry::Occupied(entry) => {
                #[cfg(feature = "tracing")]
                tracing::debug!(namespace = entry.key(), "refusing duplicate namespace registration");
                bail!("Namespace {} already exists", entry.key());
            },
            Entry::Vacant(entry) => {
                #[cfg(feature = "tracing")]
                tracing::info!(namespace = entry.key(), "registered namespace");
                entry.insert(namespace);
            },
        }

        Ok(())
    }

    #[inline]
    pub(crate) fn namespace_count(&self) -> usize {
        self.namespaces.read().len()
    }

    #[inline]
    pub(crate) fn new_namespace_builder(self: &Arc<Self>, path: &str) -> WsIoServerNamespaceBuilder {
        WsIoServerNamespaceBuilder::new(path, Arc::clone(self))
    }

    pub(crate) async fn remove_namespace(&self, path: &str) {
        let Some(namespace) = self.namespaces.write().remove(path) else {
            #[cfg(feature = "tracing")]
            tracing::trace!(
                namespace = path,
                "remove namespace ignored because namespace is not registered"
            );

            return;
        };

        #[cfg(feature = "tracing")]
        tracing::info!(namespace = path, "removing namespace");
        namespace.shutdown().await;
    }

    pub(crate) async fn shutdown(self: &Arc<Self>) {
        let runtime = Arc::clone(self);
        self.shutdown_completion
            .wait_or_spawn(async move || runtime.shutdown_inner().await)
            .await;
    }
}
