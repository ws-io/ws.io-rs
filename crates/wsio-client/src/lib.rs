#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

use std::sync::Arc;

use anyhow::{
    Result,
    anyhow,
    bail,
};
use serde::{
    Serialize,
    de::DeserializeOwned,
};
use tokio_util::sync::CancellationToken;
use url::Url;
pub use wsio_core as core;

pub mod builder;
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
#[derive(Clone, Debug)]
pub struct WsIoClient(Arc<WsIoClientRuntime>);

impl WsIoClient {
    // Public methods
    pub fn builder(url: impl AsRef<str>) -> Result<WsIoClientBuilder> {
        let url = Url::parse(url.as_ref()).map_err(|err| anyhow!("Invalid URL: {err}"))?;

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

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;

    fn assert_builder_accepts(url: impl AsRef<str>) {
        assert!(WsIoClient::builder(url).is_ok());
    }

    #[test]
    #[allow(clippy::needless_borrows_for_generic_args)] // Explicitly verify borrowed input forms.
    fn builder_accepts_common_url_input_types() {
        const URL: &str = "ws://localhost:8080/chat";

        let str_ref = URL;
        assert_builder_accepts(str_ref);
        assert_builder_accepts(&str_ref);

        let string = URL.to_owned();
        assert_builder_accepts(string.clone());
        assert_builder_accepts(&string);

        let url = Url::parse(URL).unwrap();
        assert_builder_accepts(url.clone());
        assert_builder_accepts(&url);

        assert_builder_accepts(Cow::Borrowed(URL));
        assert_builder_accepts(Cow::Owned(URL.to_owned()));
    }

    #[test]
    fn builder_rejects_unparseable_url() {
        let result = WsIoClient::builder("not a url");

        match result {
            Ok(_) => panic!("invalid URL should fail"),
            Err(err) => assert!(err.to_string().contains("Invalid URL")),
        }
    }

    #[test]
    fn builder_rejects_wss_without_tls_features() {
        let result = WsIoClient::builder("wss://localhost/socket");

        if cfg!(any(
            feature = "tls-native",
            feature = "tls-rustls-native",
            feature = "tls-rustls-webpki",
        )) {
            assert!(result.is_ok());
        } else {
            match result {
                Ok(_) => panic!("wss URL should fail when no TLS feature is enabled"),
                Err(err) => assert!(err.to_string().contains("TLS is required")),
            }
        }
    }
}
