use super::*;

#[test]
fn timestamp_ok_within_5min() {
    let now = now_ms();
    assert!(timestamp_ok(now));
    assert!(timestamp_ok(now - 4 * 60 * 1000));
    assert!(timestamp_ok(now + 4 * 60 * 1000));
    assert!(!timestamp_ok(now - 6 * 60 * 1000));
    assert!(!timestamp_ok(now + 6 * 60 * 1000));
}

#[test]
fn prefer_sender_ip_puts_sender_first_and_dedupes() {
    let ips = vec![
        "10.0.0.2".to_string(),
        "10.0.0.2".to_string(),
        "".to_string(),
    ];
    assert_eq!(prefer_sender_ip(&ips, "10.0.0.3"), "10.0.0.3,10.0.0.2");
    // Sender already in the list → listed once, first.
    assert_eq!(prefer_sender_ip(&ips, "10.0.0.2"), "10.0.0.2");
    assert_eq!(prefer_sender_ip(&[], ""), "");
}

#[test]
fn device_type_signature_value_passes_through() {
    assert_eq!(device_type_signature_value("COMPUTER"), "COMPUTER");
    assert_eq!(device_type_signature_value("NAS"), "NAS");
}
