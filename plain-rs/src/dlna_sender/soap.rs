//! SOAP envelope builder + AVTransport control call.
//!
//! Mirrors `internal/dlna/soap.go`. The Go side hard-codes the three
//! actions it actually issues (`Stop`, `SetAVTransportURI`, `Play`);
//! callers pass the action name + the action body XML as a string.
//! We follow the same shape.

use std::time::Duration;

use super::types::DiscoveredDevice;
use super::util::{dlna_debug_enabled, dlna_debug_payload_enabled, truncate_for_log, xml_escape};
use std::io::Read;

/// Per-call HTTP timeout (Go: 3s). Real TVs are often slow; the timeout
/// exists to keep the GraphQL resolver bounded.
const SOAP_TIMEOUT: Duration = Duration::from_secs(3);
/// Cap on the SOAP response body. We never parse it (Go doesn't either
/// — only the status code matters) but the limit prevents a malicious
/// renderer from streaming forever.
const SOAP_BODY_CAP: usize = 512 << 10;

/// Issue a SOAP action against a renderer's AVTransport endpoint. The
/// caller is responsible for building the `<action>` body XML (we only
/// wrap it in the envelope and set the right headers). Mirrors Go
/// `soapAVTransport(dev, action, paramsXML)`.
#[cfg(feature = "system")]
pub fn soap_av_transport(
    dev: &DiscoveredDevice,
    action: &str,
    params_xml: &str,
) -> Result<(), String> {
    let svc = &dev.av_transport;
    let st = svc.service_type.trim();
    if st.is_empty() {
        return Err("missing AVTransport serviceType".to_string());
    }
    let endpoint = resolve_service_endpoint(&dev.location, &svc.control_url)?;

    let envelope = envelope(st, action, params_xml);

    if dlna_debug_enabled() {
        log::info!(
            "soap: tx action={action} endpoint={endpoint} bytes={}",
            envelope.len()
        );
        if dlna_debug_payload_enabled() {
            log::debug!(
                "soap: txBody action={action}\n{}",
                truncate_for_log(&envelope, 4096)
            );
        }
    }

    let agent = ureq::AgentBuilder::new().timeout(SOAP_TIMEOUT).build();
    let resp = agent
        .post(&endpoint)
        .set("Content-Type", "text/xml; charset=\"utf-8\"")
        .set("SOAPAction", &format!("\"{st}#{action}\""))
        .set("Connection", "close")
        .send_string(&envelope);

    let resp = match resp {
        Ok(r) => r,
        Err(ureq::Error::Status(s, r)) => {
            let _ = r; // drop
            return Err(format!("soap {action} http {s}"));
        }
        Err(ureq::Error::Transport(t)) => return Err(format!("soap {action} transport: {t}")),
    };
    let status = resp.status();
    // Drain (capped) response body — we don't use it, but reading
    // closes the connection cleanly.
    let _ = resp
        .into_reader()
        .take(SOAP_BODY_CAP as u64)
        .read_to_end(&mut Vec::new());
    if !(200..300).contains(&status) {
        return Err(format!("soap {action} http {status}"));
    }
    Ok(())
}

/// Resolve a device's `controlURL` (which can be relative or absolute)
/// against its `Location` base. Mirrors Go `resolveServiceEndpoint`.
pub fn resolve_service_endpoint(location: &str, control_url: &str) -> Result<String, String> {
    // `loc` is only used to validate `location`; if it's not a parseable
    // URL we bail out. The actual base for the relative resolve is
    // `location.trim()` itself.
    if crate::utils::http_url::parse_http_url(location.trim()).is_none() {
        return Err("parse location".to_string());
    }
    let control = control_url.trim();
    if control.is_empty() {
        return Err("empty controlURL".to_string());
    }
    // Absolute control URL? Use as-is. Relative? Resolve against `location`.
    if crate::utils::http_url::parse_http_url(control).is_some() {
        return Ok(control.to_string());
    }
    let resolved = crate::utils::http_url::join(location.trim(), control)
        .ok_or_else(|| "resolve reference".to_string())?;
    // We have to re-parse the result to check it has a host — but we
    // just built it from a base that did, so this is paranoia.
    if crate::utils::http_url::parse_http_url(&resolved)
        .map(|u| u.host.is_empty())
        .unwrap_or(true)
    {
        return Err("invalid resolved endpoint".to_string());
    }
    Ok(resolved)
}

#[cfg(test)]
#[path = "../../tests/unit/dlna_sender/dlna/soap.rs"]
mod tests;

pub fn envelope(st: &str, action: &str, params_xml: &str) -> String {
    let action_esc = xml_escape(action);
    let st_esc = xml_escape(st);
    let body_inner = format!("<u:{action_esc} xmlns:u=\"{st_esc}\">{params_xml}</u:{action_esc}>");
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
<s:Envelope s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\" \
xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\">\
<s:Body>{body_inner}</s:Body>\
</s:Envelope>"
    )
}

#[cfg(feature = "content_api")]
pub async fn soap_response(
    dev: &DiscoveredDevice,
    action: &str,
    params: &str,
    sender_name: &str,
) -> Result<String, String> {
    let st = &dev.av_transport.service_type;
    if st.is_empty() {
        return Err("missing AVTransport serviceType".into());
    }
    let endpoint = resolve_service_endpoint(&dev.location, &dev.av_transport.control_url)?;
    let client = reqwest::Client::builder()
        .timeout(SOAP_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client
        .post(endpoint)
        .header("Content-Type", "text/xml")
        .header("SOAPAction", format!("\"{st}#{action}\""))
        .header("c-name", sender_name)
        .body(envelope(st, action, params))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("soap {action} http {}", response.status()));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() + chunk.len() > SOAP_BODY_CAP {
            return Err("SOAP response too large".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|e| e.to_string())
}
#[cfg(feature = "content_api")]
pub struct Subscription {
    pub sid: String,
    pub renew_after: std::time::Duration,
}

#[cfg(feature = "content_api")]
pub async fn subscription(
    dev: &DiscoveredDevice,
    callback: Option<&str>,
    sid: &str,
    unsubscribe: bool,
) -> Result<Subscription, String> {
    let endpoint = resolve_service_endpoint(&dev.location, &dev.av_transport.event_sub_url)?;
    let client = reqwest::Client::builder()
        .timeout(SOAP_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;
    let method = reqwest::Method::from_bytes(if unsubscribe {
        b"UNSUBSCRIBE"
    } else {
        b"SUBSCRIBE"
    })
    .map_err(|e| e.to_string())?;
    let mut request = client.request(method, endpoint);
    if !sid.is_empty() {
        request = request.header("SID", sid);
    }
    if !unsubscribe {
        request = request.header("TIMEOUT", "Second-3600");
        if let Some(callback) = callback {
            request = request
                .header("NT", "upnp:event")
                .header("CALLBACK", format!("<{callback}>"));
        }
    }
    let response = request.send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("subscription http {}", response.status()));
    }
    let lifetime = response
        .headers()
        .get("TIMEOUT")
        .and_then(|v| v.to_str().ok())
        .and_then(|value| value.split_once('-'))
        .filter(|(kind, _)| kind.eq_ignore_ascii_case("second"))
        .and_then(|(_, seconds)| seconds.parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .unwrap_or(3600);
    Ok(Subscription {
        sid: response
            .headers()
            .get("SID")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned(),
        renew_after: std::time::Duration::from_millis(lifetime.saturating_mul(500).max(1)),
    })
}
