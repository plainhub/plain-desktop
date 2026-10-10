use super::*;

#[test]
fn only_http_peer_urls_can_be_proxied() {
    assert_eq!(
        target_url("http://192.168.1.9:8080/fs?id=1").unwrap(),
        "http://192.168.1.9:8080/fs?id=1"
    );
    assert_eq!(
        target_url("  https://peer.local/fs  ").unwrap(),
        "https://peer.local/fs"
    );
    // The Kotlin original used `startsWith("http")`, which also accepted these.
    assert!(target_url("httpfoo://peer/fs").is_err());
    assert!(target_url("file:///etc/passwd").is_err());
    assert!(target_url("ftp://peer/fs").is_err());
    assert!(target_url("http://").is_err());
    assert!(target_url("").is_err());
}

#[test]
fn hop_by_hop_and_reencoded_headers_are_not_proxied() {
    for name in SKIPPED {
        assert!(
            [
                "connection",
                "transfer-encoding",
                "upgrade",
                "content-encoding",
                "content-length"
            ]
            .contains(name),
            "{name} must stay out of the proxied response"
        );
    }
    assert!(!SKIPPED.contains(&"content-type"));
    assert!(!SKIPPED.contains(&"etag"));
}
