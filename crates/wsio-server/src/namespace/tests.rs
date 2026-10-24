use super::*;
use crate::test_support::ready_connection;

#[test]
fn registration_rejects_namespace_outside_whitelist() {
    let server = WsIoServer::builder().build().unwrap();
    for path in ["/世界", "/a b", "/a/../b"] {
        assert!(server.new_namespace_builder(path).register().is_err(), "{path:?}");
    }

    assert_eq!(server.namespace_count(), 0);
}

#[tokio::test]
async fn namespace_path_spellings_share_one_identity() {
    let server = WsIoServer::builder().build().unwrap();
    for (variants, expected) in [
        (["", "/", "///"], "/"),
        (["test", "/test/", "//test//"], "/test"),
        (["a/b", "/a//b/", "//a///b//"], "/a/b"),
    ] {
        for registered in variants {
            let namespace = server.new_namespace_builder(registered).register().unwrap();
            assert_eq!(namespace.path(), expected);
            for lookup in variants {
                assert!(Arc::ptr_eq(&namespace, &server.of(lookup).unwrap()));
                assert!(server.new_namespace_builder(lookup).register().is_err());
            }

            assert_eq!(server.namespace_count(), 1);
            // Remove through a spelling different from the registered one.
            let removal = variants.into_iter().find(|&lookup| lookup != registered).unwrap();
            server.remove_namespace(removal).await;
            assert_eq!(server.namespace_count(), 0);
            for lookup in variants {
                assert!(server.of(lookup).is_none());
            }
        }
    }
}

#[tokio::test]
async fn room_membership_survives_duplicate_join_leave_and_cleanup() {
    let server = WsIoServer::builder().build().unwrap();
    let first_namespace = server.new_namespace_builder("/first").register().unwrap();
    let second_namespace = server.new_namespace_builder("/second").register().unwrap();
    let (first, _first_messages) = ready_connection(&first_namespace).await;
    let (second, _second_messages) = ready_connection(&first_namespace).await;
    let (third, _third_messages) = ready_connection(&second_namespace).await;
    let only_second = [second.id()].into_iter().collect::<RoaringTreemap>();

    first.join(["shared", "shared", "solo"]);
    second.join(["shared"]);
    third.join(["shared"]);
    assert_eq!(first_namespace.room_connection_count("shared"), 2);
    assert_eq!(second_namespace.room_connection_count("shared"), 1);
    assert_eq!(server.room_connection_count("shared"), 3);
    assert_eq!(server.room_connection_count("solo"), 1);
    assert_eq!(server.room_connection_count("missing"), 0);

    first.leave(["shared", "shared", "missing"]);
    assert_eq!(*first_namespace.rooms.get("shared").unwrap(), only_second);
    assert_eq!(server.room_connection_count("shared"), 2);
    first.leave(["solo"]);
    assert!(!first_namespace.rooms.contains_key("solo"));

    // Joins after close must not survive cleanup.
    first.join(["shared", "solo"]);
    first.close();
    first.join(["late"]);
    first.cleanup().await;
    assert!(!first_namespace.rooms.contains_key("late"));
    assert!(!first_namespace.rooms.contains_key("solo"));
    assert_eq!(*first_namespace.rooms.get("shared").unwrap(), only_second);
    assert_eq!(first_namespace.connection_count(), 1);

    second.cleanup().await;
    assert!(first_namespace.rooms.is_empty());
    assert_eq!(server.room_connection_count("shared"), 1);
    third.cleanup().await;
    assert_eq!(server.room_connection_count("shared"), 0);
    assert_eq!(server.connection_count(), 0);
}
