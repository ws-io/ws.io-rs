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
pub mod zstd;

#[cfg(feature = "packet-transformer-zstd")]
use self::zstd::{
    WsIoPacketZstdContextPool,
    WsIoPacketZstdTransformer,
    WsIoPacketZstdTransformerConfig,
};

// Traits

/// Transforms complete encoded ws.io packets before they cross the WebSocket.
///
/// A transformer is applied after the packet codec on the sending side and
/// before the packet codec on the receiving side. The crate does not prescribe
/// the transformation; implementations may encrypt, compress, or otherwise
/// process the bytes. The input is borrowed so implementations can choose the
/// output allocation, while the no-op strategy can preserve the original
/// encoded bytes without copying.
///
/// One-byte transformed packets are currently unsupported. The server reserves
/// every one-byte binary WebSocket frame for the client heartbeat, so a custom
/// encoder that returns one byte produces a packet that the server ignores.
pub trait WsIoCustomPacketTransformer: Send + Sync + 'static {
    /// Reverses [`Self::encode`] after a WebSocket packet is received.
    fn decode(&self, bytes: &[u8]) -> Result<Bytes>;

    /// Transforms an encoded packet before it is sent over the WebSocket.
    fn encode(&self, bytes: &[u8]) -> Result<Bytes>;
}

// Enums
#[derive(Default)]
enum WsIoPacketTransformerKind {
    #[default]
    Noop,
    Custom(Arc<dyn WsIoCustomPacketTransformer>),

    #[cfg(feature = "packet-transformer-zstd")]
    Zstd {
        config: WsIoPacketZstdTransformerConfig,
        context_pool: WsIoPacketZstdContextPool,
    },
}

// Structs

/// The packet transformer used by a client or server.
///
/// `WsIoPacketTransformer` owns both the selected strategy and any reusable
/// execution state required by that strategy. `Default::default()` creates the
/// no-op strategy, which keeps the input allocation without copying. `custom`
/// delegates to the user-supplied transformer through an [`Arc`]. `zstd` is
/// available with the `packet-transformer-zstd` feature and reuses built-in
/// zstd contexts across calls.
#[derive(Default)]
pub struct WsIoPacketTransformer {
    kind: WsIoPacketTransformerKind,
}

impl Clone for WsIoPacketTransformer {
    #[inline]
    fn clone(&self) -> Self {
        match &self.kind {
            WsIoPacketTransformerKind::Noop => Self::default(),
            WsIoPacketTransformerKind::Custom(transformer) => Self::custom(transformer.clone()),

            #[cfg(feature = "packet-transformer-zstd")]
            WsIoPacketTransformerKind::Zstd { config, .. } => Self::zstd(*config),
        }
    }
}

impl FmtDebug for WsIoPacketTransformer {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match &self.kind {
            WsIoPacketTransformerKind::Noop => f.write_str("WsIoPacketTransformer::Noop"),
            WsIoPacketTransformerKind::Custom(_) => f.write_str("WsIoPacketTransformer::Custom(<transformer>)"),

            #[cfg(feature = "packet-transformer-zstd")]
            WsIoPacketTransformerKind::Zstd { .. } => f.write_str("WsIoPacketTransformer::Zstd(<config>)"),
        }
    }
}

impl WsIoPacketTransformer {
    /// Creates a custom packet transformer.
    #[inline]
    pub fn custom(transformer: Arc<dyn WsIoCustomPacketTransformer>) -> Self {
        Self {
            kind: WsIoPacketTransformerKind::Custom(transformer),
        }
    }

    /// Decodes one complete transformed packet.
    #[inline]
    pub fn decode(&self, bytes: Bytes) -> Result<Bytes> {
        match &self.kind {
            WsIoPacketTransformerKind::Noop => Ok(bytes),
            WsIoPacketTransformerKind::Custom(transformer) => transformer.decode(&bytes),

            #[cfg(feature = "packet-transformer-zstd")]
            WsIoPacketTransformerKind::Zstd { config, context_pool } => {
                WsIoPacketZstdTransformer::decode(config, context_pool, bytes)
            },
        }
    }

    /// Encodes one complete codec packet.
    #[inline]
    pub fn encode(&self, bytes: Bytes) -> Result<Bytes> {
        match &self.kind {
            WsIoPacketTransformerKind::Noop => Ok(bytes),
            WsIoPacketTransformerKind::Custom(transformer) => transformer.encode(&bytes),

            #[cfg(feature = "packet-transformer-zstd")]
            WsIoPacketTransformerKind::Zstd { config, context_pool } => {
                WsIoPacketZstdTransformer::encode(config, context_pool, &bytes)
            },
        }
    }

    /// Creates a built-in zstd packet transformer.
    #[cfg(feature = "packet-transformer-zstd")]
    #[inline]
    pub fn zstd(config: WsIoPacketZstdTransformerConfig) -> Self {
        Self {
            kind: WsIoPacketTransformerKind::Zstd {
                config,
                context_pool: WsIoPacketZstdContextPool::new(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::anyhow;

    use super::*;

    struct ReverseTransformer;

    impl WsIoCustomPacketTransformer for ReverseTransformer {
        fn decode(&self, bytes: &[u8]) -> Result<Bytes> {
            self.encode(bytes)
        }

        fn encode(&self, bytes: &[u8]) -> Result<Bytes> {
            let mut output = bytes.to_vec();
            output.reverse();
            Ok(output.into())
        }
    }

    struct FailingTransformer;

    impl WsIoCustomPacketTransformer for FailingTransformer {
        fn decode(&self, _bytes: &[u8]) -> Result<Bytes> {
            Err(anyhow!("decode failed"))
        }

        fn encode(&self, _bytes: &[u8]) -> Result<Bytes> {
            Err(anyhow!("encode failed"))
        }
    }

    #[test]
    fn noop_preserves_the_input_allocation() {
        let bytes = Bytes::from_static(b"encoded packet");
        let input_pointer = bytes.as_ptr();
        let transformer = WsIoPacketTransformer::default();

        let encoded = transformer.encode(bytes).unwrap();
        assert_eq!(encoded.as_ptr(), input_pointer);

        let decoded = transformer.decode(encoded).unwrap();
        assert_eq!(decoded.as_ptr(), input_pointer);
        assert_eq!(&decoded[..], b"encoded packet");
    }

    #[test]
    fn custom_transformer_round_trips_bytes() {
        let transformer = WsIoPacketTransformer::custom(Arc::new(ReverseTransformer));
        let input = Bytes::from_static(b"encoded packet");

        let encoded = transformer.encode(input).unwrap();
        assert_eq!(&encoded[..], b"tekcap dedocne");

        let decoded = transformer.decode(encoded).unwrap();
        assert_eq!(&decoded[..], b"encoded packet");
    }

    #[test]
    fn custom_transformer_errors_are_propagated() {
        let transformer = WsIoPacketTransformer::custom(Arc::new(FailingTransformer));
        let input = Bytes::from_static(b"encoded packet");

        assert_eq!(
            transformer.encode(input.clone()).unwrap_err().to_string(),
            "encode failed"
        );

        assert_eq!(transformer.decode(input).unwrap_err().to_string(), "decode failed");
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
