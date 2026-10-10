use super::*;
#[test]
fn prefers_matching_subnet_then_private_and_rejects_invalid_ip_prefixes() {
    let ips = ["bad", "203.0.113.1", "192.168.3.4", "10.1.2.3"].map(str::to_string);
    assert_eq!(
        best(
            &ips,
            &[Interface {
                ip: "10.1.2.9".parse().unwrap(),
                prefix_length: 24
            }]
        ),
        "10.1.2.3"
    );
    assert_eq!(
        best(
            &ips,
            &[Interface {
                ip: "10.1.2.9".parse().unwrap(),
                prefix_length: 33
            }]
        ),
        "192.168.3.4"
    );
    assert_eq!(best(&["::1".into()], &[]), "::1");
    assert_eq!(best(&["999.1.2.3".into()], &[]), "");
    assert_eq!(best(&[], &[]), "");
    assert_eq!(
        best(
            &ips,
            &[Interface {
                ip: "10.1.2.9".parse().unwrap(),
                prefix_length: 0
            }]
        ),
        "203.0.113.1"
    );
}

#[test]
fn non24_masks_and_multihomed_selection_follow_actual_prefixes() {
    let ips = ["192.168.1.4", "10.5.31.8", "10.5.8.9"].map(str::to_string);
    assert_eq!(
        best(
            &ips,
            &[Interface {
                ip: "10.5.16.2".parse().unwrap(),
                prefix_length: 20
            }]
        ),
        "10.5.31.8"
    );
    assert_eq!(
        best(
            &ips,
            &[Interface {
                ip: "10.5.8.9".parse().unwrap(),
                prefix_length: 32
            }]
        ),
        "10.5.8.9"
    );
    assert_eq!(
        best(
            &ips,
            &[Interface {
                ip: "10.5.2.9".parse().unwrap(),
                prefix_length: 16
            }]
        ),
        "10.5.31.8"
    );
    assert!(
        local_interfaces()
            .iter()
            .all(|p| p.prefix_length <= 32 && !p.ip.is_loopback())
    );
}
