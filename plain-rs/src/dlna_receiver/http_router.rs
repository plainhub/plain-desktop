use std::sync::Arc;
use tokio::sync::RwLock;

use crate::dlna_receiver::receiver_engine;
use crate::dlna_receiver::renderer_state::DlnaRendererState;
use crate::dlna_receiver::soap_handler;
use crate::dlna_receiver::types::{DlnaCommand, DlnaPlaybackState, PendingCastRequest};
use crate::dlna_receiver::xml_templates;

pub struct DlnaHttpResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

#[allow(clippy::too_many_arguments)]
pub async fn route(
    state: &Arc<RwLock<DlnaRendererState>>,
    method: &str,
    path: &str,
    headers: &std::collections::HashMap<String, String>,
    body: &str,
    device_uuid: &str,
    device_name: &str,
    local_ip: &str,
    // The socket peer address, never a request header: the allow/deny rules
    // and the "remember this sender" prefs key on it, so a control point that
    // forged `c-ip` would otherwise impersonate a trusted sender.
    sender_ip: &str,
    command_tx: &tokio::sync::mpsc::UnboundedSender<DlnaCommand>,
    allowed_senders: &[String],
    denied_senders: &[String],
) -> DlnaHttpResponse {
    if path.ends_with("description.xml") {
        let s = state.read().await;
        let port = s.port;
        drop(s);
        let xml = xml_templates::device_description(local_ip, port, device_uuid, device_name);
        return http_ok(&xml, "text/xml; charset=\"utf-8\"");
    }

    if path.ends_with("scpd.xml") {
        return http_ok(xml_templates::SCPD_XML, "text/xml; charset=\"utf-8\"");
    }

    if method == "POST" && (path.ends_with("control") || path.contains("AVTransport")) {
        return handle_soap(
            state,
            headers,
            body,
            sender_ip,
            command_tx,
            allowed_senders,
            denied_senders,
        )
        .await;
    }

    if method == "POST" && path.contains("RenderingControl") {
        let xml = soap_handler::build_response("GetVolume", "<CurrentVolume>100</CurrentVolume>");
        return http_ok(&xml, "text/xml; charset=\"utf-8\"");
    }

    if method == "SUBSCRIBE" {
        return http_ok_subscribe();
    }

    if method == "UNSUBSCRIBE" {
        return http_ok("", "text/plain");
    }

    DlnaHttpResponse {
        status: 404,
        content_type: None,
        headers: vec![],
        body: String::new(),
    }
}

async fn handle_soap(
    state: &Arc<RwLock<DlnaRendererState>>,
    headers: &std::collections::HashMap<String, String>,
    body: &str,
    sender_ip: &str,
    command_tx: &tokio::sync::mpsc::UnboundedSender<DlnaCommand>,
    allowed_senders: &[String],
    denied_senders: &[String],
) -> DlnaHttpResponse {
    let soap_action = match headers
        .get("soapaction")
        .or_else(|| headers.get("SOAPACTION"))
    {
        Some(v) => v,
        None => {
            return DlnaHttpResponse {
                status: 500,
                content_type: None,
                headers: vec![],
                body: String::new(),
            };
        }
    };

    let (action, params) = soap_handler::parse_soap_action(soap_action, body);
    log::debug!("DLNA SOAP action: {action}");

    let sender_name = soap_handler::resolve_sender_name(headers, sender_ip);
    let sender_ip = sender_ip.to_string();

    let response_body = match action.as_str() {
        "SetAVTransportURI" => {
            let uri = params.get("CurrentURI").cloned().unwrap_or_default();
            let meta = params
                .get("CurrentURIMetaData")
                .cloned()
                .unwrap_or_default();
            let raw_title = soap_handler::extract_title_from_didl_meta(&meta);
            let title = if raw_title.is_empty() {
                uri.rsplit('/')
                    .next()
                    .unwrap_or(&uri)
                    .split('?')
                    .next()
                    .unwrap_or(&uri)
                    .to_string()
            } else {
                raw_title
            };
            let title = soap_handler::clean_media_title(&title);
            let media_type = soap_handler::extract_media_type_from_didl_meta(&meta, &uri);
            let album_art_uri = soap_handler::extract_album_art_uri_from_didl_meta(&meta);
            log::debug!("DLNA SetAVTransportURI uri={uri} title={title} type={media_type:?}");

            if !uri.is_empty() {
                let mut s = state.write().await;
                s.raw_pending_cast_request = Some(PendingCastRequest {
                    sender_ip,
                    sender_name,
                    media_uri: uri,
                    media_title: title,
                    media_type,
                    album_art_uri,
                });
                s.pending_play_queued = false;
                drop(s);
                receiver_engine::check_rules(state, allowed_senders, denied_senders).await;
            }
            soap_handler::build_response("SetAVTransportURI", "")
        }
        "Play" => {
            log::debug!("DLNA Play");
            let s = state.read().await;
            let has_pending =
                s.raw_pending_cast_request.is_some() || s.pending_cast_request.is_some();
            drop(s);
            if has_pending {
                let mut s = state.write().await;
                s.pending_play_queued = true;
            } else {
                let _ = command_tx.send(DlnaCommand::Play);
            }
            soap_handler::build_response("Play", "")
        }
        "Pause" => {
            let _ = command_tx.send(DlnaCommand::Pause);
            soap_handler::build_response("Pause", "")
        }
        "Stop" => {
            let _ = command_tx.send(DlnaCommand::Stop);
            soap_handler::build_response("Stop", "")
        }
        "Seek" => {
            let target = params.get("Target").cloned().unwrap_or_default();
            let pos_ms = soap_handler::parse_dlna_time_to_ms(&target);
            if pos_ms >= 0 {
                let _ = command_tx.send(DlnaCommand::Seek {
                    position_ms: pos_ms,
                });
            }
            soap_handler::build_response("Seek", "")
        }
        "GetTransportInfo" => {
            let s = state.read().await;
            soap_handler::build_transport_info_response(upnp_transport_state(s.playback_state))
        }
        "GetPositionInfo" => {
            let s = state.read().await;
            let pos = format_position_ms(s.current_position_ms);
            let dur = if s.duration_ms > 0 {
                format_position_ms(s.duration_ms)
            } else {
                "00:00:00".to_string()
            };
            soap_handler::build_position_info_response(&pos, &dur, &s.media_uri)
        }
        "GetMediaInfo" => soap_handler::build_media_info_response(),
        "GetDeviceCapabilities" => soap_handler::build_response(
            "GetDeviceCapabilities",
            "<PlayMedia>NETWORK</PlayMedia><RecMedia>NOT_IMPLEMENTED</RecMedia><RecQualityModes>NOT_IMPLEMENTED</RecQualityModes>",
        ),
        "SetPlayMode" => soap_handler::build_response("SetPlayMode", ""),
        _ => soap_handler::build_response(&action, ""),
    };

    http_ok(&response_body, "text/xml; charset=\"utf-8\"")
}

fn upnp_transport_state(state: DlnaPlaybackState) -> &'static str {
    match state {
        DlnaPlaybackState::Playing => "PLAYING",
        DlnaPlaybackState::PausedPlayback => "PAUSED_PLAYBACK",
        DlnaPlaybackState::Stopped => "STOPPED",
        DlnaPlaybackState::Transitioning => "TRANSITIONING",
        DlnaPlaybackState::NoMediaPresent => "NO_MEDIA_PRESENT",
    }
}

fn format_position_ms(ms: i64) -> String {
    let total_secs = ms / 1000;
    let h = total_secs / 3600;
    let m = (total_secs % 3600) / 60;
    let s = total_secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

fn http_ok(body: &str, content_type: &str) -> DlnaHttpResponse {
    DlnaHttpResponse {
        status: 200,
        content_type: Some(content_type.to_string()),
        headers: vec![],
        body: body.to_string(),
    }
}

fn http_ok_subscribe() -> DlnaHttpResponse {
    DlnaHttpResponse {
        status: 200,
        content_type: None,
        headers: vec![
            ("SID".to_string(), "uuid:dlna-plain-sub".to_string()),
            ("TIMEOUT".to_string(), "Second-3600".to_string()),
        ],
        body: String::new(),
    }
}
