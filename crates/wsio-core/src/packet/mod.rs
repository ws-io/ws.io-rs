use serde::{
    Deserialize,
    Serialize,
};
use serde_repr::{
    Deserialize_repr,
    Serialize_repr,
};

pub mod codecs;

// Enums
#[repr(u8)]
#[derive(Clone, Debug, Deserialize_repr, Serialize_repr)]
pub enum WsIoPacketType {
    Disconnect = 0,
    Event = 1,
    Init = 2,
    Ready = 3,
}

// Structs
#[derive(Deserialize)]
struct InnerPacket(WsIoPacketType, Option<String>, Option<Vec<u8>>);

#[derive(Serialize)]
struct InnerPacketRef<'a>(&'a WsIoPacketType, &'a Option<String>, &'a Option<Vec<u8>>);

#[derive(Clone, Debug)]
pub struct WsIoPacket {
    pub data: Option<Vec<u8>>,
    pub key: Option<String>,
    pub r#type: WsIoPacketType,
}

impl WsIoPacket {
    #[inline]
    pub fn new(r#type: WsIoPacketType, key: Option<&str>, data: Option<Vec<u8>>) -> Self {
        Self {
            data,
            key: key.map(|k| k.into()),
            r#type,
        }
    }

    // Protected methods
    #[inline]
    pub(self) fn from_inner(inner: InnerPacket) -> Self {
        Self {
            data: inner.2,
            key: inner.1,
            r#type: inner.0,
        }
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
    pub fn new_event(event: &str, data: Option<Vec<u8>>) -> Self {
        Self::new(WsIoPacketType::Event, Some(event), data)
    }

    #[inline]
    pub fn new_init(data: Option<Vec<u8>>) -> Self {
        Self::new(WsIoPacketType::Init, None, data)
    }

    #[inline]
    pub fn new_ready() -> Self {
        Self::new(WsIoPacketType::Ready, None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_packet_constructors() {
        // Disconnect
        let packet = WsIoPacket::new_disconnect();
        assert!(matches!(packet.r#type, WsIoPacketType::Disconnect));
        assert_eq!(packet.key, None);
        assert_eq!(packet.data, None);

        // Event without data
        let packet = WsIoPacket::new_event("chat", None);
        assert!(matches!(packet.r#type, WsIoPacketType::Event));
        assert_eq!(packet.key.as_deref(), Some("chat"));
        assert_eq!(packet.data, None);

        // Event with data
        let packet = WsIoPacket::new_event("chat", Some(vec![1, 2, 3]));
        assert!(matches!(packet.r#type, WsIoPacketType::Event));
        assert_eq!(packet.key.as_deref(), Some("chat"));
        assert_eq!(packet.data.as_deref(), Some(&[1, 2, 3][..]));

        // Init with data
        let packet = WsIoPacket::new_init(Some(vec![4, 5, 6]));
        assert!(matches!(packet.r#type, WsIoPacketType::Init));
        assert_eq!(packet.key, None);
        assert_eq!(packet.data.as_deref(), Some(&[4, 5, 6][..]));

        // Ready
        let packet = WsIoPacket::new_ready();
        assert!(matches!(packet.r#type, WsIoPacketType::Ready));
        assert_eq!(packet.key, None);
        assert_eq!(packet.data, None);
    }
}
