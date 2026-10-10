//! Unit tests for `src/dlna/discovery.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

fn dev(udn: &str, name: &str) -> DiscoveredDevice {
    let mut d = DiscoveredDevice::default();
    d.udn = udn.to_string();
    d.friendly_name = name.to_string();
    d.has_av_transport = true;
    d
}

#[test]
fn cache_round_trip() {
    let _g = CACHE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    CACHE.write().clear();
    put_cache(dev("uuid:1", "TV1"));
    assert!(get_cached_by_udn("uuid:1").is_some());
    let list = cached_renderers();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "TV1");
}

#[test]
fn put_cache_rejects_empty_udn() {
    let _g = CACHE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    CACHE.write().clear();
    let mut d = DiscoveredDevice::default();
    d.udn = "  ".to_string();
    put_cache(d);
    assert!(cached_renderers().is_empty());
}

#[test]
fn cached_renderers_sorted_by_name_case_insensitive() {
    let _g = CACHE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    CACHE.write().clear();
    put_cache(dev("uuid:a", "Bravo"));
    put_cache(dev("uuid:b", "alpha"));
    put_cache(dev("uuid:c", "Charlie"));
    let names: Vec<String> = cached_renderers().into_iter().map(|r| r.name).collect();
    assert_eq!(names, vec!["alpha", "Bravo", "Charlie"]);
}

#[test]
fn cached_renderers_excludes_devices_without_av_transport() {
    let _g = CACHE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    CACHE.write().clear();
    let mut d = dev("uuid:x", "Bare");
    d.has_av_transport = false;
    put_cache(d);
    assert!(
        cached_renderers().is_empty(),
        "no AVTransport means no public render"
    );
}
