use std::{
    sync::Arc,
    thread::available_parallelism,
};

use anyhow::{
    Result,
    ensure,
};
use bytes::Bytes;
use tokio::{
    runtime::Handle,
    sync::{
        OwnedSemaphorePermit,
        Semaphore,
    },
    task::spawn_blocking,
};

pub(super) mod frame;

use self::frame::{
    COMPRESSION_FRAME_HEADER_SIZE,
    WsIoPacketCompressionAlgorithm,
    WsIoPacketCompressionFrameHeader,
};
#[cfg(feature = "packet-transformer-lz4")]
use super::lz4::{
    self,
    WsIoPacketLz4TransformerConfig,
};
#[cfg(feature = "packet-transformer-zstd")]
use super::zstd::{
    WsIoPacketZstdTransformerConfig,
    context_pool::WsIoPacketZstdContextPool,
};

// Enums
enum WsIoPacketCompressionBackend {
    #[cfg(feature = "packet-transformer-lz4")]
    Lz4,

    #[cfg(feature = "packet-transformer-zstd")]
    Zstd(WsIoPacketZstdContextPool),
}

impl WsIoPacketCompressionBackend {
    fn algorithm(&self) -> WsIoPacketCompressionAlgorithm {
        match self {
            #[cfg(feature = "packet-transformer-lz4")]
            Self::Lz4 => WsIoPacketCompressionAlgorithm::Lz4,

            #[cfg(feature = "packet-transformer-zstd")]
            Self::Zstd(_) => WsIoPacketCompressionAlgorithm::Zstd,
        }
    }

    fn compress_frame(&self, bytes: &[u8], header: WsIoPacketCompressionFrameHeader) -> Result<Vec<u8>> {
        match self {
            #[cfg(feature = "packet-transformer-lz4")]
            Self::Lz4 => lz4::compress_frame(bytes, header),

            #[cfg(feature = "packet-transformer-zstd")]
            Self::Zstd(context_pool) => context_pool.compress_frame(bytes, header),
        }
    }

    fn decompress(&self, payload: &[u8], original_length: usize) -> Result<Bytes> {
        match self {
            #[cfg(feature = "packet-transformer-lz4")]
            Self::Lz4 => lz4::decompress(payload, original_length),
            #[cfg(feature = "packet-transformer-zstd")]
            Self::Zstd(context_pool) => context_pool.decompress(payload, original_length),
        }
    }
}

// Structs
struct WsIoPacketCompressionState {
    backend: WsIoPacketCompressionBackend,
    blocking_semaphore: Arc<Semaphore>,
    blocking_threshold: usize,
    compression_threshold: usize,
    max_decompressed_size: usize,
}

#[derive(Clone)]
pub(super) struct WsIoPacketCompressionTransformer(Arc<WsIoPacketCompressionState>);

impl WsIoPacketCompressionTransformer {
    fn new(
        backend: WsIoPacketCompressionBackend,
        blocking_threshold: usize,
        compression_threshold: usize,
        max_decompressed_size: usize,
    ) -> Self {
        Self(Arc::new(WsIoPacketCompressionState {
            backend,
            blocking_semaphore: Arc::new(Semaphore::new(available_parallelism().map_or(1, usize::from))),
            blocking_threshold,
            compression_threshold,
            max_decompressed_size,
        }))
    }

    pub(super) async fn acquire_blocking_permit(&self) -> Result<OwnedSemaphorePermit> {
        Ok(Arc::clone(&self.0.blocking_semaphore).acquire_owned().await?)
    }

    pub(super) fn debug_name(&self) -> &'static str {
        match &self.0.backend {
            #[cfg(feature = "packet-transformer-lz4")]
            WsIoPacketCompressionBackend::Lz4 => "WsIoPacketTransformer::Lz4(<config>)",
            #[cfg(feature = "packet-transformer-zstd")]
            WsIoPacketCompressionBackend::Zstd(_) => "WsIoPacketTransformer::Zstd(<config>)",
        }
    }

    pub(super) async fn decode(&self, mut bytes: Bytes) -> Result<Bytes> {
        let header = WsIoPacketCompressionFrameHeader::decode(&bytes)?;
        ensure!(
            header.algorithm == self.0.backend.algorithm(),
            "invalid compression transformer algorithm"
        );

        let original_length = usize::try_from(header.original_length)?;
        ensure!(
            original_length <= self.0.max_decompressed_size,
            "packet exceeds the configured maximum decompressed size"
        );

        let payload = bytes.split_off(COMPRESSION_FRAME_HEADER_SIZE);
        drop(bytes);

        if !header.compressed {
            ensure!(
                payload.len() == original_length,
                "invalid raw compression transformer payload length"
            );

            return Ok(payload);
        }

        self.process(payload.len().max(original_length), move |backend| {
            let output = backend.decompress(&payload, original_length)?;
            ensure!(
                output.len() == original_length,
                "compression transformer decoded length does not match the frame"
            );

            Ok(output)
        })
        .await
    }

    pub(super) async fn encode(&self, bytes: Bytes) -> Result<Bytes> {
        ensure!(
            bytes.len() <= self.0.max_decompressed_size,
            "packet exceeds the configured maximum decompressed size"
        );

        let raw_header = WsIoPacketCompressionFrameHeader::new(self.0.backend.algorithm(), false, bytes.len())?;
        if bytes.len() < self.0.compression_threshold {
            return raw_header.encode_payload(&bytes);
        }

        self.process(bytes.len(), move |backend| {
            let mut output = backend.compress_frame(&bytes, raw_header.with_compressed(true))?;
            if output.len() - COMPRESSION_FRAME_HEADER_SIZE >= bytes.len() {
                output.truncate(COMPRESSION_FRAME_HEADER_SIZE);
                output.extend_from_slice(&bytes);
                WsIoPacketCompressionFrameHeader::set_compressed_flag(&mut output, false);
            }

            Ok(output.into())
        })
        .await
    }

    async fn process(
        &self,
        work_size: usize,
        operation: impl FnOnce(&WsIoPacketCompressionBackend) -> Result<Bytes> + Send + 'static,
    ) -> Result<Bytes> {
        if work_size >= self.0.blocking_threshold && Handle::try_current().is_ok() {
            let permit = self.acquire_blocking_permit().await?;
            let state = Arc::clone(&self.0);
            return spawn_blocking(move || {
                let result = operation(&state.backend);
                drop(permit);
                result
            })
            .await?;
        }

        operation(&self.0.backend)
    }

    #[cfg(feature = "packet-transformer-lz4")]
    pub(super) fn lz4(config: WsIoPacketLz4TransformerConfig) -> Self {
        Self::new(
            WsIoPacketCompressionBackend::Lz4,
            config.blocking_threshold,
            config.compression_threshold,
            config.max_decompressed_size,
        )
    }

    #[cfg(feature = "packet-transformer-zstd")]
    pub(super) fn zstd(config: WsIoPacketZstdTransformerConfig) -> Self {
        Self::new(
            WsIoPacketCompressionBackend::Zstd(WsIoPacketZstdContextPool::new(config.compression_level)),
            config.blocking_threshold,
            config.compression_threshold,
            config.max_decompressed_size,
        )
    }
}

#[cfg(test)]
mod tests;
