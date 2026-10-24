use super::*;

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

#[test]
fn namespace_path_validation_allows_only_whitelisted_characters() {
    for path in ["/", "/test", "/a-b_c.d/E9", "/v1.0"] {
        assert!(validate_namespace_path(path).is_ok(), "{path:?}");
    }

    for path in ["/世界", "/a b", "/a%2Fb", "/a\\b", "/a+b", "/.", "/a/..", "/a/./b"] {
        assert!(validate_namespace_path(path).is_err(), "{path:?}");
    }
}
