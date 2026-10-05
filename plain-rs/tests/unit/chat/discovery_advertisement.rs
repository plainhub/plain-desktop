use super::*;
fn facts() -> Facts {
    Facts {
        name: "Actual tablet".into(),
        device_type: DeviceType::Tablet,
        version: "3.3.24".into(),
        platform: "IOS".into(),
        ips: vec!["192.168.7.2".into()],
        aware_supported: true,
        aware_running: false,
    }
}
#[test]
fn same_identity_and_os_facts_assemble_all_advertisements() {
    let reply = reply("fixture", "中文.Device", 2443, facts()).unwrap();
    let service = mdns(&reply, "fixture.local").unwrap();
    assert_eq!(service.instance_name, "中文.Device");
    assert_eq!(service.service_type, "_plainapp._tcp.local");
    assert_eq!(service.target_hostname, "fixture.local");
    assert_eq!(service.port, 2443);
    assert_eq!(service.ips, reply.ips);
    assert_eq!(
        service.txt_records,
        vec![
            "id=fixture",
            "dv=TABLET",
            "ver=3.3.24",
            "pf=IOS",
            "aw=1",
            "ar=0"
        ]
    );
    assert_eq!(
        super::reply("fixture", "", 2443, facts()).unwrap().name,
        "Actual tablet"
    );
    assert!(super::reply("", "", 2443, facts()).is_err());
    let mut no_port = reply.clone();
    no_port.port = 0;
    assert!(mdns(&no_port, "fixture.local").is_err());
    assert!(mdns(&reply, "").is_err());
}
#[test]
fn compact_ble_payload_preserves_all_flags_and_existing_sha256_short_id() {
    for (supported, running, flags) in [
        (false, false, 0),
        (true, false, 1),
        (false, true, 2),
        (true, true, 3),
    ] {
        let payload = ble("fixture", supported, running).unwrap();
        assert_eq!(payload.len(), 9);
        assert_eq!(payload[0], flags);
        assert_eq!(
            crate::utils::hex::bytes_to_hex(&payload[1..]),
            super::super::nearby_wire::short_id("fixture")
        );
        assert_eq!(&payload[1..], &Sha256::digest(b"fixture")[..8]);
    }
    assert!(ble("", true, true).is_err());
}

#[test]
fn ble_scan_header_preserves_wire_bits_and_rejects_truncated_data() {
    let bytes = [0xf3, 0, 1, 2, 3, 4, 128, 254, 255, 42];
    for size in 0..9 {
        assert!(decode_ble(&bytes[..size]).is_none());
    }
    assert_eq!(
        decode_ble(&bytes),
        Some(BleParts {
            short_id: "000102030480feff".into(),
            aware_supported: true,
            aware_running: true
        })
    );
    for flags in 0..=255 {
        let mut payload = bytes;
        payload[0] = flags;
        let parsed = decode_ble(&payload).unwrap();
        assert_eq!(parsed.aware_supported, flags & 1 != 0);
        assert_eq!(parsed.aware_running, flags & 2 != 0);
        assert_eq!(parsed.short_id, "000102030480feff");
    }
}
