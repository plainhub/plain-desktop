//! Type definitions shared across the DLNA module.
//!
//! Public surface mirrors Go's `internal/dlna/types.go`. The public
//! `Renderer` is what the GraphQL `DlnaRenderer` resolver returns; the
//! internal `DiscoveredDevice` / `UpnpDiscovered` structs are intermediates
//! produced during SSDP scanning and device-descriptor parsing.

use serde::Serialize;

/// One media renderer we know about (a TV, a stereo, etc.).
#[derive(Clone, Debug, Serialize)]
pub struct Renderer {
    pub udn: String,
    pub name: String,
    pub manufacturer: String,
    pub model_name: String,
    pub location: String,
}

/// The kind of media being cast. Maps to the DIDL-Lite `upnp:class` value.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum MediaType {
    Audio,
    Video,
    Image,
}

impl MediaType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaType::Audio => "AUDIO",
            MediaType::Video => "VIDEO",
            MediaType::Image => "IMAGE",
        }
    }
}

impl std::fmt::Display for MediaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Raw SSDP response — what we receive off the multicast socket before we
/// have fetched and parsed the device descriptor.
#[derive(Clone, Debug)]
pub struct UpnpDiscovered {
    pub location: String,
    pub usn: String,
}

/// One UPnP service entry parsed from a device description XML document.
#[derive(Clone, Debug, Default)]
pub struct UpnpService {
    pub service_type: String,
    pub service_id: String,
    pub control_url: String,
    pub event_sub_url: String,
    pub scpd_url: String,
}

/// A device that has been SSDP-discovered AND whose descriptor has been
/// fetched + parsed. We keep the AVTransport control URL around so we can
/// issue SOAP calls without re-fetching the descriptor every time.
#[derive(Clone, Debug, Default)]
pub struct DiscoveredDevice {
    pub udn: String,
    pub friendly_name: String,
    pub manufacturer: String,
    pub model_name: String,
    pub location: String,
    pub has_av_transport: bool,
    pub av_transport: UpnpService,
}

impl DiscoveredDevice {
    /// Project down to the public `Renderer` shape that goes to the wire
    /// (GraphQL `DlnaRenderer`). Returns `None` if the device is missing
    /// fields the resolver needs.
    pub fn to_renderer(&self) -> Option<Renderer> {
        if self.udn.trim().is_empty() || self.friendly_name.trim().is_empty() {
            return None;
        }
        Some(Renderer {
            udn: self.udn.clone(),
            name: self.friendly_name.clone(),
            manufacturer: self.manufacturer.clone(),
            model_name: self.model_name.clone(),
            location: self.location.clone(),
        })
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/nas/dlna/types.rs"]
mod tests;
