use anyhow::Result;
use bytes::Bytes;
use lz4_flex::block::{
    compress_into,
    decompress_into,
    get_maximum_output_size,
};

use super::compression::frame::{
    COMPRESSION_FRAME_HEADER_SIZE,
    WsIoPacketCompressionFrameHeader,
};

// Structs
/// Configuration for the built-in LZ4 block packet transformer.
#[derive(Debug, Clone, Copy)]
pub struct WsIoPacketLz4TransformerConfig {
    /// The minimum work size for dispatching to Tokio's blocking pool.
    ///
    /// Encoding uses input size; decoding uses the larger of encoded and original
    /// sizes. Defaults to 64 KiB. `usize::MAX` keeps work inline; `0` offloads all
    /// attempted compression and decompression when a Tokio runtime is active.
    pub blocking_threshold: usize,

    /// The minimum packet size for attempting compression. Defaults to 256 bytes.
    ///
    /// Smaller packets and compression without a size reduction use raw payloads.
    pub compression_threshold: usize,

    /// Maximum uncompressed packet size accepted during encoding and decoding.
    ///
    /// Defaults to 16 MiB. The frame also limits input to `u32::MAX` bytes.
    /// `0` accepts only empty uncompressed payloads.
    pub max_decompressed_size: usize,
}

impl Default for WsIoPacketLz4TransformerConfig {
    fn default() -> Self {
        Self {
            blocking_threshold: 64 * 1024,
            compression_threshold: 256,
            max_decompressed_size: 16 * 1024 * 1024,
        }
    }
}

// Functions
pub(super) fn compress_frame(bytes: &[u8], header: WsIoPacketCompressionFrameHeader) -> Result<Vec<u8>> {
    let capacity = get_maximum_output_size(bytes.len());
    let mut output = header.allocate(capacity)?;
    output.resize(COMPRESSION_FRAME_HEADER_SIZE + capacity, 0);
    let compressed_length = compress_into(bytes, &mut output[COMPRESSION_FRAME_HEADER_SIZE..])?;
    output.truncate(COMPRESSION_FRAME_HEADER_SIZE + compressed_length);
    Ok(output)
}

pub(super) fn decompress(payload: &[u8], original_length: usize) -> Result<Bytes> {
    let mut output = vec![0; original_length];
    let decoded_length = decompress_into(payload, &mut output)?;
    output.truncate(decoded_length);
    Ok(output.into())
}

#[cfg(test)]
mod tests;
