use super::*;

#[test]
fn strips_hop_by_hop_and_cors_headers() {
    assert!(is_stripped_header("connection"));
    assert!(is_stripped_header("keep-alive"));
    assert!(is_stripped_header("transfer-encoding"));
    assert!(is_stripped_header("access-control-allow-origin"));
    assert!(is_stripped_header("access-control-allow-methods"));
    assert!(is_stripped_header("access-control-allow-headers"));
}

#[test]
fn keeps_content_headers() {
    assert!(!is_stripped_header("content-type"));
    assert!(!is_stripped_header("content-length"));
    assert!(!is_stripped_header("content-range"));
    assert!(!is_stripped_header("x-custom"));
}
