use super::*;

#[test]
fn custom_bearer_requires_the_exact_second_header_token_and_a_custom_session() {
    assert!(valid_custom_bearer("Bearer secret", "secret"));
    assert!(!valid_custom_bearer("Bearer wrong", "secret"));
    assert!(!valid_custom_bearer("Bearer secret", ""));
    assert!(!valid_custom_bearer("Bearer", "secret"));
    assert!(!valid_custom_bearer("secret", "secret"));
    assert!(!valid_custom_bearer("Bearer  secret", "secret"));
}

#[test]
fn shutdown_only_accepts_ipv4_and_ipv6_loopback_peers() {
    assert!(shutdown_allowed("127.0.0.1:443".parse().unwrap()));
    assert!(shutdown_allowed("[::1]:443".parse().unwrap()));
    assert!(!shutdown_allowed("192.168.1.2:443".parse().unwrap()));
    assert!(!shutdown_allowed("[::ffff:127.0.0.1]:443".parse().unwrap()));
}
