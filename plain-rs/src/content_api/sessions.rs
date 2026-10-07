use super::server::ServerState;
use crate::db::{Db, SessionRow};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Request {
    List,
    Key {
        #[serde(rename = "clientId")]
        client_id: String,
    },
    Create {
        name: String,
    },
    Rename {
        #[serde(rename = "clientId")]
        client_id: String,
        name: String,
    },
    Delete {
        #[serde(rename = "clientId")]
        client_id: String,
    },
}

pub(super) fn key(db: &Db, client_id: &str) -> anyhow::Result<Option<Vec<u8>>> {
    Ok(db.session_get(client_id)?.and_then(|row| {
        let key = crate::base64_decode(&row.token);
        (key.len() == 32).then_some(key)
    }))
}

pub(super) fn touch(state: &ServerState, client_id: &str) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp_millis();
    state
        .db
        .session_touch(&[(client_id.to_owned(), now.to_string())])?;
    let _ = state.events.send(crate::ws_event::WsEvent::broadcast(
        10005,
        json!({"clientId":client_id,"time":now}).to_string(),
    ));
    Ok(())
}

pub(super) fn execute(state: &ServerState, request: Request) -> anyhow::Result<Value> {
    Ok(match request {
        Request::List => serde_json::to_value(state.db.session_list()?)?,
        Request::Key { client_id } => {
            json!(key(&state.db, &client_id)?.map(|key| crate::base64_encode(&key)))
        }
        Request::Create { name } => {
            let now = chrono::Utc::now().timestamp_millis().to_string();
            let row = SessionRow {
                client_id: uuid::Uuid::new_v4().to_string(),
                name,
                r#type: "CUSTOM".into(),
                client_ip: String::new(),
                os_name: String::new(),
                os_version: String::new(),
                browser_name: String::new(),
                browser_version: String::new(),
                token: crate::base64_encode(&crate::crypto::random_bytes(32)),
                last_active_at: None,
                created_at: now.clone(),
                updated_at: now,
            };
            state.db.session_save(&row)?;
            serde_json::to_value(row)?
        }
        Request::Rename { client_id, name } => json!(
            state.db.with_conn(|c| c.execute(
                "UPDATE sessions SET name=?2, updated_at=?3 WHERE client_id=?1",
                rusqlite::params![
                    client_id,
                    name,
                    chrono::Utc::now().timestamp_millis().to_string()
                ]
            ))? == 1
        ),
        Request::Delete { client_id } => {
            state
                .login_attempts
                .lock()
                .map_err(|_| anyhow::anyhow!("Login state unavailable"))?
                .cancel_client(&client_id);
            json!(state.db.session_delete(&client_id)? > 0)
        }
    })
}

pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match execute(&state, request) {
        Ok(value) => Json(json!({"result":value})).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}

#[cfg(all(test, feature = "http_transport"))]
#[path = "../../tests/unit/content_api/sessions.rs"]
mod tests;
