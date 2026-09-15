use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;

// Traits

/// Transforms complete encoded ws.io packets before they cross the WebSocket.
///
/// Encoding runs after the packet codec, and decoding runs before the packet
/// codec. The crate does not prescribe the transformation; implementations may
/// encrypt, compress, or otherwise process the bytes.
///
/// Both methods receive borrowed bytes and return owned [`Bytes`]. Implementations
/// must use the `async-trait` crate's `async_trait` attribute and may await
/// arbitrary asynchronous work.
///
/// One-byte transformed packets are unsupported. The server reserves every
/// one-byte binary WebSocket frame for the client heartbeat, so an encoder that
/// returns one byte produces a packet that the server ignores.
#[async_trait]
pub trait WsIoPacketCustomTransformer: Send + Sync + 'static {
    /// Decodes one transformed packet received from the WebSocket.
    async fn decode(&self, bytes: &[u8]) -> Result<Bytes>;

    /// Encodes one codec packet before it is sent over the WebSocket.
    async fn encode(&self, bytes: &[u8]) -> Result<Bytes>;
}
