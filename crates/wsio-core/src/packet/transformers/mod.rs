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

/// Transforms complete encoded ws.io packets before they cross the WebSocket.
///
/// A transformer is applied after the packet codec on the sending side and
/// before the packet codec on the receiving side. The crate does not prescribe
/// the transformation; implementations may encrypt, compress, or otherwise
/// process the bytes. The input is borrowed so implementations can choose the
/// output allocation and the no-op enum variant can preserve the original
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

/// The packet transformation strategy used by a client or server.
///
/// `Noop` is the default and keeps the already encoded [`Bytes`] allocation
/// without copying. `Custom` delegates to the user-supplied transformer through
/// an [`Arc`] so the same strategy can be shared by runtimes, namespaces, and
/// connections.
#[non_exhaustive]
#[derive(Clone, Default)]
pub enum WsIoPacketTransformer {
    #[default]
    Noop,
    Custom(Arc<dyn WsIoCustomPacketTransformer>),
}

impl FmtDebug for WsIoPacketTransformer {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Noop => f.write_str("WsIoPacketTransformer::Noop"),
            Self::Custom(_) => f.write_str("WsIoPacketTransformer::Custom(<transformer>)"),
        }
    }
}

impl WsIoPacketTransformer {
    #[inline]
    pub fn decode_bytes(&self, bytes: Bytes) -> Result<Bytes> {
        match self {
            Self::Noop => Ok(bytes),
            Self::Custom(transformer) => transformer.decode(&bytes),
        }
    }

    #[inline]
    pub fn encode_bytes(&self, bytes: Bytes) -> Result<Bytes> {
        match self {
            Self::Noop => Ok(bytes),
            Self::Custom(transformer) => transformer.encode(&bytes),
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

        let encoded = WsIoPacketTransformer::Noop.encode_bytes(bytes).unwrap();
        assert_eq!(encoded.as_ptr(), input_pointer);

        let decoded = WsIoPacketTransformer::Noop.decode_bytes(encoded).unwrap();
        assert_eq!(decoded.as_ptr(), input_pointer);
        assert_eq!(&decoded[..], b"encoded packet");
    }

    #[test]
    fn custom_transformer_round_trips_bytes() {
        let transformer = WsIoPacketTransformer::Custom(Arc::new(ReverseTransformer));
        let input = Bytes::from_static(b"encoded packet");

        let encoded = transformer.encode_bytes(input).unwrap();
        assert_eq!(&encoded[..], b"tekcap dedocne");

        let decoded = transformer.decode_bytes(encoded).unwrap();
        assert_eq!(&decoded[..], b"encoded packet");
    }

    #[test]
    fn custom_transformer_errors_are_propagated() {
        let transformer = WsIoPacketTransformer::Custom(Arc::new(FailingTransformer));
        let input = Bytes::from_static(b"encoded packet");

        assert_eq!(
            transformer.encode_bytes(input.clone()).unwrap_err().to_string(),
            "encode failed"
        );

        assert_eq!(
            transformer.decode_bytes(input).unwrap_err().to_string(),
            "decode failed"
        );
    }

    #[test]
    fn debug_output_uses_the_public_type_name() {
        assert_eq!(
            format!("{:?}", WsIoPacketTransformer::Noop),
            "WsIoPacketTransformer::Noop"
        );

        let transformer = WsIoPacketTransformer::Custom(Arc::new(ReverseTransformer));
        assert_eq!(
            format!("{transformer:?}"),
            "WsIoPacketTransformer::Custom(<transformer>)"
        );
    }
}
