use anyhow::Result;
use bytes::Bytes;
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
    #[inline]
    pub(super) fn decode(bytes: &[u8]) -> Result<WsIoPacket> {
        WsIoPacket::from_inner(from_slice(bytes)?)
    }

    #[inline]
    pub(super) fn decode_data<D: DeserializeOwned>(bytes: &[u8]) -> Result<D> {
        Ok(from_slice(bytes)?)
    }

    #[inline]
    pub(super) fn encode(packet: &WsIoPacket) -> Result<Bytes> {
        Ok(to_vec_named(&packet.to_inner_ref())?.into())
    }

    #[inline]
    pub(super) fn encode_data<D: Serialize>(data: &D) -> Result<Bytes> {
        Ok(to_vec_named(data)?.into())
    }
}
