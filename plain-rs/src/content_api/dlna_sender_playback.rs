use super::{dlna_sender_runtime::Item, server::ServerState};
use crate::dlna_sender::{
    didl, soap as protocol,
    types::{DiscoveredDevice, MediaType},
    util::xml_escape,
};
use serde_json::json;
use std::time::Duration;

pub(super) async fn soap(
    state: &ServerState,
    device: &DiscoveredDevice,
    action: &str,
    params: &str,
) -> Result<String, String> {
    let facts = state.host.call("systemCastAddressFacts", json!({})).await?;
    protocol::soap_response(
        device,
        action,
        params,
        facts["senderName"].as_str().unwrap_or_default(),
    )
    .await
}
async fn mime_type(state: &ServerState, path: &str) -> String {
    state
        .host
        .call(
            "systemFileResource",
            json!({"operation":"mime","path":path,"params":{}}),
        )
        .await
        .ok()
        .and_then(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| crate::utils::mime::mime_from_ext(path).to_owned())
}
async fn media_url(state: &ServerState, path: &str, art: bool) -> Result<String, String> {
    let facts = state.host.call("systemCastAddressFacts", json!({})).await?;
    let base = facts["baseUrl"]
        .as_str()
        .ok_or("Missing cast HTTP address")?;
    let mime = if art {
        "image/jpeg".to_owned()
    } else {
        mime_type(state, path).await
    };
    let (id, ext) = crate::dlna_media_alias::register_for_owner(path, &mime, &state.token);
    Ok(format!(
        "{base}/media/{id}.{}",
        if art { "jpg" } else { &ext }
    ))
}
pub(super) async fn cast(state: &ServerState, item: Item, advance: bool) -> Result<(), String> {
    let runtime = state.cast.clone();
    let device = runtime.device()?;
    let url = media_url(state, &item.path, false).await?;
    let art = if item.album_art.is_empty() {
        String::new()
    } else {
        media_url(state, &item.album_art, true).await?
    };
    let mime = mime_type(state, &item.path).await;
    let media_type = if mime.starts_with("audio/") {
        MediaType::Audio
    } else if mime.starts_with("image/") {
        MediaType::Image
    } else if mime.starts_with("video/") {
        MediaType::Video
    } else {
        MediaType::Unknown
    };
    let meta = if item.title.is_empty() {
        String::new()
    } else {
        didl::metadata(&url, &item.title, &mime, media_type, &art, true)
    };
    soap(state,&device,"SetAVTransportURI",&format!("<InstanceID>0</InstanceID><CurrentURI>{}</CurrentURI><CurrentURIMetaData>{}</CurrentURIMetaData>",xml_escape(&url),xml_escape(&meta))).await?;
    if !advance {
        soap(
            state,
            &device,
            "Play",
            "<InstanceID>0</InstanceID><Speed>1</Speed>",
        )
        .await?;
    }
    if item.audio {
        let track = item.clone();
        let _ = state.audio
            .run(move |db, _lib| {
                crate::library::audio_queue::on_playing(
                    db,
                    &track.path,
                    &track.title,
                    &track.artist,
                    track.duration_ms,
                )
            })
            .await;
    }
    if !advance {
        unsubscribe(state).await;
    }
    {
        let mut s = runtime.snapshot.lock().unwrap();
        s.current_audio = item.audio || media_type == MediaType::Audio;
        s.current_uri = item.path;
        s.playing = true;
        s.active = true;
    }
    if !advance {
        let facts = state.host.call("systemCastAddressFacts", json!({})).await?;
        let callback = format!(
            "{}/callback/cast",
            facts["baseUrl"].as_str().unwrap_or_default()
        );
        if let Ok(subscription) = protocol::subscription(&device, Some(&callback), "", false).await
        {
            if !subscription.sid.is_empty() {
                *runtime.subscription_device.lock().unwrap() = Some(device.clone());
                *runtime.subscription_renew_after.lock().unwrap() = subscription.renew_after;
                *runtime.callback_sequence.lock().unwrap() = None;
                let mut s = runtime.snapshot.lock().unwrap();
                s.sid = subscription.sid;
                s.supports_callback = true;
            }
        }
        start_tasks(state);
    }
    runtime.publish(state);
    Ok(())
}
async fn unsubscribe(state: &ServerState) {
    let runtime = &state.cast;
    if let Some(task) = runtime.renewal.lock().unwrap().take() {
        task.abort();
    }
    let sid = {
        let mut s = runtime.snapshot.lock().unwrap();
        s.supports_callback = false;
        std::mem::take(&mut s.sid)
    };
    let device = runtime.subscription_device.lock().unwrap().take();
    *runtime.callback_sequence.lock().unwrap() = None;
    if let Some(device) = device.filter(|_| !sid.is_empty()) {
        let _ = protocol::subscription(&device, None, &sid, true).await;
    }
}
pub(super) async fn end(state: &ServerState, stop: bool) {
    let runtime = &state.cast;
    if let Some(task) = runtime.polling.lock().unwrap().take() {
        task.abort();
    }
    if let Ok(device) = runtime.device() {
        if stop {
            let _ = soap(state, &device, "Stop", "<InstanceID>0</InstanceID>").await;
        }
    }
    unsubscribe(state).await;
    let mut s = runtime.snapshot.lock().unwrap();
    s.playing = false;
    s.progress_ms = 0;
    s.duration_ms = 0;
    s.supports_callback = false;
    if stop {
        crate::dlna_media_alias::release_owner(&state.token);
        s.items.clear();
        s.current_uri.clear();
        s.current_audio = false;
        s.current_device = None;
        s.active = false;
    }
}
fn start_tasks(state: &ServerState) {
    let runtime = &state.cast;
    if let Some(task) = runtime.polling.lock().unwrap().take() {
        task.abort();
    }
    let poll_state = state.clone();
    *runtime.polling.lock().unwrap() = Some(tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let runtime = &poll_state.cast;
            let _guard = runtime.operations.lock().await;
            let snap = runtime.snapshot();
            if snap.current_uri.is_empty() {
                break;
            }
            if !snap.playing {
                continue;
            }
            let Ok(device) = runtime.device() else {
                break;
            };
            let Ok(xml) = soap(
                &poll_state,
                &device,
                "GetPositionInfo",
                "<InstanceID>0</InstanceID>",
            )
            .await
            else {
                continue;
            };
            let fields = xml_fields(&xml);
            {
                let mut s = runtime.snapshot.lock().unwrap();
                s.progress_ms = super::cast_runtime::time_ms(
                    fields
                        .get("RelTime")
                        .map(String::as_str)
                        .unwrap_or_default(),
                );
                s.duration_ms = super::cast_runtime::time_ms(
                    fields
                        .get("TrackDuration")
                        .map(String::as_str)
                        .unwrap_or_default(),
                );
                s.supports_callback = true;
            }
            runtime.publish(&poll_state);
        }
    }));
    if runtime.snapshot().sid.is_empty() {
        return;
    }
    let renew_state = state.clone();
    *runtime.renewal.lock().unwrap() = Some(tokio::spawn(async move {
        loop {
            let delay = *renew_state.cast.subscription_renew_after.lock().unwrap();
            tokio::time::sleep(delay).await;
            let runtime = &renew_state.cast;
            let _guard = runtime.operations.lock().await;
            let Ok(device) = runtime.device() else {
                break;
            };
            let sid = runtime.snapshot().sid;
            if sid.is_empty() {
                break;
            }
            match protocol::subscription(&device, None, &sid, false).await {
                Ok(next) => {
                    *runtime.subscription_renew_after.lock().unwrap() = next.renew_after;
                    if !next.sid.is_empty() {
                        runtime.snapshot.lock().unwrap().sid = next.sid;
                    }
                }
                Err(_) => {
                    runtime.snapshot.lock().unwrap().supports_callback = false;
                }
            }
            runtime.publish(&renew_state);
        }
    }));
}
fn xml_fields(xml: &str) -> std::collections::HashMap<String, String> {
    use crate::dlna_sender::xml_sax::{Event, Reader};
    let mut reader = Reader::new(xml.as_bytes());
    let mut path = Vec::new();
    let mut values = std::collections::HashMap::new();
    while let Some(event) = reader.next_event() {
        match event {
            Event::Start { name } => path.push(name),
            Event::End { .. } => {
                path.pop();
            }
            Event::Text { content } => {
                if let Some(name) = path.last() {
                    values.insert(name.clone(), content);
                }
            }
        }
    }
    values
}
