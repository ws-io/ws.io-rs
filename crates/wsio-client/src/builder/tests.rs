use super::*;

#[test]
fn builder_url_carries_normalized_namespace_and_request_path() {
    for (path, expected) in [("/", "/"), ("//a///b//", "/a/b")] {
        let builder = WsIoClient::builder(format!("ws://localhost{path}?namespace=ignored&token=a%26b"))
            .unwrap()
            .request_path("/transport/")
            .unwrap();

        let query_value = |name: &str| {
            builder
                .connect_url
                .query_pairs()
                .filter(|(key, _)| key == name)
                .map(|(_, value)| value.into_owned())
                .collect::<Vec<_>>()
        };

        assert_eq!(builder.connect_url.path(), "/transport");
        assert_eq!(query_value("namespace"), [expected], "{path:?}");
        assert_eq!(query_value("token"), ["a&b"]);
    }
}

#[test]
fn builder_rejects_namespace_outside_whitelist() {
    for path in ["/世界", "/a%20b", "/a+b"] {
        assert!(
            WsIoClient::builder(format!("ws://localhost{path}")).is_err(),
            "{path:?}"
        );
    }
}

#[test]
fn request_path_rejects_invalid_input_before_url_rewriting() {
    for path in ["/世界", "/a%20b", "/a/../b", "/a\\b", "/a\tb"] {
        assert!(
            WsIoClient::builder("ws://localhost/test")
                .unwrap()
                .request_path(path)
                .is_err(),
            "{path:?}"
        );
    }
}
