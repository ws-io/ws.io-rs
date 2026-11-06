use std::{
    fmt::{
        Debug as FmtDebug,
        Formatter,
        Result as FmtResult,
    },
    sync::Arc,
};

use anyhow::Result;
use bytes::Bytes;

#[cfg(any(feature = "packet-transformer-lz4", feature = "packet-transformer-zstd"))]
mod compression;
pub mod custom;
#[cfg(feature = "packet-transformer-lz4")]
pub mod lz4;
#[cfg(feature = "packet-transformer-zstd")]
pub mod zstd;

#[cfg(any(feature = "packet-transformer-lz4", feature = "packet-transformer-zstd"))]
use self::compression::WsIoPacketCompressionTransformer;
use self::custom::WsIoPacketCustomTransformer;
#[cfg(feature = "packet-transformer-lz4")]
use self::lz4::WsIoPacketLz4TransformerConfig;
#[cfg(feature = "packet-transformer-zstd")]
use self::zstd::WsIoPacketZstdTransformerConfig;

// Enums
#[derive(Clone, Default)]
enum WsIoPacketTransformerKind {
    #[default]
    Noop,

    #[cfg(any(feature = "packet-transformer-lz4", feature = "packet-transformer-zstd"))]
    Compression(WsIoPacketCompressionTransformer),
    Custom(Arc<dyn WsIoPacketCustomTransformer>),
}

// Structs

/// A selectable packet transformation strategy for a client or server.
///
/// [`Default::default`] selects the no-op strategy, which returns the input
/// allocation unchanged. [`Self::custom`] delegates to a user-supplied transformer
/// through an [`Arc`]. LZ4 and zstd are optional compression strategies that share
/// framing, size limits and bounded blocking work. zstd also reuses its contexts
/// across calls and clones.
#[derive(Clone, Default)]
pub struct WsIoPacketTransformer {
    kind: WsIoPacketTransformerKind,
}

impl FmtDebug for WsIoPacketTransformer {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match &self.kind {
            WsIoPacketTransformerKind::Noop => f.write_str("WsIoPacketTransformer::Noop"),

            #[cfg(any(feature = "packet-transformer-lz4", feature = "packet-transformer-zstd"))]
            WsIoPacketTransformerKind::Compression(transformer) => f.write_str(transformer.debug_name()),
            WsIoPacketTransformerKind::Custom(_) => f.write_str("WsIoPacketTransformer::Custom(<transformer>)"),
        }
    }
}

impl WsIoPacketTransformer {
    /// Creates a packet transformer backed by a custom implementation.
    #[inline]
    pub fn custom(transformer: Arc<dyn WsIoPacketCustomTransformer>) -> Self {
        Self {
            kind: WsIoPacketTransformerKind::Custom(transformer),
        }
    }

    /// Decodes one complete transformed WebSocket packet.
    ///
    /// Processing begins when the returned future is polled. This consumes `bytes`
    /// and returns owned bytes; the no-op strategy preserves the input allocation.
    ///
    /// # Errors
    ///
    /// Propagates errors from a custom transformer. With a built-in compression strategy,
    /// returns an error for invalid compression headers or payloads, unsupported
    /// frame versions or algorithms, or lengths exceeding the configured limit.
    /// Decompression and blocking-task failures are also propagated.
    ///
    /// # Cancellation safety
    ///
    /// Dropping the future discards its result. Already submitted compression blocking work
    /// may continue to completion. A custom transformer defines its own surviving
    /// side effects and reuse guarantees.
    #[inline]
    pub async fn decode(&self, bytes: Bytes) -> Result<Bytes> {
        match &self.kind {
            WsIoPacketTransformerKind::Noop => Ok(bytes),

            #[cfg(any(feature = "packet-transformer-lz4", feature = "packet-transformer-zstd"))]
            WsIoPacketTransformerKind::Compression(transformer) => transformer.decode(bytes).await,
            WsIoPacketTransformerKind::Custom(transformer) => transformer.decode(&bytes).await,
        }
    }

    /// Encodes one complete codec packet for WebSocket transmission.
    ///
    /// Processing begins when the returned future is polled. This consumes `bytes`
    /// and returns owned bytes; the no-op strategy preserves the input allocation.
    ///
    /// # Errors
    ///
    /// Propagates errors from a custom transformer. With a built-in compression strategy,
    /// returns an error if the input exceeds the configured size limit or
    /// `u32::MAX` bytes. Compression and blocking-task failures are also propagated.
    ///
    /// # Cancellation safety
    ///
    /// Dropping the future discards its result. Already submitted compression blocking work
    /// may continue to completion. A custom transformer defines its own surviving
    /// side effects and reuse guarantees.
    #[inline]
    pub async fn encode(&self, bytes: Bytes) -> Result<Bytes> {
        match &self.kind {
            WsIoPacketTransformerKind::Noop => Ok(bytes),

            #[cfg(any(feature = "packet-transformer-lz4", feature = "packet-transformer-zstd"))]
            WsIoPacketTransformerKind::Compression(transformer) => transformer.encode(bytes).await,
            WsIoPacketTransformerKind::Custom(transformer) => transformer.encode(&bytes).await,
        }
    }

    /// Creates a built-in LZ4 block packet transformer with `config`.
    ///
    /// Requires `packet-transformer-lz4`. Peers must select the same transformer.
    /// Uses the ws.io compression frame with algorithm `2`, a big-endian `u32`
    /// original length and no additional LZ4 frame or length prefix. Packets below
    /// the compression threshold or without a size reduction use the raw flag.
    #[cfg(feature = "packet-transformer-lz4")]
    #[inline]
    pub fn lz4(config: WsIoPacketLz4TransformerConfig) -> Self {
        Self {
            kind: WsIoPacketTransformerKind::Compression(WsIoPacketCompressionTransformer::lz4(config)),
        }
    }

    /// Creates a built-in zstd packet transformer with `config`.
    ///
    /// Requires `packet-transformer-zstd`. Peers must use the same transformation
    /// format. Frames contain a one-byte descriptor followed by a big-endian `u32`
    /// original payload length in bytes, then the raw or zstd-compressed payload.
    /// Descriptor bits 5 through 7 hold version `1`, bits 1 through 4 hold algorithm
    /// `1` (zstd), and bit 0 indicates compression. Compression falls back to raw
    /// payload data if it does not reduce the size. Encoding and decoding enforce
    /// [`WsIoPacketZstdTransformerConfig::max_decompressed_size`].
    #[cfg(feature = "packet-transformer-zstd")]
    #[inline]
    pub fn zstd(config: WsIoPacketZstdTransformerConfig) -> Self {
        Self {
            kind: WsIoPacketTransformerKind::Compression(WsIoPacketCompressionTransformer::zstd(config)),
        }
    }
}

#[cfg(test)]
mod tests;
