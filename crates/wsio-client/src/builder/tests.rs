use super::*;

#[test]
fn builder_url_carries_normalized_namespace_and_request_path() {
    for (path, expected) in [("/", "/"), ("//a///b//", "/a/b")] {
        let builder = WsIoClient::builder(format!("ws://localhost{path}?namespace=ignored&token=a%26b"))
            .unwrap()
            .request_path("/transport/");

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
