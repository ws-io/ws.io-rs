use super::*;

#[test]
fn request_path_is_resolved_before_building() {
    for (input, expected) in [("", "/"), ("///", "/"), ("custom//ws/", "/custom/ws")] {
        let builder = WsIoServer::builder().request_path(input).unwrap();
        assert_eq!(builder.config.request_path, expected, "{input:?}");
    }

    for path in ["/世界", "/a%20b", "/a/../b", "/a\\b", "/a\tb"] {
        assert!(WsIoServer::builder().request_path(path).is_err(), "{path:?}");
    }
}
