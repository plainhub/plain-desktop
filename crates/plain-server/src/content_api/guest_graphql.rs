use super::server::ServerState;
use crate::{content_types::Long, shares::Service};
use async_graphql::{Context, EmptyMutation, EmptySubscription, Object, Schema, SimpleObject};
use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(SimpleObject)]
struct SharedFile {
    name: String,
    virtual_path: String,
    is_dir: bool,
    size: Long,
    mime_type: String,
    has_thumb: bool,
}
#[derive(SimpleObject)]
struct SharedInfo {
    name: String,
    read_only: bool,
    requires_password: bool,
    expires_at: Option<Long>,
    url_token: String,
    entries: Vec<SharedFile>,
}
struct Guest {
    state: ServerState,
    id: String,
}
struct Query;
#[Object]
impl Query {
    async fn shared_info(
        &self,
        ctx: &Context<'_>,
        virtual_path: Option<String>,
    ) -> async_graphql::Result<SharedInfo> {
        let guest = ctx.data::<Guest>()?;
        let service = Service::new(guest.state.db.clone(), guest.state.prefs.clone());
        let id = guest.id.clone();
        let path = virtual_path.unwrap_or_default();
        let (share, roots) = tokio::task::spawn_blocking(move || service.browse(&id, &path))
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let mut entries = Vec::new();
        for root in roots {
            let name = root
                .virtual_path
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_owned();
            let ext = name
                .rsplit('.')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            let has_thumb = !root.is_dir
                && [
                    "jpg", "png", "jpeg", "bmp", "webp", "heic", "heif", "apng", "avif", "gif",
                    "tiff", "tif", "svg",
                ]
                .contains(&ext.as_str());
            let metadata = if root.is_dir {
                Value::Null
            } else {
                guest
                    .state
                    .host
                    .call("fileMetadataFacts", json!({"path":root.real_path}))
                    .await
                    .map_err(async_graphql::Error::new)?
            };
            entries.push(SharedFile {
                name,
                virtual_path: root.virtual_path,
                is_dir: root.is_dir,
                size: Long(metadata["size"].as_i64().unwrap_or(0)),
                mime_type: metadata["mimeType"].as_str().unwrap_or_default().to_owned(),
                has_thumb,
            });
        }
        Ok(SharedInfo {
            name: share.name,
            read_only: share.read_only,
            requires_password: !share.password.is_empty(),
            expires_at: share
                .expires_at
                .as_deref()
                .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                .map(|t| Long(t.timestamp_millis())),
            url_token: share.url_token,
            entries,
        })
    }
}
async fn execute(state: &ServerState, id: &str, body: &[u8]) -> (u16, Vec<u8>) {
    if id.is_empty() {
        return (401, vec![]);
    }
    let shares = Service::new(state.db.clone(), state.prefs.clone());
    if !matches!(shares.active(id, true), Ok(Some(_))) {
        return (403, vec![]);
    }
    let key = match shares
        .token(id)
        .and_then(|key| crate::utils::base64::base64_decode_checked(&key).map_err(Into::into))
    {
        Ok(key) => key,
        Err(_) => return (500, vec![]),
    };
    let plain = match crate::xchacha_decrypt_raw(&key, body).and_then(|p| String::from_utf8(p).ok())
    {
        Some(p) if !p.is_empty() => p,
        _ => return (401, vec![]),
    };
    let now = chrono::Utc::now().timestamp_millis();
    let body = match state.guest_replay.admit(id, &plain, now) {
        Ok(p) => p,
        Err(_) => return (400, vec![]),
    };
    let request: async_graphql::Request = match serde_json::from_str(body) {
        Ok(p) => p,
        Err(_) => return (400, vec![]),
    };
    let _ = state.events.send(crate::ws_event::WsEvent::broadcast(
        "WEB_REQUEST_RECEIVED",
        json!({"clientId":id,"time":now}).to_string(),
    ));
    let schema = Schema::build(Query, EmptyMutation, EmptySubscription)
        .data(Guest {
            state: state.clone(),
            id: id.to_owned(),
        })
        .finish();
    let result = schema.execute(request).await;
    match serde_json::to_vec(&result)
        .ok()
        .and_then(|p| crate::xchacha_encrypt_raw(&key, &p))
    {
        Some(body) => (200, body),
        None => (500, vec![]),
    }
}
pub(super) async fn public(
    State(state): State<ServerState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let id = headers
        .get("c-id")
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default();
    let (status, body) = execute(&state, id, &body).await;
    (
        StatusCode::from_u16(status).unwrap(),
        [("content-type", "application/octet-stream")],
        body,
    )
        .into_response()
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
    client_id: String,
    body: String,
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let (status, body) = execute(
        &state,
        &request.client_id,
        &crate::base64_decode(&request.body),
    )
    .await;
    Json(json!({"status":status,"body":crate::base64_encode(&body)})).into_response()
}
#[cfg(test)]
#[path = "../../tests/unit/content_api/guest_graphql.rs"]
mod tests;
