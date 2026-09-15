use std::sync::Arc;

use anyhow::{
    Result,
    ensure,
};
use bytes::Bytes;
use tokio::{
    runtime::Handle,
    task::spawn_blocking,
};

mod context_pool;

use self::context_pool::WsIoPacketZstdContextPool;
use super::compression_frame::{
    COMPRESSION_FRAME_HEADER_SIZE,
    WsIoPacketCompressionAlgorithm,
    WsIoPacketCompressionFrameHeader,
};

// Structs

#[derive(Clone)]
pub(super) struct WsIoPacketZstdTransformer {
    config: WsIoPacketZstdTransformerConfig,
    context_pool: Arc<WsIoPacketZstdContextPool>,
}

impl WsIoPacketZstdTransformer {
    #[inline]
    pub(super) fn new(config: WsIoPacketZstdTransformerConfig) -> Self {
        Self {
            config,
            context_pool: Arc::new(WsIoPacketZstdContextPool::new(config.compression_level)),
        }
    }

    #[inline]
    pub(super) async fn decode(&self, mut bytes: Bytes) -> Result<Bytes> {
        let header = WsIoPacketCompressionFrameHeader::decode(&bytes)?;
        ensure!(
            header.algorithm == WsIoPacketCompressionAlgorithm::Zstd,
            "invalid zstd transformer algorithm"
        );

        let original_length = usize::try_from(header.original_length)?;
        ensure!(
            original_length <= self.config.max_decompressed_size,
            "packet exceeds the configured maximum decompressed size"
        );

        let payload = bytes.split_off(COMPRESSION_FRAME_HEADER_SIZE);
        drop(bytes);

        if !header.compressed {
            ensure!(
                payload.len() == original_length,
                "invalid raw zstd transformer payload length"
            );

            return Ok(payload);
        }

        if self.config.should_use_blocking(payload.len().max(original_length)) && Handle::try_current().is_ok() {
            let permit = self.context_pool.acquire_blocking_permit().await?;
            let context_pool = self.context_pool.clone();
            return spawn_blocking(move || {
                let result = context_pool.decompress(&payload, original_length);
                drop(permit);
                result
            })
            .await?;
        }

        self.context_pool.decompress(&payload, original_length)
    }

    #[inline]
    pub(super) async fn encode(&self, bytes: Bytes) -> Result<Bytes> {
        ensure!(
            bytes.len() <= self.config.max_decompressed_size,
            "packet exceeds the configured maximum decompressed size"
        );

        let raw_header =
            WsIoPacketCompressionFrameHeader::new(WsIoPacketCompressionAlgorithm::Zstd, false, bytes.len())?;

        if bytes.len() < self.config.compression_threshold {
            return raw_header.encode_payload(&bytes);
        }

        if self.config.should_use_blocking(bytes.len()) && Handle::try_current().is_ok() {
            let permit = self.context_pool.acquire_blocking_permit().await?;
            let context_pool = self.context_pool.clone();
            return spawn_blocking(move || {
                let result = context_pool.compress(&bytes, raw_header);
                drop(permit);
                result
            })
            .await?;
        }

        self.context_pool.compress(&bytes, raw_header)
    }
}

/// Configuration for the built-in zstd packet transformer.
#[derive(Clone, Copy, Debug)]
pub struct WsIoPacketZstdTransformerConfig {
    /// Packets at least this many bytes are processed on Tokio's blocking
    /// thread pool when zstd compression or decompression is required.
    ///
    /// The larger of the encoded and original payload sizes is used for the
    /// decode decision. Set this to `usize::MAX` to keep zstd processing on
    /// the async worker. If no Tokio runtime is active, processing falls back
    /// to the synchronous path.
    pub blocking_threshold: usize,

    /// zstd compression level. Level `3` is the default low-latency setting.
    pub compression_level: i32,

    /// Packets smaller than this many bytes are sent with the raw payload flag.
    pub compression_threshold: usize,

    /// Maximum uncompressed packet size in bytes accepted for encoding and decoding.
    ///
    /// The compression frame stores this length as a `u32`, so packets larger
    /// than `u32::MAX` cannot be encoded regardless of this setting.
    pub max_decompressed_size: usize,
}

impl Default for WsIoPacketZstdTransformerConfig {
    fn default() -> Self {
        Self {
            blocking_threshold: 64 * 1024,
            compression_level: 3,
            compression_threshold: 256,
            max_decompressed_size: 16 * 1024 * 1024,
        }
    }
}

impl WsIoPacketZstdTransformerConfig {
    #[inline]
    fn should_use_blocking(&self, payload_size: usize) -> bool {
        payload_size >= self.blocking_threshold
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::{
        super::WsIoPacketTransformer,
        WsIoPacketZstdTransformerConfig,
    };

    #[tokio::test]
    async fn blocking_threshold_round_trips_zstd_packets() {
        let transformer = WsIoPacketTransformer::zstd(WsIoPacketZstdTransformerConfig {
            blocking_threshold: 1,
            ..Default::default()
        });
        let input = Bytes::from(vec![b'a'; 4096]);

        let encoded = transformer.encode(input.clone()).await.unwrap();
        let decoded = transformer.decode(encoded).await.unwrap();

        assert_eq!(decoded, input);
    }
}
