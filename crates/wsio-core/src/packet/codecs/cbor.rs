use std::io::Cursor;

use anyhow::Result;
use ciborium::{
    de::from_reader,
    ser::into_writer,
};
use serde::{
    Serialize,
    de::DeserializeOwned,
};

use super::super::WsIoPacket;

// Structs
pub(super) struct WsIoPacketCborCodec;

impl WsIoPacketCborCodec {
    pub(super) const IS_TEXT: bool = false;

    #[inline]
    pub(super) fn decode(&self, bytes: &[u8]) -> Result<WsIoPacket> {
        Ok(WsIoPacket::from_inner(from_reader(Cursor::new(bytes))?))
    }

    #[inline]
    pub(super) fn decode_data<D: DeserializeOwned>(&self, bytes: &[u8]) -> Result<D> {
        Ok(from_reader(Cursor::new(bytes))?)
    }

    #[inline]
    pub(super) fn encode(&self, packet: &WsIoPacket) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        into_writer(&packet.to_inner_ref(), &mut buffer)?;
        Ok(buffer)
    }

    #[inline]
    pub(super) fn encode_data<D: Serialize>(&self, data: &D) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        into_writer(data, &mut buffer)?;
        Ok(buffer)
    }
}
