pub mod http_router;
pub mod receiver_engine;
pub mod renderer_state;
pub mod soap_handler;
pub mod ssdp_messages;
pub mod types;
pub mod xml_templates;

/// Whether a persisted sender list (`ip|name` entries, see
/// [`crate::api::ShellHooks::dlna_senders`]) contains `ip`.
pub fn senders_contain_ip(list: &[String], ip: &str) -> bool {
    list.iter().any(|entry| entry.split('|').next() == Some(ip))
}

/// True when `method` + `path` targets a DLNA MediaRenderer receiver endpoint.
/// Mirrors plain-app's `isDlnaReceiverPath` in `HttpRouteRegistry.kt`.
pub fn is_receiver_path(method: &str, path: &str) -> bool {
    let clean = path.split('?').next().unwrap_or(path);
    if method == "GET" && clean == "/description.xml" {
        return true;
    }
    clean.starts_with("/AVTransport/") || clean.starts_with("/RenderingControl/")
}
