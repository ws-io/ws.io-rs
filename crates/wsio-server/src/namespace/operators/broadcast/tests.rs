use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;

use super::*;
use crate::{
    WsIoServer,
    core::packet::WsIoPacketType,
    test_support::{
        TEST_TIMEOUT,
        ready_connection,
    },
};

#[tokio::test]
async fn room_union_exclusion_and_delivery_are_exactly_once_per_target() {
    let server = WsIoServer::builder().build().unwrap();
    let namespace = server.new_namespace_builder("/test").register().unwrap();
    let (first, mut first_messages) = ready_connection(&namespace).await;
    let (second, mut second_messages) = ready_connection(&namespace).await;
    let (third, mut third_messages) = ready_connection(&namespace).await;
    first.join(["red", "blue"]);
    second.join(["red"]);
    third.join(["blue", "excluded"]);
    let cases = [
        (
            WsIoServerNamespaceBroadcastOperator::new(Arc::clone(&namespace)),
            vec![first.id(), second.id(), third.id()],
        ),
        (
            namespace.to(["red", "blue", "red"]),
            vec![first.id(), second.id(), third.id()],
        ),
        (
            namespace
                .to(["red", "blue"])
                .except(["excluded"])
                .except_connection_ids([second.id()]),
            vec![first.id()],
        ),
        (namespace.to(["red"]).except(["red"]), vec![]),
        (namespace.to(["missing"]), vec![]),
        (
            namespace
                .to(["red"])
                .except(["missing"])
                .except_connection_ids([u64::MAX]),
            vec![first.id(), second.id()],
        ),
    ];

    for (operator, expected) in cases {
        let expected: RoaringTreemap = expected.into_iter().collect();
        assert_eq!(operator.target_connection_ids(), expected);
        timeout(TEST_TIMEOUT, operator.emit("e", Some(&7_u8)))
            .await
            .unwrap()
            .unwrap();

        for (id, messages) in [
            (first.id(), &mut first_messages),
            (second.id(), &mut second_messages),
            (third.id(), &mut third_messages),
        ] {
            if expected.contains(id) {
                let message = messages.try_recv().unwrap();
                let Message::Binary(bytes) = message.as_ref() else {
                    panic!("expected event")
                };

                let bytes = namespace.config.packet_transformer.decode(bytes.clone()).await.unwrap();
                let packet = namespace.config.packet_codec.decode(&bytes).unwrap();
                assert!(matches!(packet.r#type, WsIoPacketType::Event));
                assert_eq!(packet.key.as_deref(), Some("e"));
                assert_eq!(
                    namespace
                        .config
                        .packet_codec
                        .decode_data::<u8>(&packet.data.unwrap())
                        .unwrap(),
                    7
                );
            }

            assert!(messages.try_recv().is_err(), "unexpected or duplicate delivery to {id}");
        }
    }

    first.cleanup().await;
    second.cleanup().await;
    third.cleanup().await;
    assert!(namespace.rooms.is_empty());
    assert!(namespace.connections.is_empty());
    assert_eq!(server.connection_count(), 0);
    assert!(
        WsIoServerNamespaceBroadcastOperator::new(namespace)
            .target_connection_ids()
            .is_empty()
    );
}
