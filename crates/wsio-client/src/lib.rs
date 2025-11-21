use std::{
    fmt::Display,
    sync::Arc,
};

use anyhow::{
    Result,
    bail,
};
use serde::{
    Serialize,
    de::DeserializeOwned,
};
use tokio_util::sync::CancellationToken;
use url::Url;
pub use wsio_core as core;

mod builder;
mod config;
mod runtime;
pub mod session;

use crate::{
    builder::WsIoClientBuilder,
    core::traits::task::spawner::TaskSpawner,
    runtime::WsIoClientRuntime,
    session::WsIoClientSession,
};

// Structs
#[derive(Clone)]
pub struct WsIoClient(Arc<WsIoClientRuntime>);

impl WsIoClient {
    // Public methods
    pub fn builder<U>(url: U) -> Result<WsIoClientBuilder>
    where
        U: TryInto<Url>,
        U::Error: Display,
    {
        let url = match url.try_into() {
            Ok(url) => url,
            Err(err) => bail!("Invalid URL: {err}"),
        };

        if url.scheme() == "wss"
            && !cfg!(any(
                feature = "tls-native",
                feature = "tls-rustls-native",
                feature = "tls-rustls-webpki",
            ))
        {
            bail!("TLS is required for wss:// URLs, but no TLS feature is enabled");
        }

        WsIoClientBuilder::new(url)
    }

    pub fn cancel_token(&self) -> Arc<CancellationToken> {
        self.0.cancel_token()
    }

    pub async fn connect(&self) {
        self.0.connect().await
    }

    pub async fn disconnect(&self) {
        self.0.disconnect().await
    }

    pub async fn emit<D: Serialize>(&self, event: impl AsRef<str>, data: Option<&D>) -> Result<()> {
        self.0.emit(event.as_ref(), data).await
    }

    #[inline]
    pub fn is_session_ready(&self) -> bool {
        self.0.is_session_ready()
    }

    #[inline]
    pub fn off(&self, event: impl AsRef<str>) {
        self.0.off(event.as_ref());
    }

    #[inline]
    pub fn off_by_handler_id(&self, event: impl AsRef<str>, handler_id: u32) {
        self.0.off_by_handler_id(event.as_ref(), handler_id);
    }

    #[inline]
    pub fn on<H, Fut, D>(&self, event: impl AsRef<str>, handler: H) -> u32
    where
        H: Fn(Arc<WsIoClientSession>, Arc<D>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
        D: DeserializeOwned + Send + Sync + 'static,
    {
        self.0.on(event.as_ref(), handler)
    }

    #[inline]
    pub fn spawn_task<F: Future<Output = Result<()>> + Send + 'static>(&self, future: F) {
        self.0.spawn_task(future);
    }
}
