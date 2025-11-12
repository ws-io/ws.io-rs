use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;
pub use wsio_core as core;

mod builder;
mod config;
pub mod connection;
pub mod namespace;
mod request;
pub mod request_adapters;
mod runtime;

#[cfg(feature = "tower")]
use crate::request_adapters::tower::layer::WsIoServerLayer;
use crate::{
    builder::WsIoServerBuilder,
    namespace::{
        WsIoServerNamespace,
        builder::WsIoServerNamespaceBuilder,
    },
    runtime::WsIoServerRuntime,
};

// Structs
#[derive(Clone)]
pub struct WsIoServer(Arc<WsIoServerRuntime>);

impl WsIoServer {
    // Public methods
    pub fn builder() -> WsIoServerBuilder {
        WsIoServerBuilder::new()
    }

    #[inline]
    pub fn connection_count(&self) -> usize {
        self.0.connection_count()
    }

    pub async fn emit<D: Serialize>(&self, event: impl AsRef<str>, data: Option<&D>) -> Result<()> {
        self.0.emit(event.as_ref(), data).await
    }

    #[cfg(feature = "tower")]
    pub fn layer(&self) -> WsIoServerLayer {
        WsIoServerLayer::new(self.0.clone())
    }

    #[inline]
    pub fn of(&self, path: impl AsRef<str>) -> Option<Arc<WsIoServerNamespace>> {
        self.0.get_namespace(path.as_ref())
    }

    #[inline]
    pub fn namespace_count(&self) -> usize {
        self.0.namespace_count()
    }

    #[inline]
    pub fn new_namespace_builder(&self, path: impl AsRef<str>) -> Result<WsIoServerNamespaceBuilder> {
        self.0.new_namespace_builder(path.as_ref())
    }

    pub async fn remove_namespace(&self, path: impl AsRef<str>) {
        self.0.remove_namespace(path.as_ref()).await
    }

    pub async fn shutdown(&self) {
        self.0.shutdown().await
    }
}
