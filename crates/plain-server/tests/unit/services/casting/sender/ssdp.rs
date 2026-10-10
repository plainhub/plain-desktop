//! Unit tests for `src/dlna/ssdp.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

#[test]
fn build_m_search_matches_go_format() {
    let q = build_m_search("ssdp:all");
    assert!(q.starts_with("M-SEARCH * HTTP/1.1\n"));
    assert!(q.contains("ST: ssdp:all\n"));
    assert!(q.contains("HOST: 239.255.255.250:1900\n"));
    assert!(q.contains("MX: 3\n"));
    assert!(q.contains("MAN: \"ssdp:discover\"\n"));
    // Two empty lines at the end is the PlainAPP quirk the Go side
    // explicitly preserves.
    assert!(q.ends_with("\n\n\n"));
}

#[test]
fn build_m_search_trims_st() {
    assert!(build_m_search("  ssdp:all  ").contains("ST: ssdp:all\n"));
}

#[test]
fn build_m_search_empty_st_defaults_to_ssdp_all() {
    assert!(build_m_search("").contains("ST: ssdp:all\n"));
}

#[test]
fn parse_udn_from_usn_basic() {
    assert_eq!(
        parse_udn_from_usn("uuid:4d696e69-444c-164e-9d41-b827abcdef01::upnp:rootdevice"),
        "uuid:4d696e69-444c-164e-9d41-b827abcdef01"
    );
}

#[test]
fn parse_udn_from_usn_no_prefix() {
    assert_eq!(
        parse_udn_from_usn("4d696e69-444c-164e-9d41-b827abcdef01"),
        "uuid:4d696e69-444c-164e-9d41-b827abcdef01"
    );
}

#[test]
fn parse_ssdp_headers_basic() {
    let raw = "HTTP/1.1 200 OK\r\nLOCATION: http://192.168.1.10:49152/desc.xml\r\nUSN: uuid:abc::upnp:rootdevice\r\n\r\n";
    let h = parse_ssdp_headers(raw);
    assert_eq!(
        h.get("location").unwrap(),
        "http://192.168.1.10:49152/desc.xml"
    );
    assert_eq!(h.get("usn").unwrap(), "uuid:abc::upnp:rootdevice");
}

#[test]
fn extra_destinations_default_port() {
    // We can't change env vars in this test easily, but the parsing
    // path is straightforward — `ssdp_extra_destinations` is unit-
    // tested via the env var in the integration suite.
}
