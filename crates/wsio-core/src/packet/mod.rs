use anyhow::{
    Result,
    bail,
};
use bytes::Bytes;
use serde::{
    Deserialize,
    Serialize,
};
use serde_repr::{
    Deserialize_repr,
    Serialize_repr,
};

pub mod codecs;
pub mod transformers;

// Enums
#[repr(u8)]
#[derive(Debug, Clone, Serialize_repr, Deserialize_repr)]
pub enum WsIoPacketType {
    Disconnect = 0,
    Event = 1,
    Init = 2,
    Ready = 3,
}

// Structs
#[derive(Deserialize)]
struct InnerPacket(WsIoPacketType, Option<String>, Option<Bytes>);

#[derive(Serialize)]
struct InnerPacketRef<'a>(&'a WsIoPacketType, &'a Option<String>, &'a Option<Bytes>);

#[derive(Debug, Clone)]
pub struct WsIoPacket {
    pub data: Option<Bytes>,
    pub key: Option<String>,
    pub r#type: WsIoPacketType,
}

impl WsIoPacket {
    #[inline]
    pub fn new(r#type: WsIoPacketType, key: Option<&str>, data: Option<Bytes>) -> Self {
        Self {
            data,
            key: key.map(str::to_owned),
            r#type,
        }
    }

    // Protected methods
    #[inline]
    pub(self) fn from_inner(inner: InnerPacket) -> Result<Self> {
        let InnerPacket(r#type, key, data) = inner;
        if matches!(&r#type, WsIoPacketType::Event) && key.as_deref().is_none_or(str::is_empty) {
            bail!("Event packet missing key");
        }

        Ok(Self { data, key, r#type })
    }

    #[inline]
    pub(self) fn to_inner_ref(&self) -> InnerPacketRef<'_> {
        InnerPacketRef(&self.r#type, &self.key, &self.data)
    }

    // Public methods
    #[inline]
    pub fn new_disconnect() -> Self {
        Self::new(WsIoPacketType::Disconnect, None, None)
    }

    #[inline]
    pub fn new_event(event: impl Into<String>, data: Option<Bytes>) -> Self {
        Self {
            data,
            key: Some(event.into()),
            r#type: WsIoPacketType::Event,
        }
    }

    #[inline]
    pub fn new_init(data: Option<Bytes>) -> Self {
        Self::new(WsIoPacketType::Init, None, data)
    }

    #[inline]
    pub fn new_ready() -> Self {
        Self::new(WsIoPacketType::Ready, None, None)
    }
}
