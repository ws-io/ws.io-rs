use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;

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
/// Implementations must use the `async-trait` crate's `async_trait` attribute
/// and may await arbitrary asynchronous work in either method.
///
/// One-byte transformed packets are currently unsupported. The server reserves
/// every one-byte binary WebSocket frame for the client heartbeat, so a custom
/// encoder that returns one byte produces a packet that the server ignores.
#[async_trait]
pub trait WsIoPacketCustomTransformer: Send + Sync + 'static {
    /// Reverses [`Self::encode`] after a WebSocket packet is received.
    async fn decode(&self, bytes: &[u8]) -> Result<Bytes>;

    /// Transforms an encoded packet before it is sent over the WebSocket.
    async fn encode(&self, bytes: &[u8]) -> Result<Bytes>;
}
