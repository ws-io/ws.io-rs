use std::{
    future::pending,
    sync::Arc,
    time::Duration,
};

use anyhow::{
    Result,
    bail,
    ensure,
};
use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::mpsc;

use crate::packet::transformers::{
    WsIoPacketTransformer,
    custom::WsIoPacketCustomTransformer,
};

// Constants/Statics
pub(crate) const TEST_TIMEOUT: Duration = Duration::from_secs(2);

// Structs

/// Prefixes one tag byte and rejects inputs larger than `max_input`.
struct TaggedTransformer {
    max_input: usize,
}

#[async_trait]
impl WsIoPacketCustomTransformer for TaggedTransformer {
    async fn decode(&self, bytes: &[u8]) -> Result<Bytes> {
        let Some((0xa5, payload)) = bytes.split_first() else {
            bail!("invalid custom tag")
        };

        Ok(Bytes::copy_from_slice(payload))
    }

    async fn encode(&self, bytes: &[u8]) -> Result<Bytes> {
        ensure!(bytes.len() <= self.max_input, "custom input too large");
        let mut output = vec![0xa5];
        output.extend_from_slice(bytes);
        Ok(output.into())
    }
}

// Functions

/// Reports that it started, then stays pending while holding `alive` until dropped.
pub(crate) async fn park_until_dropped(
    started: mpsc::UnboundedSender<()>,
    _alive: mpsc::UnboundedSender<()>,
) -> Result<()> {
    started.send(()).unwrap();
    pending().await
}

pub(crate) fn tagged_transformer(max_input: usize) -> WsIoPacketTransformer {
    WsIoPacketTransformer::custom(Arc::new(TaggedTransformer { max_input }))
}
