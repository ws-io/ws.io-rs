use std::{
    slice::from_raw_parts_mut,
    thread::available_parallelism,
};

use anyhow::{
    Result,
    ensure,
};
use bytes::Bytes;
use parking_lot::Mutex;
use zstd::{
    bulk::{
        Compressor,
        Decompressor,
    },
    zstd_safe::compress_bound,
};

use super::compression_frame::{
    COMPRESSION_FRAME_HEADER_SIZE,
    WsIoPacketCompressionAlgorithm,
    WsIoPacketCompressionFrameHeader,
};

// Structs
pub(super) struct WsIoPacketZstdContextPool {
    compressors: Mutex<Vec<Compressor<'static>>>,
    decompressors: Mutex<Vec<Decompressor<'static>>>,
    max_contexts: usize,
}

impl WsIoPacketZstdContextPool {
    pub(super) fn new() -> Self {
        Self {
            compressors: Mutex::new(Vec::new()),
            decompressors: Mutex::new(Vec::new()),
            max_contexts: available_parallelism().map_or(1, usize::from),
        }
    }

    #[inline]
    fn acquire_compressor(&self, compression_level: i32) -> Result<Compressor<'static>> {
        self.compressors
            .lock()
            .pop()
            .map_or_else(|| Ok(Compressor::new(compression_level)?), Ok)
    }

    #[inline]
    fn acquire_decompressor(&self) -> Result<Decompressor<'static>> {
        self.decompressors
            .lock()
            .pop()
            .map_or_else(|| Ok(Decompressor::new()?), Ok)
    }

    #[inline]
    fn release_compressor(&self, compressor: Compressor<'static>) {
        let mut compressors = self.compressors.lock();
        if compressors.len() < self.max_contexts {
            compressors.push(compressor);
        }
    }

    #[inline]
    fn release_decompressor(&self, decompressor: Decompressor<'static>) {
        let mut decompressors = self.decompressors.lock();
        if decompressors.len() < self.max_contexts {
            decompressors.push(decompressor);
        }
    }
}

pub(super) struct WsIoPacketZstdTransformer;

impl WsIoPacketZstdTransformer {
    #[inline]
    pub(super) fn decode(
        config: &WsIoPacketZstdTransformerConfig,
        context_pool: &WsIoPacketZstdContextPool,
        mut bytes: Bytes,
    ) -> Result<Bytes> {
        let header = WsIoPacketCompressionFrameHeader::decode(&bytes)?;
        ensure!(
            header.algorithm == WsIoPacketCompressionAlgorithm::Zstd,
            "invalid zstd transformer algorithm"
        );

        let original_length = usize::try_from(header.original_length)?;
        ensure!(
            original_length <= config.max_decompressed_size,
            "packet exceeds the configured maximum decompressed size"
        );

        if !header.compressed {
            ensure!(
                bytes.len() - COMPRESSION_FRAME_HEADER_SIZE == original_length,
                "invalid raw zstd transformer payload length"
            );

            return Ok(bytes.split_off(COMPRESSION_FRAME_HEADER_SIZE));
        }

        let mut decompressor = context_pool.acquire_decompressor()?;
        let result = decode_compressed(&mut decompressor, bytes, original_length);
        context_pool.release_decompressor(decompressor);
        result
    }

    #[inline]
    pub(super) fn encode(
        config: &WsIoPacketZstdTransformerConfig,
        context_pool: &WsIoPacketZstdContextPool,
        bytes: &[u8],
    ) -> Result<Bytes> {
        ensure!(
            bytes.len() <= config.max_decompressed_size,
            "packet exceeds the configured maximum decompressed size"
        );

        let raw_header =
            WsIoPacketCompressionFrameHeader::new(WsIoPacketCompressionAlgorithm::Zstd, false, bytes.len())?;

        if bytes.len() < config.compression_threshold {
            return raw_header.encode_payload(bytes);
        }

        let mut compressor = context_pool.acquire_compressor(config.compression_level)?;
        let result = encode_compressed(&mut compressor, bytes, raw_header);
        context_pool.release_compressor(compressor);
        result
    }
}

/// Configuration for the built-in zstd packet transformer.
#[derive(Clone, Copy, Debug)]
pub struct WsIoPacketZstdTransformerConfig {
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
            compression_level: 3,
            compression_threshold: 256,
            max_decompressed_size: 16 * 1024 * 1024,
        }
    }
}

// Functions
#[inline]
fn decode_compressed(
    decompressor: &mut Decompressor<'static>,
    mut bytes: Bytes,
    original_length: usize,
) -> Result<Bytes> {
    let payload = bytes.split_off(COMPRESSION_FRAME_HEADER_SIZE);
    drop(bytes);

    let mut output = Vec::with_capacity(original_length);
    let destination = unsafe {
        // SAFETY: The pointer and length cover exactly the Vec spare
        // capacity. zstd writes only within the supplied destination and
        // returns the initialized byte count.
        from_raw_parts_mut(output.spare_capacity_mut().as_mut_ptr().cast::<u8>(), original_length)
    };

    let decoded_length = decompressor.decompress_to_buffer(&payload, destination)?;
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
fn encode_compressed(
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
