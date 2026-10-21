use std::time::Duration;

use http::{
    HeaderMap,
    Uri,
};
use tokio::{
    sync::mpsc,
    time::timeout,
};
use tokio_tungstenite::tungstenite::Message;

use super::*;
use crate::{
    WsIoServer,
    core::packet::WsIoPacketType,
};

async fn connection(namespace: &Arc<WsIoServerNamespace>) -> (Arc<WsIoServerConnection>, mpsc::Receiver<Arc<Message>>) {
    let (connection, mut messages, _events) =
        WsIoServerConnection::new(HeaderMap::new(), Arc::clone(namespace), Uri::from_static("/ws.io"));

    connection.init().await.unwrap();
    messages.try_recv().unwrap();
    connection
        .handle_incoming_packet(
            namespace
                .config
                .packet_codec
                .encode(&WsIoPacket::new_init(None))
                .unwrap(),
        )
        .await
        .unwrap();

    messages.try_recv().unwrap();
    (connection, messages)
}

#[tokio::test]
async fn room_union_exclusion_and_delivery_are_exactly_once_per_target() {
    let server = WsIoServer::builder().build().unwrap();
    let ns = server.new_namespace_builder("/test").register().unwrap();
    let (a, mut arx) = connection(&ns).await;
    let (b, mut brx) = connection(&ns).await;
    let (c, mut crx) = connection(&ns).await;
    a.join(["red", "blue"]);
    b.join(["red"]);
    c.join(["blue", "excluded"]);
    let cases = [
        (
            WsIoServerNamespaceBroadcastOperator::new(Arc::clone(&ns)),
            vec![a.id(), b.id(), c.id()],
        ),
        (ns.to(["red", "blue", "red"]), vec![a.id(), b.id(), c.id()]),
        (
            ns.to(["red", "blue"])
                .except(["excluded"])
                .except_connection_ids([b.id()]),
            vec![a.id()],
        ),
        (ns.to(["red"]).except(["red"]), vec![]),
        (ns.to(["missing"]), vec![]),
        (
            ns.to(["red"]).except(["missing"]).except_connection_ids([u64::MAX]),
            vec![a.id(), b.id()],
        ),
    ];

    for (operator, expected) in cases {
        let expected: RoaringTreemap = expected.into_iter().collect();
        assert_eq!(operator.target_connection_ids(), expected);
        timeout(Duration::from_secs(2), operator.emit("e", Some(&7_u8)))
            .await
            .unwrap()
            .unwrap();

        for (id, rx) in [(a.id(), &mut arx), (b.id(), &mut brx), (c.id(), &mut crx)] {
            if expected.contains(id) {
                let message = rx.try_recv().unwrap();
                let Message::Binary(bytes) = message.as_ref() else {
                    panic!("expected event")
                };

                let packet = ns.config.packet_codec.decode(bytes).unwrap();
                assert!(matches!(packet.r#type, WsIoPacketType::Event));
                assert_eq!(packet.key.as_deref(), Some("e"));
                assert_eq!(
                    ns.config.packet_codec.decode_data::<u8>(&packet.data.unwrap()).unwrap(),
                    7
                );
            }

            assert!(rx.try_recv().is_err(), "unexpected or duplicate delivery to {id}");
        }
    }

    a.cleanup().await;
    b.cleanup().await;
    c.cleanup().await;
    assert!(ns.rooms.is_empty());
    assert!(ns.connections.is_empty());
    assert_eq!(server.connection_count(), 0);
    assert!(
        WsIoServerNamespaceBroadcastOperator::new(ns)
            .target_connection_ids()
            .is_empty()
    );
}

#[tokio::test]
async fn membership_indexes_survive_duplicate_join_leave_and_cleanup() {
    let server = WsIoServer::builder().build().unwrap();
    let ns = server.new_namespace_builder("/test").register().unwrap();
    let (a, _arx) = connection(&ns).await;
    let (b, _brx) = connection(&ns).await;
    a.join(["shared", "shared", "solo"]);
    b.join(["shared"]);
    assert_eq!(ns.rooms.get("shared").unwrap().len(), 2);
    a.leave(["shared", "shared", "missing"]);
    assert_eq!(
        *ns.rooms.get("shared").unwrap(),
        [b.id()].into_iter().collect::<RoaringTreemap>()
    );

    a.leave(["solo"]);
    assert!(!ns.rooms.contains_key("solo"));
    a.join(["shared", "solo"]);
    a.close();
    a.join(["late"]);
    a.cleanup().await;
    assert!(!ns.rooms.contains_key("late"));
    assert!(!ns.rooms.contains_key("solo"));
    assert_eq!(
        *ns.rooms.get("shared").unwrap(),
        [b.id()].into_iter().collect::<RoaringTreemap>()
    );

    assert_eq!(ns.connection_count(), 1);
    assert_eq!(server.connection_count(), 1);
    b.cleanup().await;
    assert!(ns.rooms.is_empty());
    assert_eq!(server.connection_count(), 0);
}
