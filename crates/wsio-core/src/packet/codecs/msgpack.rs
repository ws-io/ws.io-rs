use anyhow::Result;
use rmp_serde::{
    from_slice,
    to_vec_named,
};
use serde::{
    Serialize,
    de::DeserializeOwned,
};

use super::super::WsIoPacket;

// Structs
pub(super) struct WsIoPacketMsgpackCodec;

impl WsIoPacketMsgpackCodec {
    pub(super) const IS_TEXT: bool = false;

    #[inline]
    pub(super) fn decode(&self, bytes: &[u8]) -> Result<WsIoPacket> {
        Ok(WsIoPacket::from_inner(from_slice(bytes)?))
    }

    #[inline]
    pub(super) fn decode_data<D: DeserializeOwned>(&self, bytes: &[u8]) -> Result<D> {
        Ok(from_slice(bytes)?)
    }

    #[inline]
    pub(super) fn encode(&self, packet: &WsIoPacket) -> Result<Vec<u8>> {
        Ok(to_vec_named(&packet.to_inner_ref())?)
    }

    #[inline]
    pub(super) fn encode_data<D: Serialize>(&self, data: &D) -> Result<Vec<u8>> {
        Ok(to_vec_named(data)?)
    }
}
