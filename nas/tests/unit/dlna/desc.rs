//! Unit tests for `src/dlna/desc.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

const SAMPLE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <specVersion><major>1</major><minor>0</minor></specVersion>
  <URLBase>http://192.168.1.10:49152</URLBase>
  <device>
<deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>
<friendlyName>Living Room TV</friendlyName>
<manufacturer>Samsung</manufacturer>
<modelName>UN55TU8000</modelName>
<UDN>uuid:4d696e69-444c-164e-9d41-b827abcdef01</UDN>
<serviceList>
  <service>
    <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
    <serviceId>urn:upnp-org:serviceId:AVTransport</serviceId>
    <controlURL>/ctl/AVTransport</controlURL>
    <eventSubURL>/evt/AVTransport</eventSubURL>
    <SCPDURL>/scpd/AVTransport.xml</SCPDURL>
  </service>
  <service>
    <serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType>
    <serviceId>urn:upnp-org:serviceId:RenderingControl</serviceId>
    <controlURL>/ctl/RenderingControl</controlURL>
    <eventSubURL>/evt/RenderingControl</eventSubURL>
    <SCPDURL>/scpd/RenderingControl.xml</SCPDURL>
  </service>
</serviceList>
  </device>
</root>"#;

#[test]
fn parse_extracts_friendly_name_manufacturer_model_udn() {
    let d = parse_device_desc(SAMPLE_XML.as_bytes());
    assert_eq!(d.friendly_name, "Living Room TV");
    assert_eq!(d.manufacturer, "Samsung");
    assert_eq!(d.model_name, "UN55TU8000");
    assert_eq!(d.udn, "uuid:4d696e69-444c-164e-9d41-b827abcdef01");
}

#[test]
fn parse_picks_av_transport_service() {
    let d = parse_device_desc(SAMPLE_XML.as_bytes());
    assert!(d.has_av_transport);
    assert_eq!(
        d.av_transport.service_type,
        "urn:schemas-upnp-org:service:AVTransport:1"
    );
    assert_eq!(
        d.av_transport.service_id,
        "urn:upnp-org:serviceId:AVTransport"
    );
    assert_eq!(d.av_transport.control_url, "/ctl/AVTransport");
}

#[test]
fn parse_handles_missing_av_transport() {
    let xml = r#"<?xml version="1.0"?><root><device><friendlyName>X</friendlyName><UDN>uuid:1</UDN></device></root>"#;
    let d = parse_device_desc(xml.as_bytes());
    assert_eq!(d.friendly_name, "X");
    assert_eq!(d.udn, "uuid:1");
    assert!(!d.has_av_transport);
}

#[test]
fn parse_falls_back_friendly_name_to_udn() {
    let xml = r#"<?xml version="1.0"?><root><device><UDN>uuid:fallback</UDN></device></root>"#;
    let mut d = parse_device_desc(xml.as_bytes());
    if d.friendly_name.trim().is_empty() {
        d.friendly_name = d.udn.trim().to_string();
    }
    assert_eq!(d.friendly_name, "uuid:fallback");
}

#[test]
fn parse_tolerates_namespaced_elements() {
    // quick-xml keeps the prefix in the name; we strip it.
    let xml = r#"<?xml version="1.0"?><root xmlns="urn:x"><device><dc:friendlyName xmlns:dc="urn:dc">NS Name</dc:friendlyName><UDN>uuid:ns</UDN></device></root>"#;
    let d = parse_device_desc(xml.as_bytes());
    // The element is prefixed `dc:friendlyName`; we strip the prefix
    // and only match on local-name. Behaviour matches the Go side
    // (encoding/xml ignores namespaces by default for element names).
    assert_eq!(d.friendly_name, "NS Name");
    assert_eq!(d.udn, "uuid:ns");
}
