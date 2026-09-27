//! Fetch and parse UPnP device description XML.
//!
//! Mirrors `internal/dlna/desc.go`. We pull the XML with `ureq` (already
//! in the dep tree for the install command) using a 2s per-request
//! timeout, then extract the fields we care about with the in-tree
//! `crate::xml_sax` parser (replaces the `quick-xml` crate we used
//! to depend on).
//!
//! Note: the Go side deliberately **does not** bind the caller's context
//! to the HTTP request — discovery can run under a short context budget
//! (e.g. UI-triggered polling), and relying on `http.Client` timeouts
//! makes description fetches more reliable. We follow the same pattern.

use std::time::Duration;

use crate::nas::xml_sax::{Event, Reader};

use super::ssdp::ssdp_search;
use super::types::{DiscoveredDevice, UpnpService};
use super::util::{dlna_debug_enabled, dlna_debug_payload_enabled, truncate_for_log};

/// Per-request HTTP timeout for the device-descriptor fetch (Go uses 2s).
const FETCH_TIMEOUT: Duration = Duration::from_secs(2);
/// Hard cap on the descriptor body size (2 MiB). Real descriptors are <8
/// KiB, but some weird firmwares stuff a lot in there.
const FETCH_BODY_CAP: usize = 2 << 20;

/// Fetch the XML at `location` and parse it into a `DiscoveredDevice`.
/// Returns an error string on network failure, non-2xx status, or XML
/// errors. The caller's context is **not** honoured — see module docs.
pub fn fetch_and_parse_device(location: &str) -> Result<DiscoveredDevice, String> {
    let started = std::time::Instant::now();
    let agent = ureq::AgentBuilder::new().timeout(FETCH_TIMEOUT).build();
    let resp = match agent.get(location).call() {
        Ok(r) => r,
        Err(e) => return Err(format!("fetch: {e}")),
    };
    let status = resp.status();
    if !(200..300).contains(&status) {
        return Err(format!("device description http {status}"));
    }
    let mut body = Vec::new();
    use std::io::Read;
    let reader = resp.into_reader();
    let mut limited = reader.take(FETCH_BODY_CAP as u64);
    if let Err(e) = limited.read_to_end(&mut body) {
        return Err(format!("read body: {e}"));
    }
    if dlna_debug_enabled() {
        log::info!(
            "desc: ok location={location} status={status} bytes={} dur_ms={}",
            body.len(),
            started.elapsed().as_millis() as u64
        );
        if dlna_debug_payload_enabled() {
            let s = String::from_utf8_lossy(&body);
            log::debug!("desc: body\n{}", truncate_for_log(&s, 4096));
        }
    }
    let mut dev = parse_device_desc(&body);
    if dev.friendly_name.trim().is_empty() {
        dev.friendly_name = dev.udn.trim().to_string();
    }
    dev.location = location.to_string();
    Ok(dev)
}

/// Parse the inner `<device>` element of a UPnP device-description XML
/// document. We extract the fields the Go side extracts — no
/// `presentationURL`, no `iconList`. The caller is expected to attach
/// `Location` and to fall back to UDN-from-USN if `udn` is empty.
pub fn parse_device_desc(body: &[u8]) -> DiscoveredDevice {
    let mut dev = DiscoveredDevice::default();
    let mut r = Reader::new(body);

    // State: which `<X>` we are currently inside. We descend into
    // `<device>...</device>` and collect its immediate children's text.
    let mut path: Vec<String> = Vec::new();
    // When we see `<service>` (only meaningful inside `<serviceList>`), we
    // start collecting into a local UpnpService.
    let mut in_service = false;
    let mut cur_service = UpnpService::default();

    while let Some(ev) = r.next_event() {
        match ev {
            Event::Start { name } => {
                if name == "service" {
                    in_service = true;
                    cur_service = UpnpService::default();
                } else {
                    path.push(name);
                }
            }
            Event::End { name } => {
                if name == "service" {
                    if cur_service.service_type.contains("AVTransport:1")
                        || cur_service.service_id.contains("AVTransport")
                    {
                        dev.has_av_transport = true;
                        dev.av_transport = UpnpService {
                            service_type: cur_service.service_type.trim().to_string(),
                            service_id: cur_service.service_id.trim().to_string(),
                            control_url: cur_service.control_url.trim().to_string(),
                            event_sub_url: cur_service.event_sub_url.trim().to_string(),
                            scpd_url: cur_service.scpd_url.trim().to_string(),
                        };
                    }
                    in_service = false;
                } else {
                    path.pop();
                }
            }
            Event::Text { content } => {
                let field = path.last().cloned().unwrap_or_default();
                if in_service {
                    match field.as_str() {
                        "serviceType" => cur_service.service_type = content,
                        "serviceId" => cur_service.service_id = content,
                        "controlURL" => cur_service.control_url = content,
                        "eventSubURL" => cur_service.event_sub_url = content,
                        "SCPDURL" => cur_service.scpd_url = content,
                        _ => {}
                    }
                } else if path.iter().any(|p| p == "device") {
                    match field.as_str() {
                        "friendlyName" => dev.friendly_name = content,
                        "manufacturer" => dev.manufacturer = content,
                        "modelName" => dev.model_name = content,
                        "UDN" => dev.udn = content,
                        _ => {}
                    }
                }
            }
        }
    }
    dev
}

/// Resolve an SSDP response list into a deduplicated set of fully-parsed
/// devices by fetching each unique `Location`. If `on_device` is `Some`,
/// it is invoked once for every device we successfully parse (including
/// duplicates suppressed by `Location`).
pub fn discover_upnp_devices<F: FnMut(&DiscoveredDevice)>(
    search_targets: &[String],
    mut on_device: Option<F>,
) -> Vec<DiscoveredDevice> {
    let responses = ssdp_search(search_targets);
    if dlna_debug_enabled() {
        log::info!("ssdp: got unique responses count={}", responses.len());
    }
    let mut by_location: std::collections::HashMap<String, DiscoveredDevice> =
        std::collections::HashMap::new();
    for r in &responses {
        let loc = r.location.trim().to_string();
        if loc.is_empty() {
            continue;
        }
        if by_location.contains_key(&loc) {
            continue;
        }
        if dlna_debug_enabled() {
            log::info!("desc: fetching location={loc} usn={}", r.usn);
        }
        let mut d = match fetch_and_parse_device(&loc) {
            Ok(d) => d,
            Err(e) => {
                log::info!("desc: fetch failed location={loc} err={e}");
                continue;
            }
        };
        if d.udn.trim().is_empty() {
            d.udn = super::ssdp::parse_udn_from_usn(&r.usn);
        }
        if d.udn.trim().is_empty() {
            continue;
        }
        if let Some(cb) = on_device.as_mut() {
            cb(&d);
        }
        by_location.insert(loc, d);
    }
    by_location.into_values().collect()
}

/// Resolve a UDN to a `DiscoveredDevice`. Hits the in-memory cache first
/// (populated by previous discovery rounds); on miss, runs a fresh SSDP
/// scan. Mirrors Go `findUPnPDeviceByUDN(ctx, udn)`.
pub fn find_upnp_device_by_udn(udn: &str) -> Result<DiscoveredDevice, String> {
    let udn = udn.trim();
    if udn.is_empty() {
        return Err("udn is empty".to_string());
    }
    if let Some(d) = crate::nas::dlna::discovery::get_cached_by_udn(udn) {
        return Ok(d);
    }
    let devs = discover_upnp_devices::<fn(&DiscoveredDevice)>(&["ssdp:all".to_string()], None);
    for d in devs {
        if d.udn == udn {
            // Populate the cache for next time.
            crate::nas::dlna::discovery::put_cache(d.clone());
            return Ok(d);
        }
    }
    Err("renderer not found".to_string())
}

#[cfg(test)]
#[path = "../../../tests/unit/nas/dlna/desc.rs"]
mod tests;
