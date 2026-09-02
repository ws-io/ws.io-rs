use std::io::Cursor;

use anyhow::Result;
use bytes::Bytes;
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
    #[inline]
    pub(super) fn decode(bytes: &[u8]) -> Result<WsIoPacket> {
        WsIoPacket::from_inner(from_reader(Cursor::new(bytes))?)
    }

    #[inline]
    pub(super) fn decode_data<D: DeserializeOwned>(bytes: &[u8]) -> Result<D> {
        Ok(from_reader(Cursor::new(bytes))?)
    }

    #[inline]
    pub(super) fn encode(packet: &WsIoPacket) -> Result<Bytes> {
        let mut buffer = Vec::new();
        into_writer(&packet.to_inner_ref(), &mut buffer)?;
        Ok(Bytes::from(buffer))
    }

    #[inline]
    pub(super) fn encode_data<D: Serialize>(data: &D) -> Result<Bytes> {
        let mut buffer = Vec::new();
        into_writer(data, &mut buffer)?;
        Ok(Bytes::from(buffer))
    }
}
