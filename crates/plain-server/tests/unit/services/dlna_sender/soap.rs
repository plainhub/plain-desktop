//! Unit tests for `src/dlna/soap.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

fn dev() -> DiscoveredDevice {
    let mut d = DiscoveredDevice::default();
    d.location = "http://192.168.1.10:49152/desc.xml".to_string();
    d.av_transport.service_type = "urn:schemas-upnp-org:service:AVTransport:1".to_string();
    d.av_transport.control_url = "/ctl/AVTransport".to_string();
    d
}

#[test]
fn resolve_relative_against_location() {
    // The `url` crate elides default ports (`:80` for http, `:443` for
    // https). Go's `net/url` keeps them. The two are semantically
    // equivalent; we just assert the Rust behaviour.
    let url = resolve_service_endpoint("http://1.2.3.4:80/d.xml", "/ctl/AVT").unwrap();
    assert_eq!(url, "http://1.2.3.4/ctl/AVT");
}

#[test]
fn resolve_absolute_control_url_unchanged() {
    let url = resolve_service_endpoint("http://1.2.3.4/d.xml", "http://9.9.9.9/ctl").unwrap();
    assert_eq!(url, "http://9.9.9.9/ctl");
}

#[test]
fn resolve_rejects_empty_control() {
    let err = resolve_service_endpoint("http://1.2.3.4/d.xml", "  ").unwrap_err();
    assert!(err.contains("empty controlURL"));
}

#[test]
fn resolve_rejects_invalid_location() {
    let err = resolve_service_endpoint("not a url", "/x").unwrap_err();
    assert!(err.contains("parse location"));
}

#[test]
fn dev_returns_service_type() {
    // Indirect smoke test that the helper struct shapes match what
    // the SOAP builder expects.
    let d = dev();
    assert!(d.av_transport.service_type.contains("AVTransport"));
}
