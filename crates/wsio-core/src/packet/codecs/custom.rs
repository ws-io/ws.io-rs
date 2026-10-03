use anyhow::Result;
use bytes::Bytes;
use erased_serde::{
    Deserializer,
    Serialize,
};

use super::super::WsIoPacket;

// Traits

/// A custom serialization contract for ws.io packets and packet payloads.
///
/// The codec controls the serialization format. Packet inputs are borrowed, and
/// encoded data is returned as owned [`Bytes`]. Payload methods use erased Serde
/// values so implementations can be shared through a trait object. Calls may
/// occur concurrently; implementations must use a compatible format for
/// encoding and decoding.
pub trait WsIoPacketCustomCodec: Send + Sync + 'static {
    /// Decodes one complete ws.io packet.
    ///
    /// # Errors
    ///
    /// Returns an implementation-defined error if `bytes` is invalid or unsupported
    /// for the codec.
    fn decode(&self, bytes: &[u8]) -> Result<WsIoPacket>;

    /// Creates a type-erased deserializer for one packet payload.
    ///
    /// The returned deserializer may borrow `bytes` and cannot outlive it.
    ///
    /// # Errors
    ///
    /// Returns an implementation-defined error if a deserializer cannot be created
    /// for `bytes`. Further decoding errors may occur when the deserializer is used.
    fn decode_data<'de>(&self, bytes: &'de [u8]) -> Result<Box<dyn Deserializer<'de> + 'de>>;

    /// Encodes one complete ws.io packet.
    ///
    /// # Errors
    ///
    /// Returns an implementation-defined error if `packet` cannot be represented
    /// in the codec format or serialization fails.
    fn encode(&self, packet: &WsIoPacket) -> Result<Bytes>;

    /// Encodes one type-erased packet payload.
    ///
    /// # Errors
    ///
    /// Returns an implementation-defined error if `data` cannot be represented
    /// in the codec format or serialization fails.
    fn encode_data(&self, data: &dyn Serialize) -> Result<Bytes>;
}
