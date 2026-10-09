use super::server::ServerState;
use axum::{
    extract::{Path, Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

pub(super) fn time_ms(time: &str) -> i64 {
    // Mobile playback reports whole seconds even when the renderer includes fractions.
    let time = time.split('.').next().unwrap_or_default();
    crate::dlna_receiver::soap_handler::parse_dlna_time_to_ms(time).max(0)
}
pub(super) async fn media(
    State(state): State<ServerState>,
    Path(raw): Path<String>,
    request: Request,
) -> Response {
    if !state.prefs.get_user_or("service", false) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let id = raw.split('.').next().unwrap_or_default();
    let Some((path, mime)) = crate::dlna_media_alias::lookup(id) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if path.starts_with("http://") || path.starts_with("https://") {
        return super::mobile_files::secure(
            super::public_proxy::proxy(&path, request.headers()).await,
            None,
            false,
        );
    }
    if path.starts_with("content://") {
        #[cfg(feature = "http_transport")]
        {
            return match state.bridge.resource(&path, state.stop.clone()).await {
                Ok(body) => super::mobile_files::secure(body.into_response(), None, false),
                Err(status) => status.into_response(),
            };
        }
        #[cfg(not(feature = "http_transport"))]
        {
            return StatusCode::NOT_FOUND.into_response();
        }
    }
    let path = match super::mobile_files::resolve(&state, &path).await {
        Ok(path) => path,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let mut response =
        super::server::files::stream_file(std::path::Path::new(&path), &mime, request.headers())
            .await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert("x-content-type-options", "nosniff".parse().unwrap());
    if !mime.starts_with("image/")
        && (response.status() == StatusCode::OK || response.status() == StatusCode::PARTIAL_CONTENT)
    {
        *response.status_mut() = StatusCode::PARTIAL_CONTENT;
        response
            .headers_mut()
            .insert("realTimeInfo.dlna.org", "DLNA.ORG_TLAG=*".parse().unwrap());
        response
            .headers_mut()
            .insert("Server", "DLNADOC/1.50 UPnP/1.0 Plain/1.0".parse().unwrap());
        response
            .headers_mut()
            .insert("transfermode.dlna.org", "Streaming".parse().unwrap());
        response
            .headers_mut()
            .insert("contentfeatures.dlna.org", "".parse().unwrap());
    }
    super::mobile_files::secure(response, None, false)
}
fn attributes(xml: &str) -> std::collections::HashMap<String, String> {
    use crate::utils::xml::{Event, Reader};
    let mut values = std::collections::HashMap::new();
    let mut documents = vec![xml.to_owned()];
    let mut remaining = 8;
    while let Some(document) = documents.pop() {
        if remaining == 0 {
            break;
        }
        remaining -= 1;
        let mut reader = Reader::new(&document);
        let mut last_change = None::<String>;
        while let Some(event) = reader.next() {
            match event {
                Event::Start(tag) => {
                    // Keys keep the casing the sender used — the callback
                    // looks them up as `TrackDuration`, not `trackduration`.
                    if let Some(val) = tag.attr("val") {
                        values.insert(tag.raw.clone(), val.to_owned());
                    }
                    if tag.name == "avtransporturimetadata" {
                        values.entry(tag.raw.clone()).or_default();
                    }
                    if tag.name == "lastchange" {
                        last_change = Some(String::new());
                    }
                }
                Event::Text(text) => {
                    if let Some(body) = last_change.as_mut() {
                        body.push_str(&text);
                    }
                }
                Event::End(name) if name == "lastchange" => {
                    if let Some(body) = last_change.take().filter(|body| body.contains('<')) {
                        documents.push(body);
                    }
                }
                _ => {}
            }
        }
    }
    values
}
pub(super) async fn callback(State(state): State<ServerState>, request: Request) -> Response {
    if request.method().as_str() != "NOTIFY" {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    if !state.prefs.get_user_or("service", false) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let sid = request
        .headers()
        .get("SID")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let sequence = request
        .headers()
        .get("SEQ")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u32>().ok());
    let Ok(body) = axum::body::to_bytes(request.into_body(), 1024 * 1024).await else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let xml = String::from_utf8_lossy(&body);
    let fields = attributes(&xml);
    let _guard = state.cast.operations.lock().await;
    if sid
        .as_ref()
        .is_some_and(|sid| sid != &state.cast.snapshot().sid)
    {
        return StatusCode::OK.into_response();
    }
    if let Some(sequence) = sequence {
        let mut previous = state.cast.callback_sequence.lock().unwrap();
        if previous.is_some_and(|previous| {
            sequence.wrapping_sub(previous) == 0 || sequence.wrapping_sub(previous) >= (1 << 31)
        }) {
            return StatusCode::OK.into_response();
        }
        *previous = Some(sequence);
    }
    let outcome = apply(&state, &fields).await;
    if let Err(error) = outcome {
        log::warn!("Cast callback failed: {error}");
    }
    StatusCode::OK.into_response()
}
async fn apply(
    state: &ServerState,
    fields: &std::collections::HashMap<String, String>,
) -> anyhow::Result<()> {
    let runtime = &state.cast;
    {
        let mut s = runtime.snapshot.lock().unwrap();
        if let (Some(position), Some(duration)) =
            (fields.get("RelTime"), fields.get("TrackDuration"))
        {
            s.progress_ms = time_ms(position);
            s.duration_ms = time_ms(duration);
        }
        match fields.get("TransportState").map(String::as_str) {
            Some("PLAYING") => s.playing = true,
            Some("PAUSED_PLAYBACK" | "STOPPED") => s.playing = false,
            _ => {}
        }
    }
    if fields.get("TransportState").map(String::as_str) == Some("STOPPED")
        && !fields.contains_key("AVTransportURIMetaData")
        && runtime.snapshot().current_device.is_some()
    {
        let snapshot = runtime.snapshot();
        let next = if !snapshot.items.is_empty() {
            let index = snapshot
                .items
                .iter()
                .position(|item| item.path == snapshot.current_uri)
                .map_or(0, |index| (index + 1) % snapshot.items.len());
            Some(snapshot.items[index].clone())
        } else if snapshot.current_audio {
            let shuffle = state.prefs.get_user_or("audio_play_mode", String::new()) == "SHUFFLE";
            state
                .audio
                .run(move |db, lib| {
                    crate::library::audio_queue::select_next(db, lib, true, shuffle)
                })
                .await
                .map_err(|e| anyhow::anyhow!(e.message))?
                .map(|track| super::dlna_sender_runtime::Item {
                    path: track.path,
                    title: track.title,
                    audio: true,
                    ..Default::default()
                })
        } else {
            None
        };
        if let Some(next) =
            next.filter(|item| !item.path.is_empty() && item.path != snapshot.current_uri)
        {
            super::dlna_sender_playback::cast(state, next, true)
                .await
                .map_err(anyhow::Error::msg)?;
        }
    }
    runtime.publish(state);
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/cast_runtime.rs"]
mod tests;
