use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;

// Traits

/// An asynchronous transformation contract for complete encoded ws.io packets.
///
/// Encoding runs after the packet codec, and decoding runs before the packet
/// codec. The crate does not prescribe the transformation; implementations may
/// encrypt, compress, or otherwise process the bytes.
///
/// Both methods receive borrowed bytes and return owned [`Bytes`]. Implementations
/// must use [`async_trait::async_trait`] and may await arbitrary asynchronous
/// work. Calls may occur concurrently, and peer implementations must use
/// compatible transformations.
///
/// Client-to-server transformed packets must not contain exactly one byte. The
/// server treats every one-byte binary WebSocket frame as a client heartbeat
/// and ignores it before decoding. Server-to-client one-byte frames are passed
/// to the client's decoder instead.
#[async_trait]
pub trait WsIoPacketCustomTransformer: Send + Sync + 'static {
    /// Decodes one transformed packet received from the WebSocket.
    ///
    /// Processing begins when the returned future is polled.
    ///
    /// # Errors
    ///
    /// Returns an implementation-defined error if the input is unsupported or
    /// invalid, or if decoding fails.
    ///
    /// # Cancellation safety
    ///
    /// Dropping the future cancels unfinished asynchronous work. Implementations
    /// must document any side effects that survive cancellation and whether they
    /// remain reusable after cancellation.
    async fn decode(&self, bytes: &[u8]) -> Result<Bytes>;

    /// Encodes one codec packet before it is sent over the WebSocket.
    ///
    /// Processing begins when the returned future is polled.
    ///
    /// # Errors
    ///
    /// Returns an implementation-defined error if the input is unsupported or
    /// encoding fails.
    ///
    /// # Cancellation safety
    ///
    /// Dropping the future cancels unfinished asynchronous work. Implementations
    /// must document any side effects that survive cancellation and whether they
    /// remain reusable after cancellation.
    async fn encode(&self, bytes: &[u8]) -> Result<Bytes>;
}
