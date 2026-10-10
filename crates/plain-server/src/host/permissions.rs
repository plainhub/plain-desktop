use crate::assembly::server::ServerState;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

use plain_rs::prefs::system::PERMISSIONS;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Request {
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
    let configured: Vec<String> = state
        .prefs
        .get_or(plain_rs::prefs::system::keys::API_PERMISSIONS, Vec::new());
    let configured = configured
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let enabled = crate::http::auth::public_gate::api_enabled(&request.permissions, &configured);
    if !enabled || !request.require_granted {
        return Ok(json!({"allowed": enabled}));
    }
    let granted =
        crate::http::auth::public_gate::granted(&state.host, &request.permissions).await?;
    Ok(json!({"allowed":granted}))
}

pub(crate) async fn call(
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
#[path = "../../tests/unit/host/permissions.rs"]
mod tests;
