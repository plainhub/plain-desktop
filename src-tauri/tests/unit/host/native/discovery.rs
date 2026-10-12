use super::*;
use plain_rs::chat::enums::DeviceType;

#[test]
fn the_desktop_announces_itself_as_a_computer_under_the_given_name() {
    let facts = discovery_facts("Study Desk".to_string(), "4.0.0".to_string())
        .expect("reading this host's own facts must not fail");

    assert_eq!(facts.name, "Study Desk");
    assert_eq!(facts.version, "4.0.0");
    assert_eq!(facts.device_type, DeviceType::Computer);
}

#[test]
fn the_platform_names_the_operating_system_this_binary_ran_on() {
    let facts = discovery_facts("Study Desk".to_string(), "4.0.0".to_string())
        .expect("reading this host's own facts must not fail");

    assert_eq!(facts.platform, std::env::consts::OS.to_uppercase());
    assert!(facts.platform.chars().all(|c| c.is_ascii_uppercase()));
}

#[test]
fn every_advertised_address_is_a_real_local_ipv4() {
    let facts = discovery_facts("Study Desk".to_string(), "4.0.0".to_string())
        .expect("reading this host's own facts must not fail");

    for ip in &facts.ips {
        assert!(
            ip.parse::<std::net::Ipv4Addr>().is_ok(),
            "not an IPv4 address: {ip}"
        );
    }
}

#[test]
fn wifi_aware_is_reported_absent_because_the_desktop_has_no_adapter() {
    let facts = discovery_facts("Study Desk".to_string(), "4.0.0".to_string())
        .expect("reading this host's own facts must not fail");

    assert!(!facts.aware_supported);
    assert!(!facts.aware_running);
}
