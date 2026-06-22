use anyhow::Result;
use postcard::{
    from_bytes,
    to_allocvec,
};
use serde::{
    Serialize,
    de::DeserializeOwned,
};

use super::super::WsIoPacket;

// Structs
pub(super) struct WsIoPacketPostcardCodec;

impl WsIoPacketPostcardCodec {
    pub(super) const IS_TEXT: bool = false;

    #[inline]
    pub(super) fn decode(bytes: &[u8]) -> Result<WsIoPacket> {
        Ok(WsIoPacket::from_inner(from_bytes(bytes)?))
    }

    #[inline]
    pub(super) fn decode_data<D: DeserializeOwned>(bytes: &[u8]) -> Result<D> {
        Ok(from_bytes::<D>(bytes)?)
    }

    #[inline]
    pub(super) fn encode(packet: &WsIoPacket) -> Result<Vec<u8>> {
        Ok(to_allocvec(&packet.to_inner_ref())?)
    }

    #[inline]
    pub(super) fn encode_data<D: Serialize>(data: &D) -> Result<Vec<u8>> {
        Ok(to_allocvec(data)?)
    }
}
