#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

use wsio_server::WsIoServer;

mod e2e;

#[test]
fn test_multiple_namespaces() {
    let server = WsIoServer::builder().build();

    server.new_namespace_builder("/namespace1").register().unwrap();
    server.new_namespace_builder("/namespace2").register().unwrap();

    assert_eq!(server.namespace_count(), 2);
}

#[tokio::test]
async fn test_remove_namespace() {
    let server = WsIoServer::builder().build();

    server.new_namespace_builder("/test").register().unwrap();
    assert_eq!(server.namespace_count(), 1);
    assert_eq!(server.of("/test").unwrap().path(), "/test");

    server.remove_namespace("/test").await;

    assert_eq!(server.namespace_count(), 0);
    assert!(server.of("/test").is_none());
}
