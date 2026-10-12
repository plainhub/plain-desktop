use crate::assembly::server::ServerState;
use crate::services::casting::receiver::control::emit;
#[cfg(test)]
use crate::services::casting::receiver::control::snapshot;
use axum::{
    extract::{ConnectInfo, Request as HttpRequest, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{collections::HashMap, net::SocketAddr};

/// Control points send a small description document; a larger cap than they
/// need would only widen the surface for a stray LAN client.
const MAX_BODY: usize = 2 * 1024 * 1024;

/// An over-limit body and a body that failed mid-read are different faults, and
/// neither may reach the protocol as the empty request it would otherwise look
/// like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BodyFailure {
    TooLarge,
    Unreadable,
}

impl From<BodyFailure> for StatusCode {
    fn from(failure: BodyFailure) -> Self {
        match failure {
            BodyFailure::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            BodyFailure::Unreadable => StatusCode::BAD_REQUEST,
        }
    }
}

/// Reads the body under [MAX_BODY] and reports which limit was hit, which the
/// buffered extractor cannot: it folds both into one error.
async fn read_body(body: axum::body::Body, limit: usize) -> Result<Vec<u8>, BodyFailure> {
    let mut stream = body.into_data_stream();
    let mut buffer = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| BodyFailure::Unreadable)?;
        if buffer.len() + chunk.len() > limit {
            return Err(BodyFailure::TooLarge);
        }
        buffer.extend_from_slice(&chunk);
    }
    Ok(buffer)
}

fn access_allowed(state: &ServerState) -> bool {
    plain_rs::prefs::dlna::enabled(&state.prefs)
}

fn headers_to_map(headers: &HeaderMap) -> HashMap<String, String> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_ascii_lowercase(), value.to_string()))
        })
        .collect()
}

async fn device_name(state: &ServerState) -> Result<String, String> {
    let settings =
        plain_rs::prefs::user::snapshot(&state.prefs).map_err(|error| error.to_string())?;
    if !settings.device_name.is_empty() {
        return Ok(settings.device_name);
    }
    let facts = state.host.call("systemCastAddressFacts", json!({})).await?;
    facts
        .get("defaultDeviceName")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .map(String::from)
        .ok_or_else(|| "Missing cast device name".to_string())
}

/// The address the SSDP LOCATION points at and the device description
/// advertises. Must agree with the one the advertiser used, or a control
/// point fetches the description from an address the renderer never joined.
fn local_ip() -> String {
    plain_rs::protocols::mdns::host_responder::local_ipv4_strs()
        .into_iter()
        .find(|ip| !ip.starts_with("127."))
        .or_else(|| {
            plain_rs::protocols::mdns::host_responder::local_ipv4_strs()
                .into_iter()
                .next()
        })
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

/// Public DLNA receiver endpoint. One handler for every UPnP path — the
/// router itself decides by method and path suffix, mirroring plain-app's
/// `DlnaHttpRouter`.
pub(crate) async fn receiver(
    State(state): State<ServerState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: HttpRequest,
) -> Response {
    if !access_allowed(&state) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let (parts, body) = request.into_parts();
    // A body that never arrived intact must not be answered as an empty SOAP
    // request, and it must not reach the router or the command channel.
    let bytes = match read_body(body, MAX_BODY).await {
        Ok(bytes) => bytes,
        Err(failure) => return StatusCode::from(failure).into_response(),
    };
    let body = String::from_utf8_lossy(&bytes).to_string();
    let headers = headers_to_map(&parts.headers);

    let allowed = plain_rs::prefs::dlna::senders(&state.prefs, "dlna_allowed_senders");
    let denied = plain_rs::prefs::dlna::senders(&state.prefs, "dlna_denied_senders");
    // The allow/deny rules and the "remember this sender" prefs both key on
    // the peer address, so it comes from the socket — a control point that
    // forged `c-ip` would otherwise be able to impersonate a trusted sender.
    let sender_ip = peer.ip().to_string();

    let name = match device_name(&state).await {
        Ok(name) => name,
        Err(error) => {
            log::error!("DLNA device name failed: {error}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let response = crate::services::casting::receiver::http_router::route(
        &state.dlna.state,
        parts.method.as_str(),
        parts.uri.path(),
        &headers,
        &body,
        state.dlna.device_uuid(),
        &name,
        &local_ip(),
        &sender_ip,
        // The channel exists from construction: these routes answer as soon
        // as the toggle is on, which can be before the app starts the engine.
        &state.dlna.command_sender().expect("command channel"),
        &allowed,
        &denied,
    )
    .await;

    let renderer = state.dlna.snapshot().await;
    emit(&state, &renderer);

    let mut out = Response::builder()
        .status(StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR));
    for (name, value) in &response.headers {
        out = out.header(name.as_str(), value.as_str());
    }
    if let Some(content_type) = &response.content_type {
        out = out.header("content-type", content_type.as_str());
    }
    out.body(axum::body::Body::from(response.body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// The public router half. Grouped here so `start_public` only has to merge
/// it; axum needs an explicit `MethodRouter` for SUBSCRIBE/UNSUBSCRIBE.
#[cfg(feature = "http_transport")]
pub(crate) fn router(state: ServerState) -> axum::Router {
    use axum::routing::{any, get};
    axum::Router::new()
        .route("/description.xml", get(receiver))
        .route("/AVTransport/scpd.xml", get(receiver))
        .route("/RenderingControl/scpd.xml", get(receiver))
        .route("/AVTransport/control", axum::routing::post(receiver))
        .route("/RenderingControl/control", axum::routing::post(receiver))
        // axum 0.7 can only route the ten standard methods, so the GENA
        // event paths are registered for any method and the router answers
        // SUBSCRIBE/UNSUBSCRIBE — anything else 404s, same as before.
        .route("/AVTransport/event", any(receiver))
        .route("/RenderingControl/event", any(receiver))
        .with_state(state)
}

#[cfg(all(test, feature = "http_transport"))]
#[path = "../../../../tests/unit/graphql/resolvers/casting/control.rs"]
mod tests;
