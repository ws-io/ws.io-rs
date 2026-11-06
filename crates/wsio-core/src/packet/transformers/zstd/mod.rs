pub(super) mod context_pool;

// Structs
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

#[cfg(test)]
mod tests;
