use anyhow::Result;
use bytes::Bytes;
use serde::{
    Serialize,
    de::DeserializeOwned,
};

use super::super::WsIoPacket;

// Structs
pub(super) struct WsIoPacketPostcardCodec;

impl WsIoPacketPostcardCodec {
    #[inline]
    pub(super) fn decode(bytes: &[u8]) -> Result<WsIoPacket> {
        WsIoPacket::from_inner(postcard::from_bytes(bytes)?)
    }

    #[inline]
    pub(super) fn decode_data<D: DeserializeOwned>(bytes: &[u8]) -> Result<D> {
        Ok(postcard::from_bytes::<D>(bytes)?)
    }

    #[inline]
    pub(super) fn encode(packet: &WsIoPacket) -> Result<Bytes> {
        Ok(postcard::to_allocvec(&packet.to_inner_ref())?.into())
    }

    #[inline]
    pub(super) fn encode_data<D: Serialize>(data: &D) -> Result<Bytes> {
        Ok(postcard::to_allocvec(data)?.into())
    }
}
