//! DLNA MediaRenderer receiver: the public UPnP endpoints and the host
//! control surface behind them.
//!
//! The public half serves `description.xml`, the `scpd.xml` service
//! descriptions, SOAP `control` and the GENA `event` (SUBSCRIBE/UNSUBSCRIBE)
//! paths on the shared web port, gated on the `dlna` toggle plus `service` —
//! the same pair as plain-app's `TempData.canDLNAAccess()`.
//!
//! The host half is what the app UI drives: start/stop the SSDP advertiser,
//! read a state snapshot for the Compose overlay, and answer the cast-request
//! prompt. The sender allow/deny lists stay in prefs, so accepting a cast also
//! remembers the sender exactly as plain-app did.

use super::server::ServerState;
use crate::dlna_receiver::renderer_state::DlnaRendererState;
use axum::{
    Json,
    body::to_bytes,
    extract::{ConnectInfo, Request as HttpRequest, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::HashMap, net::SocketAddr};

/// Broadcast when the renderer state changes, so the host overlay can pull a
/// fresh snapshot instead of polling.
pub(super) const EVENT_UPDATED: i32 = 10005;

/// Control points send a small description document; a larger cap than they
/// need would only widen the surface for a stray LAN client.
const MAX_BODY: usize = 2 * 1024 * 1024;

fn access_allowed(state: &ServerState) -> bool {
    crate::prefs::dlna::enabled(&state.prefs) && state.prefs.get_or("service", false)
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

async fn device_name(state: &ServerState) -> String {
    state
        .host
        .call("dlnaDeviceName", json!({}))
        .await
        .ok()
        .and_then(|value| value.get("name").and_then(Value::as_str).map(String::from))
        .unwrap_or_default()
}

/// The address the SSDP LOCATION points at and the device description
/// advertises. Must agree with the one the advertiser used, or a control
/// point fetches the description from an address the renderer never joined.
fn local_ip() -> String {
    crate::mdns::host_responder::local_ipv4_strs()
        .into_iter()
        .find(|ip| !ip.starts_with("127."))
        .or_else(|| {
            crate::mdns::host_responder::local_ipv4_strs()
                .into_iter()
                .next()
        })
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

pub(super) fn snapshot(state: &DlnaRendererState) -> Value {
    json!({
        "isRunning": state.is_running,
        "isRetrying": state.is_retrying,
        "mediaUri": state.media_uri,
        "mediaTitle": state.media_title,
        "mediaAlbumArtUri": state.media_album_art_uri,
        "mediaType": state.media_type,
        "playbackState": state.playback_state,
        "port": state.port,
        "currentPositionMs": state.current_position_ms,
        "durationMs": state.duration_ms,
        "seekTargetMs": state.seek_target_ms,
        "pendingCastRequest": state.pending_cast_request,
        "startError": state.start_error,
    })
}

/// Public DLNA receiver endpoint. One handler for every UPnP path — the
/// router itself decides by method and path suffix, mirroring plain-app's
/// `DlnaHttpRouter`.
pub(super) async fn receiver(
    State(state): State<ServerState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: HttpRequest,
) -> Response {
    if !access_allowed(&state) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let (parts, body) = request.into_parts();
    let bytes = to_bytes(body, MAX_BODY).await.unwrap_or_default();
    let body = String::from_utf8_lossy(&bytes).to_string();
    let headers = headers_to_map(&parts.headers);

    let allowed = crate::prefs::dlna::senders(&state.prefs, "dlna_allowed_senders");
    let denied = crate::prefs::dlna::senders(&state.prefs, "dlna_denied_senders");
    // The allow/deny rules and the "remember this sender" prefs both key on
    // the peer address, so it comes from the socket — a control point that
    // forged `c-ip` would otherwise be able to impersonate a trusted sender.
    let sender_ip = peer.ip().to_string();
    let had_pending = state.dlna.state.read().await.pending_cast_request.is_some();

    let response = crate::dlna_receiver::http_router::route(
        &state.dlna.state,
        parts.method.as_str(),
        parts.uri.path(),
        &headers,
        &body,
        state.dlna.device_uuid(),
        &device_name(&state).await,
        &local_ip(),
        &sender_ip,
        // The channel exists from construction: these routes answer as soon
        // as the toggle is on, which can be before the app starts the engine.
        &state.dlna.command_sender().expect("command channel"),
        &allowed,
        &denied,
    )
    .await;

    if !had_pending && state.dlna.state.read().await.pending_cast_request.is_some() {
        emit(&state);
    }

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
pub(super) fn router(state: ServerState) -> axum::Router {
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

fn emit(state: &ServerState) {
    let _ = state.events.send(crate::ws_event::WsEvent::broadcast(
        EVENT_UPDATED,
        json!({"pending": true}).to_string(),
    ));
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Request {
    Start {
        port: u16,
    },
    Stop {},
    Retry {
        port: u16,
    },
    Snapshot {},
    Accept {
        remember: bool,
    },
    Reject {
        remember: bool,
    },
    Position {
        #[serde(rename = "positionMs")]
        position_ms: i64,
        #[serde(rename = "durationMs")]
        duration_ms: i64,
    },
    SeekConsumed {},
    Playback {
        state: crate::dlna_receiver::types::DlnaPlaybackState,
    },
    Stopped {},
}

pub(super) async fn execute(state: &ServerState, request: Request) -> Result<Value, String> {
    let engine = &state.dlna;
    match request {
        Request::Start { port } => {
            engine.start(port).await;
            Ok(snapshot(&engine.snapshot().await))
        }
        Request::Stop {} => {
            engine.stop().await;
            Ok(snapshot(&engine.snapshot().await))
        }
        Request::Retry { port } => {
            // Restarting from the engine's own scope would self-cancel: the
            // stop path aborts the tasks this command is running on.
            engine.set_retrying(true).await;
            engine.stop().await;
            engine.start(port).await;
            engine.set_retrying(false).await;
            Ok(snapshot(&engine.snapshot().await))
        }
        Request::Snapshot {} => Ok(snapshot(&engine.snapshot().await)),
        Request::Accept { remember } => {
            let before = engine.snapshot().await.pending_cast_request;
            engine.accept_cast(remember, &state.prefs).await;
            let after = engine.snapshot().await;
            if before.is_some() {
                emit(state);
            }
            Ok(snapshot(&after))
        }
        Request::Reject { remember } => {
            let before = engine.snapshot().await.pending_cast_request;
            engine.reject_cast(remember, &state.prefs).await;
            if before.is_some() {
                emit(state);
            }
            Ok(snapshot(&engine.snapshot().await))
        }
        Request::Position {
            position_ms,
            duration_ms,
        } => {
            engine.set_position(position_ms, duration_ms).await;
            Ok(snapshot(&engine.snapshot().await))
        }
        Request::SeekConsumed {} => {
            engine.clear_seek_target().await;
            Ok(snapshot(&engine.snapshot().await))
        }
        Request::Playback { state } => {
            engine.set_playback_state(state).await;
            Ok(snapshot(&engine.snapshot().await))
        }
        Request::Stopped {} => {
            engine.clear_media().await;
            Ok(snapshot(&engine.snapshot().await))
        }
    }
}

pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut stop = state.stop.clone();
    if *stop.borrow() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let result = tokio::select! {
        _ = stop.changed() => Err("Server stopped".to_string()),
        result = execute(&state, request) => result,
    };
    match result {
        Ok(value) => Json(json!({"result": value})).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, Json(json!({"error": error}))).into_response(),
    }
}

#[cfg(all(test, feature = "http_transport"))]
#[path = "../../tests/unit/content_api/public_dlna.rs"]
mod tests;
