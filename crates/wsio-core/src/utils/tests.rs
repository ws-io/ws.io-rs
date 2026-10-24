use super::normalize_path;

#[test]
fn path_normalization_preserves_segments_and_is_idempotent() {
    for (input, expected) in [
        ("", "/"),
        ("/", "/"),
        ("///", "/"),
        ("test", "/test"),
        ("/test/", "/test"),
        ("//a///b//", "/a/b"),
        ("/世界/", "/世界"),
        ("/a%2Fb/", "/a%2Fb"),
        ("/ a /", "/ a "),
    ] {
        assert_eq!(normalize_path(input), expected, "{input:?}");
        assert_eq!(normalize_path(expected), expected, "{expected:?}");
    }
}
