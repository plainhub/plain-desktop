use super::server::ServerState;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

const PERMISSIONS: &[&str] = &[
    "WRITE_EXTERNAL_STORAGE",
    "READ_SMS",
    "SEND_SMS",
    "READ_CONTACTS",
    "WRITE_CONTACTS",
    "READ_CALL_LOG",
    "WRITE_CALL_LOG",
    "CALL_PHONE",
    "POST_NOTIFICATIONS",
    "NEARBY_WIFI_DEVICES",
    "ACCESS_FINE_LOCATION",
    "CAMERA",
    "SYSTEM_ALERT_WINDOW",
    "RECORD_AUDIO",
    "READ_MEDIA_IMAGES",
    "READ_MEDIA_VIDEOS",
    "READ_MEDIA_AUDIO",
    "NOTIFICATION_LISTENER",
    "READ_PHONE_STATE",
    "READ_PHONE_NUMBERS",
    "SCHEDULE_EXACT_ALARM",
    "QUERY_ALL_PACKAGES",
    "ADB",
    "CLIPBOARD",
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
    permissions: Vec<String>,
    #[serde(default)]
    require_granted: bool,
}

async fn execute(state: &ServerState, request: Request) -> anyhow::Result<Value> {
    anyhow::ensure!(!request.permissions.is_empty(), "permissions is empty");
    anyhow::ensure!(
        request
            .permissions
            .iter()
            .all(|name| PERMISSIONS.contains(&name.as_str())),
        "unknown permission"
    );
    let configured: Vec<String> = state.prefs.get_or("api_permissions", Vec::new());
    let configured = configured
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let enabled = super::public_gate::api_enabled(&request.permissions, &configured);
    if !enabled || !request.require_granted {
        return Ok(json!({"allowed": enabled}));
    }
    let granted = super::public_gate::granted(&state.host, &request.permissions).await?;
    Ok(json!({"allowed":granted}))
}

pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match execute(&state, request).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/system_permissions.rs"]
mod tests;
