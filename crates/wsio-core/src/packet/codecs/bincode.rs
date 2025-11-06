use ::bincode::{
    config::standard,
    serde::{
        decode_from_slice,
        encode_to_vec,
    },
};
use anyhow::Result;
use serde::{
    Serialize,
    de::DeserializeOwned,
};

use super::super::WsIoPacket;

// Structs
pub(super) struct WsIoPacketBincodeCodec;

impl WsIoPacketBincodeCodec {
    pub(super) const IS_TEXT: bool = false;

    #[inline]
    pub(super) fn decode(&self, bytes: &[u8]) -> Result<WsIoPacket> {
        let (inner_packet, _) = decode_from_slice(bytes, standard())?;
        Ok(WsIoPacket::from_inner(inner_packet))
    }

    #[inline]
    pub(super) fn decode_data<D: DeserializeOwned>(&self, bytes: &[u8]) -> Result<D> {
        let (data, _) = decode_from_slice(bytes, standard())?;
        Ok(data)
    }

    #[inline]
    pub(super) fn encode(&self, packet: &WsIoPacket) -> Result<Vec<u8>> {
        Ok(encode_to_vec(packet.to_inner_ref(), standard())?)
    }

    #[inline]
    pub(super) fn encode_data<D: Serialize>(&self, data: &D) -> Result<Vec<u8>> {
        Ok(encode_to_vec(data, standard())?)
    }
}
