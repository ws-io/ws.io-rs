use ::sonic_rs::{
    from_slice,
    to_vec,
};
use anyhow::Result;
use serde::{
    Serialize,
    de::DeserializeOwned,
};

use super::super::WsIoPacket;

// Structs
pub(super) struct WsIoPacketSonicRsCodec;

impl WsIoPacketSonicRsCodec {
    pub(super) const IS_TEXT: bool = true;

    #[inline]
    pub(super) fn decode(bytes: &[u8]) -> Result<WsIoPacket> {
        Ok(WsIoPacket::from_inner(from_slice(bytes)?))
    }

    #[inline]
    pub(super) fn decode_data<D: DeserializeOwned>(bytes: &[u8]) -> Result<D> {
        Ok(from_slice(bytes)?)
    }

    #[inline]
    pub(super) fn encode(packet: &WsIoPacket) -> Result<Vec<u8>> {
        Ok(to_vec(&packet.to_inner_ref())?)
    }

    #[inline]
    pub(super) fn encode_data<D: Serialize>(data: &D) -> Result<Vec<u8>> {
        Ok(to_vec(data)?)
    }
}
