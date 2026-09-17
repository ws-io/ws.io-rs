use std::{
    slice::from_raw_parts_mut,
    sync::Arc,
    thread::available_parallelism,
};

use anyhow::{
    Result,
    ensure,
};
use bytes::Bytes;
use tokio::sync::{
    OwnedSemaphorePermit,
    Semaphore,
};
use zstd::{
    bulk::{
        Compressor,
        Decompressor,
    },
    zstd_safe::compress_bound,
};

use super::super::{
    compression_frame::{
        COMPRESSION_FRAME_HEADER_SIZE,
        WsIoPacketCompressionFrameHeader,
    },
    context_pool::WsIoPacketContextCache,
};

// Structs
pub(super) struct WsIoPacketZstdContextPool {
    blocking_semaphore: Arc<Semaphore>,
    compression_level: i32,
    compressors: WsIoPacketContextCache<Compressor<'static>>,
    decompressors: WsIoPacketContextCache<Decompressor<'static>>,
    max_contexts: usize,
}

impl WsIoPacketZstdContextPool {
    #[inline]
    pub(super) fn new(compression_level: i32) -> Self {
        let max_contexts = available_parallelism().map_or(1, usize::from);

        Self {
            blocking_semaphore: Arc::new(Semaphore::new(max_contexts)),
            compression_level,
            compressors: WsIoPacketContextCache::new(),
            decompressors: WsIoPacketContextCache::new(),
            max_contexts,
        }
    }

    #[inline]
    pub(super) async fn acquire_blocking_permit(&self) -> Result<OwnedSemaphorePermit> {
        Ok(Arc::clone(&self.blocking_semaphore).acquire_owned().await?)
    }

    #[inline]
    pub(super) fn compress(&self, bytes: &[u8], raw_header: WsIoPacketCompressionFrameHeader) -> Result<Bytes> {
        self.with_compressor(|compressor| Self::compress_with_context(compressor, bytes, raw_header))
    }

    #[inline]
    fn compress_with_context(
        compressor: &mut Compressor<'static>,
        bytes: &[u8],
        raw_header: WsIoPacketCompressionFrameHeader,
    ) -> Result<Bytes> {
        let compressed_header = raw_header.with_compressed(true);

        let compression_bound = compress_bound(bytes.len());
        let payload_capacity = compression_bound.max(bytes.len());
        let mut output = compressed_header.allocate(payload_capacity)?;

        let destination = unsafe {
            // SAFETY: The pointer and length cover exactly the Vec spare
            // capacity. zstd writes only within the supplied destination and
            // returns the initialized byte count.
            from_raw_parts_mut(output.spare_capacity_mut().as_mut_ptr().cast::<u8>(), payload_capacity)
        };

        let compressed_len = compressor.compress_to_buffer(bytes, destination)?;

        if compressed_len >= bytes.len() {
            destination[..bytes.len()].copy_from_slice(bytes);
            WsIoPacketCompressionFrameHeader::set_compressed_flag(&mut output, false);
            unsafe {
                // SAFETY: The header and raw payload have been initialized.
                output.set_len(COMPRESSION_FRAME_HEADER_SIZE + bytes.len());
            }
        } else {
            unsafe {
                // SAFETY: The header and zstd-reported compressed payload have
                // been initialized.
                output.set_len(COMPRESSION_FRAME_HEADER_SIZE + compressed_len);
            }
        }

        Ok(output.into())
    }

    #[inline]
    fn with_compressor<T>(&self, operation: impl FnOnce(&mut Compressor<'static>) -> Result<T>) -> Result<T> {
        self.compressors.with_context(
            self.max_contexts,
            || Ok(Compressor::new(self.compression_level)?),
            operation,
        )
    }

    #[inline]
    pub(super) fn decompress(&self, payload: &[u8], original_length: usize) -> Result<Bytes> {
        self.with_decompressor(|decompressor| Self::decompress_with_context(decompressor, payload, original_length))
    }

    #[inline]
    fn decompress_with_context(
        decompressor: &mut Decompressor<'static>,
        payload: &[u8],
        original_length: usize,
    ) -> Result<Bytes> {
        let mut output = Vec::with_capacity(original_length);
        let destination = unsafe {
            // SAFETY: The pointer and length cover exactly the Vec spare
            // capacity. zstd writes only within the supplied destination and
            // returns the initialized byte count.
            from_raw_parts_mut(output.spare_capacity_mut().as_mut_ptr().cast::<u8>(), original_length)
        };

        let decoded_length = decompressor.decompress_to_buffer(payload, destination)?;
        ensure!(
            decoded_length == original_length,
            "zstd transformer decoded length does not match the frame"
        );

        unsafe {
            // SAFETY: zstd reported that exactly `decoded_length` bytes were
            // written to the destination buffer.
            output.set_len(decoded_length);
        }

        Ok(output.into())
    }

    #[inline]
    fn with_decompressor<T>(&self, operation: impl FnOnce(&mut Decompressor<'static>) -> Result<T>) -> Result<T> {
        self.decompressors
            .with_context(self.max_contexts, || Ok(Decompressor::new()?), operation)
    }
}
