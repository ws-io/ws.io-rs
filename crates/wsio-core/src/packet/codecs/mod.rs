use std::{
    fmt::{
        Debug as FmtDebug,
        Formatter,
        Result as FmtResult,
    },
    sync::Arc,
};

use anyhow::Result;
use bytes::Bytes;
use erased_serde::deserialize;
use serde::{
    Serialize,
    de::DeserializeOwned,
};

#[cfg(feature = "packet-codec-cbor")]
mod cbor;
pub mod custom;
mod msgpack;
#[cfg(feature = "packet-codec-postcard")]
mod postcard;

#[cfg(feature = "packet-codec-cbor")]
use self::cbor::WsIoPacketCborCodec;
#[cfg(feature = "packet-codec-postcard")]
use self::postcard::WsIoPacketPostcardCodec;
use self::{
    custom::WsIoPacketCustomCodec,
    msgpack::WsIoPacketMsgpackCodec,
};
use super::WsIoPacket;

// Enums
#[derive(Clone)]
pub enum WsIoPacketCodec {
    #[cfg(feature = "packet-codec-cbor")]
    Cbor,
    Custom(Arc<dyn WsIoPacketCustomCodec>),
    Msgpack,

    #[cfg(feature = "packet-codec-postcard")]
    Postcard,
}

impl FmtDebug for WsIoPacketCodec {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            #[cfg(feature = "packet-codec-cbor")]
            Self::Cbor => f.write_str("WsIoPacketCodec::Cbor"),
            Self::Msgpack => f.write_str("WsIoPacketCodec::Msgpack"),
            Self::Custom(_) => f.write_str("WsIoPacketCodec::Custom(<codec>)"),
            #[cfg(feature = "packet-codec-postcard")]
            Self::Postcard => f.write_str("WsIoPacketCodec::Postcard"),
        }
    }
}

impl WsIoPacketCodec {
    #[inline]
    pub fn decode(&self, bytes: &[u8]) -> Result<WsIoPacket> {
        match self {
            #[cfg(feature = "packet-codec-cbor")]
            Self::Cbor => WsIoPacketCborCodec::decode(bytes),
            Self::Custom(codec) => codec.decode(bytes),
            Self::Msgpack => WsIoPacketMsgpackCodec::decode(bytes),

            #[cfg(feature = "packet-codec-postcard")]
            Self::Postcard => WsIoPacketPostcardCodec::decode(bytes),
        }
    }

    #[inline]
    pub fn decode_data<D: DeserializeOwned>(&self, bytes: &[u8]) -> Result<D> {
        match self {
            #[cfg(feature = "packet-codec-cbor")]
            Self::Cbor => WsIoPacketCborCodec::decode_data(bytes),
            Self::Custom(codec) => {
                let mut deserializer = codec.decode_data(bytes)?;
                Ok(deserialize(&mut *deserializer)?)
            },
            Self::Msgpack => WsIoPacketMsgpackCodec::decode_data(bytes),

            #[cfg(feature = "packet-codec-postcard")]
            Self::Postcard => WsIoPacketPostcardCodec::decode_data(bytes),
        }
    }

    #[inline]
    pub fn encode(&self, packet: &WsIoPacket) -> Result<Bytes> {
        match self {
            #[cfg(feature = "packet-codec-cbor")]
            Self::Cbor => WsIoPacketCborCodec::encode(packet),
            Self::Custom(codec) => codec.encode(packet),
            Self::Msgpack => WsIoPacketMsgpackCodec::encode(packet),

            #[cfg(feature = "packet-codec-postcard")]
            Self::Postcard => WsIoPacketPostcardCodec::encode(packet),
        }
    }

    #[inline]
    pub fn encode_data<D: Serialize>(&self, data: &D) -> Result<Bytes> {
        match self {
            #[cfg(feature = "packet-codec-cbor")]
            Self::Cbor => WsIoPacketCborCodec::encode_data(data),
            Self::Custom(codec) => codec.encode_data(data),
            Self::Msgpack => WsIoPacketMsgpackCodec::encode_data(data),

            #[cfg(feature = "packet-codec-postcard")]
            Self::Postcard => WsIoPacketPostcardCodec::encode_data(data),
        }
    }
}

#[cfg(test)]
mod tests;
