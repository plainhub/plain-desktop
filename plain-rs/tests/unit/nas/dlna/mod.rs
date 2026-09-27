//! Unit tests for `src/dlna/mod.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;
use std::collections::HashMap;

#[test]
fn renderer_payload_has_all_fields() {
    let mut d = DiscoveredDevice::default();
    d.udn = "uuid:x".into();
    d.friendly_name = "TV".into();
    d.manufacturer = "Acme".into();
    d.model_name = "M1".into();
    d.location = "http://1.2.3.4/d.xml".into();
    let v = renderer_payload(&d);
    assert_eq!(v["udn"], "uuid:x");
    assert_eq!(v["name"], "TV");
    assert_eq!(v["manufacturer"], "Acme");
    assert_eq!(v["modelName"], "M1");
    assert_eq!(v["location"], "http://1.2.3.4/d.xml");
}

#[test]
fn discovery_done_payload_marks_done() {
    let v = discovery_done_payload();
    assert_eq!(v["done"], true);
    // Make sure no extra fields sneak in.
    let map: HashMap<String, _> = v
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert_eq!(map.len(), 1);
}
