//! Unit tests for the CORS policy (moved from plain-nas).
use super::*;
use crate::media::config::Config;

#[test]
fn default_policy_accepts_localhost_loopback() {
    let cfg = Config::parse("[server]\nhttp_port = 8080\nhttps_port = 8443\n");
    let p = CorsPolicy::from_config(&cfg);
    assert!(
        p.is_wildcard()
            || p.allowed_snapshot()
                .iter()
                .any(|o| o == "http://localhost:8080")
    );
    assert!(
        p.is_wildcard()
            || p.allowed_snapshot()
                .iter()
                .any(|o| o == "http://127.0.0.1:8080")
    );
    assert!(
        p.is_wildcard()
            || p.allowed_snapshot()
                .iter()
                .any(|o| o == "https://localhost:8443")
    );
    assert!(
        p.is_wildcard()
            || p.allowed_snapshot()
                .iter()
                .any(|o| o == "https://127.0.0.1:8443")
    );
}

#[test]
fn explicit_allowlist_overrides_defaults() {
    let cfg = Config::parse("server.allowed_origins = https://a.example,https://b.example\n");
    let p = CorsPolicy::from_config(&cfg);
    assert!(
        p.allowed_snapshot()
            .iter()
            .any(|o| o == "https://a.example")
    );
    assert!(
        p.allowed_snapshot()
            .iter()
            .any(|o| o == "https://b.example")
    );
    // The loopback defaults are NOT added when the user has
    // configured an explicit list — we honour the operator's intent.
    assert!(
        !p.allowed_snapshot()
            .iter()
            .any(|o| o == "http://localhost:8080")
    );
}

#[test]
fn wildcard_mode_accepts_everything() {
    let cfg = Config::parse("server.allow_wildcard_cors = true\n");
    let p = CorsPolicy::from_config(&cfg);
    assert!(p.is_wildcard());
}
