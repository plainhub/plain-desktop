use super::server::ServerState;
use crate::{prefs::Prefs, ws_event::WsEvent};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::broadcast;

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub(super) enum Command {
    Snapshot {
        user: bool,
    },
    Set {
        user: bool,
        key: String,
        value: Value,
    },
    Remove {
        user: bool,
        key: String,
    },
}

pub(super) fn set(
    prefs: &Prefs,
    events: &broadcast::Sender<WsEvent>,
    user: bool,
    key: &str,
    value: Value,
) -> Result<bool, String> {
    let changed = if user {
        prefs.set_user(key, value)
    } else {
        prefs.set(key, value)
    }
    .map_err(|e| e.to_string())?;
    if changed {
        let _ = events.send(WsEvent::broadcast(10010, "{}".into()));
    }
    Ok(changed)
}

pub(super) fn remove(
    prefs: &Prefs,
    events: &broadcast::Sender<WsEvent>,
    user: bool,
    key: &str,
) -> Result<bool, String> {
    let changed = if user {
        prefs.remove_user(key)
    } else {
        prefs.remove(key)
    }
    .map_err(|e| e.to_string())?;
    if changed {
        let _ = events.send(WsEvent::broadcast(10010, "{}".into()));
    }
    Ok(changed)
}

pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<Command>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let result = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        match command {
            Command::Snapshot { user } => Ok(Value::Object(
                if user {
                    state.prefs.user_entries()
                } else {
                    state.prefs.entries()
                }
                .into_iter()
                .collect(),
            )),
            Command::Set { user, key, value } => {
                set(&state.prefs, &state.events, user, &key, value)
                    .map(|changed| json!({"changed": changed}))
            }
            Command::Remove { user, key } => remove(&state.prefs, &state.events, user, &key)
                .map(|changed| json!({"changed": changed})),
        }
    })
    .await;
    match result {
        Ok(Ok(value)) => Json(value).into_response(),
        Ok(Err(error)) => (StatusCode::BAD_REQUEST, Json(json!({"error": error}))).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": error.to_string()})),
        )
            .into_response(),
    }
}
