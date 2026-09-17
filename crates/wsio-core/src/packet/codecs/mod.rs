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
mod tests {
    use bytes::Bytes;
    use serde::{
        Deserialize,
        Serialize,
    };

    use super::*;
    use crate::packet::{
        WsIoPacket,
        WsIoPacketType,
    };

    #[derive(Debug, Deserialize, PartialEq, Serialize)]
    struct TestPayload {
        id: u32,
        message: String,
    }

    macro_rules! test_codec {
        ($codec:expr, $name:ident) => {
            #[test]
            fn $name() {
                let codec = $codec;

                // 1. Test encoding/decoding raw data
                let original_data = TestPayload {
                    id: 42,
                    message: "hello world".to_string(),
                };

                let encoded_data = codec.encode_data(&original_data).expect("Failed to encode data");
                let decoded_data: TestPayload = codec.decode_data(&encoded_data).expect("Failed to decode data");
                assert_eq!(
                    original_data, decoded_data,
                    "Data decoding did not match original"
                );

                // 2. Test encoding/decoding an Event packet with data
                let packet = WsIoPacket::new_event("chat", Some(encoded_data.clone()));
                let encoded_packet = codec.encode(&packet).expect("Failed to encode packet");
                let decoded_packet = codec.decode(&encoded_packet).expect("Failed to decode packet");

                assert!(
                    matches!(decoded_packet.r#type, WsIoPacketType::Event),
                    "Packet type mismatch"
                );

                assert_eq!(decoded_packet.key.as_deref(), Some("chat"), "Packet key mismatch");
                assert_eq!(
                    decoded_packet.data.as_deref(),
                    Some(&encoded_data[..]),
                    "Packet data mismatch"
                );

                // 3. Test encoding/decoding a Disconnect packet (no data, no key)
                let packet = WsIoPacket::new_disconnect();
                let encoded_packet = codec.encode(&packet).expect("Failed to encode disconnect packet");
                let decoded_packet = codec
                    .decode(&encoded_packet)
                    .expect("Failed to decode disconnect packet");

                assert!(
                    matches!(decoded_packet.r#type, WsIoPacketType::Disconnect),
                    "Packet type mismatch"
                );

                assert_eq!(decoded_packet.key, None, "Packet key should be None");
                assert_eq!(decoded_packet.data, None, "Packet data should be None");
            }
        };
    }

    #[cfg(feature = "packet-codec-cbor")]
    test_codec!(WsIoPacketCodec::Cbor, test_cbor_codec);
    test_codec!(WsIoPacketCodec::Msgpack, test_msgpack_codec);

    #[test]
    fn test_msgpack_packet_data_uses_binary_payload() {
        let payload = Bytes::from_static(&[0, 1, 255]);
        let packet = WsIoPacket::new_event("chat", Some(payload.clone()));
        let encoded_packet = WsIoPacketCodec::Msgpack
            .encode(&packet)
            .expect("Failed to encode packet");

        assert!(encoded_packet.windows(2).any(|window| window == [0xc4, 3]));

        let decoded_packet = WsIoPacketCodec::Msgpack
            .decode(&encoded_packet)
            .expect("Failed to decode packet");
        assert_eq!(decoded_packet.data.as_deref(), Some(&payload[..]));
    }

    #[cfg(feature = "packet-codec-postcard")]
    test_codec!(WsIoPacketCodec::Postcard, test_postcard_codec);
}
