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

#[cfg(feature = "packet-transformer-zstd")]
mod compression_frame;
#[cfg(feature = "packet-transformer-zstd")]
mod context_pool;
pub mod custom;
#[cfg(feature = "packet-transformer-zstd")]
pub mod zstd;

use self::custom::WsIoPacketCustomTransformer;
#[cfg(feature = "packet-transformer-zstd")]
use self::zstd::{
    WsIoPacketZstdTransformer,
    WsIoPacketZstdTransformerConfig,
};

// Enums
#[derive(Clone, Default)]
enum WsIoPacketTransformerKind {
    #[default]
    Noop,
    Custom(Arc<dyn WsIoPacketCustomTransformer>),

    #[cfg(feature = "packet-transformer-zstd")]
    Zstd(WsIoPacketZstdTransformer),
}

// Structs

/// A selectable packet transformation strategy for a client or server.
///
/// [`Default::default`] selects the no-op strategy, which returns the input
/// allocation unchanged. [`Self::custom`] delegates to a user-supplied transformer
/// through an [`Arc`]. The zstd strategy is available with the `packet-transformer-zstd`
/// feature and reuses its zstd contexts across calls and clones.
#[derive(Clone, Default)]
pub struct WsIoPacketTransformer {
    kind: WsIoPacketTransformerKind,
}

impl FmtDebug for WsIoPacketTransformer {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match &self.kind {
            WsIoPacketTransformerKind::Noop => f.write_str("WsIoPacketTransformer::Noop"),
            WsIoPacketTransformerKind::Custom(_) => f.write_str("WsIoPacketTransformer::Custom(<transformer>)"),

            #[cfg(feature = "packet-transformer-zstd")]
            WsIoPacketTransformerKind::Zstd(_) => f.write_str("WsIoPacketTransformer::Zstd(<config>)"),
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
    /// Propagates errors from a custom transformer. With `packet-transformer-zstd`,
    /// returns an error for invalid compression headers or payloads, unsupported
    /// frame versions or algorithms, or lengths exceeding the configured limit.
    /// Decompression and blocking-task failures are also propagated.
    ///
    /// # Cancellation safety
    ///
    /// Dropping the future discards its result. Already submitted zstd blocking work
    /// may continue to completion. A custom transformer defines its own surviving
    /// side effects and reuse guarantees.
    #[inline]
    pub async fn decode(&self, bytes: Bytes) -> Result<Bytes> {
        match &self.kind {
            WsIoPacketTransformerKind::Noop => Ok(bytes),
            WsIoPacketTransformerKind::Custom(transformer) => transformer.decode(&bytes).await,

            #[cfg(feature = "packet-transformer-zstd")]
            WsIoPacketTransformerKind::Zstd(transformer) => transformer.decode(bytes).await,
        }
    }

    /// Encodes one complete codec packet for WebSocket transmission.
    ///
    /// Processing begins when the returned future is polled. This consumes `bytes`
    /// and returns owned bytes; the no-op strategy preserves the input allocation.
    ///
    /// # Errors
    ///
    /// Propagates errors from a custom transformer. With `packet-transformer-zstd`,
    /// returns an error if the input exceeds the configured size limit or
    /// `u32::MAX` bytes. Compression and blocking-task failures are also propagated.
    ///
    /// # Cancellation safety
    ///
    /// Dropping the future discards its result. Already submitted zstd blocking work
    /// may continue to completion. A custom transformer defines its own surviving
    /// side effects and reuse guarantees.
    #[inline]
    pub async fn encode(&self, bytes: Bytes) -> Result<Bytes> {
        match &self.kind {
            WsIoPacketTransformerKind::Noop => Ok(bytes),
            WsIoPacketTransformerKind::Custom(transformer) => transformer.encode(&bytes).await,

            #[cfg(feature = "packet-transformer-zstd")]
            WsIoPacketTransformerKind::Zstd(transformer) => transformer.encode(bytes).await,
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
            kind: WsIoPacketTransformerKind::Zstd(WsIoPacketZstdTransformer::new(config)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::bail;
    use async_trait::async_trait;

    use super::*;

    struct ReverseTransformer;

    #[async_trait]
    impl WsIoPacketCustomTransformer for ReverseTransformer {
        async fn decode(&self, bytes: &[u8]) -> Result<Bytes> {
            self.encode(bytes).await
        }

        async fn encode(&self, bytes: &[u8]) -> Result<Bytes> {
            let mut output = bytes.to_vec();
            output.reverse();
            Ok(output.into())
        }
    }

    struct FailingTransformer;

    #[async_trait]
    impl WsIoPacketCustomTransformer for FailingTransformer {
        async fn decode(&self, _bytes: &[u8]) -> Result<Bytes> {
            bail!("decode failed")
        }

        async fn encode(&self, _bytes: &[u8]) -> Result<Bytes> {
            bail!("encode failed")
        }
    }

    #[tokio::test]
    async fn noop_preserves_the_input_allocation() {
        let bytes = Bytes::from_static(b"encoded packet");
        let input_pointer = bytes.as_ptr();
        let transformer = WsIoPacketTransformer::default();

        let encoded = transformer.encode(bytes).await.unwrap();
        assert_eq!(encoded.as_ptr(), input_pointer);

        let decoded = transformer.decode(encoded).await.unwrap();
        assert_eq!(decoded.as_ptr(), input_pointer);
        assert_eq!(&decoded[..], b"encoded packet");
    }

    #[tokio::test]
    async fn custom_transformer_round_trips_bytes() {
        let transformer = WsIoPacketTransformer::custom(Arc::new(ReverseTransformer));
        let input = Bytes::from_static(b"encoded packet");

        let encoded = transformer.encode(input).await.unwrap();
        assert_eq!(&encoded[..], b"tekcap dedocne");

        let decoded = transformer.decode(encoded).await.unwrap();
        assert_eq!(&decoded[..], b"encoded packet");
    }

    #[tokio::test]
    async fn custom_transformer_errors_are_propagated() {
        let transformer = WsIoPacketTransformer::custom(Arc::new(FailingTransformer));
        let input = Bytes::from_static(b"encoded packet");

        assert_eq!(
            transformer.encode(input.clone()).await.unwrap_err().to_string(),
            "encode failed"
        );

        assert_eq!(
            transformer.decode(input).await.unwrap_err().to_string(),
            "decode failed"
        );
    }

    #[test]
    fn debug_output_uses_the_public_type_name() {
        assert_eq!(
            format!("{:?}", WsIoPacketTransformer::default()),
            "WsIoPacketTransformer::Noop"
        );

        let transformer = WsIoPacketTransformer::custom(Arc::new(ReverseTransformer));
        assert_eq!(
            format!("{transformer:?}"),
            "WsIoPacketTransformer::Custom(<transformer>)"
        );
    }
}
