//! DLNA renderer discovery and casting.
//!
//! Mirrors `internal/dlna/` in the Go reference. The module is organised
//! into a few small files for readability:
//!
//!   * [`util`]        — XML escape, log helpers
//!   * [`types`]       — public `Renderer` / `MediaType`, internal
//!     `DiscoveredDevice` / `UpnpDiscovered` / `UpnpService`
//!   * [`ssdp`]        — SSDP M-SEARCH send/response loop
//!   * [`desc`]        — device-description XML fetch + parse
//!   * [`didl`]        — DIDL-Lite metadata builder
//!   * [`soap`]        — SOAP envelope + AVTransport control calls
//!   * [`media_alias`] — short-id registry so TVs can fetch our `/fs`
//!     endpoint by a URL they can reach
//!   * [`discovery`]   — long-running discovery task + cache + event
//!     publishing
//!
//! Public surface (the bits the GraphQL resolvers call) lives in this
//! file:
//!   * [`Renderer`] / [`MediaType`]
//!   * [`discover_renderers`] (one-shot SSDP scan)
//!   * [`cast`] (build DIDL-Lite + send `Stop` / `SetAVTransportURI` /
//!     `Play` to a renderer)
//!   * [`start_renderer_discovery`] / [`cached_renderers`] (long-lived
//!     background task)
//!   * [`lookup_media_alias`] (called by the `/media/:id.ext` handler)

pub mod desc;
pub mod didl;
#[cfg(feature = "system")]
pub mod discovery;
pub mod xml_sax;
pub use crate::dlna_media_alias as media_alias;
pub mod soap;
pub mod ssdp;
pub mod types;
pub mod util;

#[cfg(feature = "system")]
use types::DiscoveredDevice;
pub use types::{MediaType, Renderer};

#[cfg(feature = "system")]
use serde_json::json;

/// Cast a media URL to a renderer by UDN. Mirrors Go `Cast(ctx, …)`.
/// Performs: rewrite `/fs?id=…` URLs to `/media/:alias.ext`, build
/// DIDL-Lite metadata, then issue `Stop` (best-effort) → `SetAVTransportURI`
/// → `Play`.
///
/// `db` is used to resolve the encrypted `file_id` in `/fs?id=…` URLs to
/// a real filesystem path before registering the alias. Mirrors Go's
/// `plainfs.PathFromFileID(id)` call inside `dlnaSafeMediaURL`.
#[cfg(feature = "system")]
pub fn cast(
    renderer_udn: &str,
    media_url: &str,
    title: &str,
    mime: &str,
    media_type: MediaType,
    prefs: &crate::prefs::Prefs,
) -> Result<(), String> {
    let udn = renderer_udn.trim();
    if udn.is_empty() {
        return Err("renderer UDN is empty".to_string());
    }

    // Rewrite the URL so the TV can reach it on our HTTP port.
    let media_url_owned = media_alias::safe_media_url_with_prefs(media_url, mime, Some(prefs));
    if crate::utils::http_url::parse_http_url(&media_url_owned).is_none() {
        return Err("invalid url".to_string());
    }
    if util::dlna_debug_enabled() {
        log::info!(
            "[DLNA] cast udn={udn} url={media_url_owned} mime={mime} media_type={media_type:?} title={title}"
        );
    }

    let meta = didl::didl_lite_metadata(&media_url_owned, title, mime, media_type);

    // Find the device descriptor (cache hit or fresh discovery).
    let dev = desc::find_upnp_device_by_udn(udn)?;
    if !dev.has_av_transport || dev.av_transport.service_type.is_empty() {
        return Err("renderer has no AVTransport".to_string());
    }

    // Some renderers behave better if we Stop first. Best-effort.
    let _ = soap::soap_av_transport(&dev, "Stop", "<InstanceID>0</InstanceID>");

    // SetURI + metadata, then Play.
    let set_body = format!(
        "<InstanceID>0</InstanceID><CurrentURI>{}</CurrentURI><CurrentURIMetaData>{}</CurrentURIMetaData>",
        util::xml_escape(&media_url_owned),
        util::xml_escape(&meta),
    );
    soap::soap_av_transport(&dev, "SetAVTransportURI", &set_body)?;
    soap::soap_av_transport(&dev, "Play", "<InstanceID>0</InstanceID><Speed>1</Speed>")?;
    Ok(())
}

/// JSON payload published on `dlna:renderer:found`. Mirrors
/// `rendererPayload(d)` in Go.
#[cfg(feature = "system")]
pub fn renderer_payload(d: &DiscoveredDevice) -> serde_json::Value {
    json!({
        "udn": d.udn,
        "name": d.friendly_name,
        "manufacturer": d.manufacturer,
        "modelName": d.model_name,
        "location": d.location,
    })
}

/// JSON payload published on `dlna:discovery:done`.
#[cfg(feature = "system")]
pub fn discovery_done_payload() -> serde_json::Value {
    json!({ "done": true })
}

/// Join the long-lived renderer discovery task. Mirrors Go
/// `StartRendererDiscovery(clientID)`. Idempotent: if a task is already
/// running, we just register `client_id` and flush the current cache.
#[cfg(feature = "system")]
pub fn start_renderer_discovery(client_id: &str) {
    discovery::start_renderer_discovery(client_id);
}

/// Snapshot the cached renderers (sorted by name). Mirrors Go
/// `CachedRenderers()`.
#[cfg(feature = "system")]
pub fn cached_renderers() -> Vec<Renderer> {
    discovery::cached_renderers()
}

#[cfg(all(test, feature = "system"))]
#[path = "../../tests/unit/dlna_sender/dlna/mod.rs"]
mod tests;
