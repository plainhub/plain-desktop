use super::renderer_state::DlnaRendererState;
use crate::assembly::server::ServerState;
use serde::Deserialize;
use serde_json::{Value, json};

pub struct Controller {
    state: ServerState,
}

impl Controller {
    pub(crate) fn new(state: ServerState) -> Self {
        Self { state }
    }

    pub async fn execute_json(&self, command: &str) -> Result<String, String> {
        let request = serde_json::from_str(command).map_err(|error| error.to_string())?;
        let mut stop = self.state.stop.clone();
        if *stop.borrow() {
            return Err("Core stopped".into());
        }
        let result = tokio::select! {
            _ = stop.changed() => Err("Core stopped".to_string()),
            result = execute(&self.state, request) => result,
        }?;
        serde_json::to_string(&result).map_err(|error| error.to_string())
    }
}

pub(crate) const EVENT_UPDATED: &str = "DLNA_RENDERER_UPDATED";

pub(crate) fn snapshot(state: &DlnaRendererState) -> Value {
    json!({
        "version": state.version,
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

/// Pushes the renderer state *in the event*, so the app renders it directly
/// instead of being told to go and fetch it over HTTP.
pub(crate) fn emit(state: &ServerState, renderer: &DlnaRendererState) {
    let _ = state.events.send(plain_rs::events::WsEvent::broadcast(
        EVENT_UPDATED,
        json!({"renderer": snapshot(renderer)}).to_string(),
    ));
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Request {
    Start {
        port: u16,
    },
    Stop {},
    Retry {
        port: u16,
    },
    Snapshot {},
    Rules {},
    RemoveRule {
        ip: String,
        allowed: bool,
    },
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
        state: crate::services::casting::receiver::types::DlnaPlaybackState,
    },
    Stopped {},
}

pub(crate) async fn execute(state: &ServerState, request: Request) -> Result<Value, String> {
    let _guard = state.cast.receiver_operations.lock().await;
    let engine = &state.dlna;
    match request {
        Request::Rules {} => Ok(rules(state)),
        Request::RemoveRule { ip, allowed } => {
            plain_rs::prefs::dlna::remove_sender(
                &state.prefs,
                if allowed {
                    "dlna_allowed_senders"
                } else {
                    "dlna_denied_senders"
                },
                &ip,
            );
            Ok(rules(state))
        }
        Request::Start { port } => {
            crate::services::casting::dlna_sender_runtime::acquire_permission(
                state,
                &state.cast.receiver_lease,
            )
            .await?;
            engine.start(port).await;
            if !engine.snapshot().await.is_running {
                crate::services::casting::dlna_sender_runtime::release_permission(
                    state,
                    &state.cast.receiver_lease,
                )
                .await;
            }
            Ok(snapshot(&engine.snapshot().await))
        }
        Request::Stop {} => {
            engine.stop().await;
            crate::services::casting::dlna_sender_runtime::release_permission(
                state,
                &state.cast.receiver_lease,
            )
            .await;
            Ok(snapshot(&engine.snapshot().await))
        }
        Request::Retry { port } => {
            // Restarting from the engine's own scope would self-cancel: the
            // stop path aborts the tasks this command is running on.
            engine.set_retrying(true).await;
            engine.stop().await;
            crate::services::casting::dlna_sender_runtime::release_permission(
                state,
                &state.cast.receiver_lease,
            )
            .await;
            crate::services::casting::dlna_sender_runtime::acquire_permission(
                state,
                &state.cast.receiver_lease,
            )
            .await?;
            engine.start(port).await;
            if !engine.snapshot().await.is_running {
                crate::services::casting::dlna_sender_runtime::release_permission(
                    state,
                    &state.cast.receiver_lease,
                )
                .await;
            }
            engine.set_retrying(false).await;
            Ok(snapshot(&engine.snapshot().await))
        }
        Request::Snapshot {} => Ok(snapshot(&engine.snapshot().await)),
        Request::Accept { remember } => {
            let before = engine.snapshot().await.pending_cast_request;
            engine.accept_cast(remember, &state.prefs).await;
            let after = engine.snapshot().await;
            if before.is_some() {
                emit(state, &after);
            }
            Ok(snapshot(&after))
        }
        Request::Reject { remember } => {
            let before = engine.snapshot().await.pending_cast_request;
            engine.reject_cast(remember, &state.prefs).await;
            let after = engine.snapshot().await;
            if before.is_some() {
                emit(state, &after);
            }
            Ok(snapshot(&after))
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

pub(crate) fn follow_changes(state: &ServerState) {
    let mut changes = state.dlna.changes();
    let state = state.clone();
    tokio::spawn(async move {
        let mut stop = state.stop.clone();
        loop {
            if *stop.borrow() {
                break;
            }
            tokio::select! {
                _ = stop.changed() => break,
                result = changes.recv() => {
                    if matches!(result, Err(tokio::sync::broadcast::error::RecvError::Closed)) { break; }
                    let renderer = state.dlna.snapshot().await;
                    emit(&state, &renderer);
                }
            }
        }
    });
}

fn rules(state: &ServerState) -> Value {
    let entries = |key| {
        plain_rs::prefs::dlna::senders(&state.prefs, key)
            .into_iter()
            .map(|entry| {
                let (ip, name) = plain_rs::prefs::dlna::decode_sender_entry(&entry);
                json!({"ip":ip,"name":name})
            })
            .collect::<Vec<_>>()
    };
    json!({"allowed":entries("dlna_allowed_senders"),"denied":entries("dlna_denied_senders")})
}
