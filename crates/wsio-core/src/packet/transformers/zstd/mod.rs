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
            let context_pool = Arc::clone(&self.context_pool);
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
            let context_pool = Arc::clone(&self.context_pool);
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
#[derive(Debug, Clone, Copy)]
pub struct WsIoPacketZstdTransformerConfig {
    /// The minimum payload size in bytes for dispatching zstd work to Tokio's
    /// blocking thread pool.
    ///
    /// Encoding uses the encoded payload size; decoding uses the larger of the
    /// encoded and original sizes. Set this to `usize::MAX` to keep zstd work on
    /// the async worker. Without an active Tokio runtime, processing is
    /// synchronous. Defaults to 64 KiB. `0` dispatches all attempted compression
    /// and decompression to blocking threads when a Tokio runtime is active.
    pub blocking_threshold: usize,

    /// The compression level used by zstd for encoding.
    ///
    /// Defaults to `3`. The value is passed to zstd when creating a compressor;
    /// zstd errors are propagated by packet encoding.
    pub compression_level: i32,

    /// The minimum packet size in bytes for attempting zstd compression.
    ///
    /// Smaller packets use the raw payload flag. Defaults to `256` bytes. `0`
    /// attempts compression for every packet; `usize::MAX` effectively disables
    /// compression for encodable packets.
    pub compression_threshold: usize,

    /// The maximum uncompressed packet size in bytes accepted during encoding and decoding.
    ///
    /// The compression frame stores this length as a `u32`, so packets larger
    /// than `u32::MAX` cannot be encoded regardless of this setting. Defaults to
    /// 16 MiB. `0` accepts only empty uncompressed payloads.
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
