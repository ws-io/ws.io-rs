use anyhow::Result;
use bytes::Bytes;
use erased_serde::{
    Deserializer,
    Serialize,
};

use super::super::WsIoPacket;

// Traits

/// Implements a custom codec for ws.io packets and packet payloads.
///
/// The codec controls the serialization format. Packet inputs are borrowed, and
/// encoded data is returned as owned [`Bytes`]. Payload methods use erased Serde
/// values so implementations can be stored behind `Arc<dyn ...>`.
pub trait WsIoPacketCustomCodec: Send + Sync + 'static {
    /// Decodes one complete ws.io packet.
    fn decode(&self, bytes: &[u8]) -> Result<WsIoPacket>;

    /// Creates a type-erased deserializer for one packet payload.
    fn decode_data<'de>(&self, bytes: &'de [u8]) -> Result<Box<dyn Deserializer<'de> + 'de>>;

    /// Encodes one complete ws.io packet.
    fn encode(&self, packet: &WsIoPacket) -> Result<Bytes>;

    /// Encodes one type-erased packet payload.
    fn encode_data(&self, data: &dyn Serialize) -> Result<Bytes>;
}
