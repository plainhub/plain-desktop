use super::*;

#[test]
fn short_id_is_16_hex_and_unique() {
    let a = short_id();
    let b = short_id();
    assert_eq!(a.len(), 16);
    assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(a, b);
}

#[test]
fn iso_helpers_format_utc() {
    // 2026-01-02T03:04:05Z = 20455 days * 86400 + 3*3600 + 4*60 + 5 seconds.
    assert_eq!(
        iso_from_unix_millis(1_767_323_045_000),
        "2026-01-02T03:04:05Z"
    );
    // Negative/zero clamps to epoch.
    assert_eq!(iso_from_unix_millis(0), "1970-01-01T00:00:00Z");
    assert_eq!(iso_from_unix_millis(-5), "1970-01-01T00:00:00Z");
    let now = now_iso();
    assert!(now.ends_with('Z') && now.len() == 20, "now_iso={now}");
}
