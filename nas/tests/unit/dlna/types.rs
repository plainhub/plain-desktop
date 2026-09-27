//! Unit tests for `src/dlna/types.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

#[test]
fn media_type_str_round_trips() {
    for mt in [MediaType::Audio, MediaType::Video, MediaType::Image] {
        assert_eq!(mt.as_str(), format!("{mt}"));
    }
}

#[test]
fn discovered_device_to_renderer_requires_udn_and_name() {
    let mut d = DiscoveredDevice::default();
    assert!(d.to_renderer().is_none(), "empty fields should reject");

    d.udn = "uuid:abc".to_string();
    assert!(d.to_renderer().is_none(), "name still empty");

    d.friendly_name = "Living Room TV".to_string();
    let r = d.to_renderer().expect("now valid");
    assert_eq!(r.udn, "uuid:abc");
    assert_eq!(r.name, "Living Room TV");
}

#[test]
fn to_renderer_trims_whitespace() {
    let mut d = DiscoveredDevice::default();
    d.udn = "  ".to_string();
    d.friendly_name = "  ".to_string();
    assert!(d.to_renderer().is_none());
}
