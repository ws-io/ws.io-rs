use super::*;

#[test]
fn path_resolution_normalizes_valid_names_and_is_idempotent() {
    for (input, expected) in [
        ("", "/"),
        ("/", "/"),
        ("///", "/"),
        ("test", "/test"),
        ("/test/", "/test"),
        ("//a///b//", "/a/b"),
        ("/a-b_c.d/E9/", "/a-b_c.d/E9"),
        ("/v1.0", "/v1.0"),
    ] {
        assert_eq!(resolve_path(input).unwrap(), expected, "{input:?}");
        assert_eq!(resolve_path(expected).unwrap(), expected, "{expected:?}");
    }
}

#[test]
fn path_resolution_rejects_unsupported_characters_and_dot_segments() {
    for path in [
        "/世界", "/a b", "/a%2Fb", "/a\\b", "/a+b", "/.", "/a/..", "/a/./b", "/a\tb", "/a\nb",
    ] {
        assert!(resolve_path(path).is_err(), "{path:?}");
    }
}
