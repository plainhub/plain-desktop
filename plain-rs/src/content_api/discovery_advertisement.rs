use super::server::ServerState;
use crate::chat::discovery_advertisement::{self as advertisement, Facts};
use anyhow::Result;
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
    Reply {},
    Mdns {},
    Ble {},
}
pub(super) async fn execute(state: &ServerState, request: Request) -> Result<Value> {
    let facts: Facts = serde_json::from_value(
        state
            .host
            .call("discoveryFacts", json!({}))
            .await
            .map_err(anyhow::Error::msg)?,
    )?;
    let id = state.prefs.get::<String>("client_id")?.unwrap_or_default();
    if matches!(request, Request::Ble {}) {
        return Ok(json!(crate::base64_encode(&advertisement::ble(
            &id,
            facts.aware_supported,
            facts.aware_running
        )?)));
    }
    let name = state
        .prefs
        .get_user::<String>("device_name")?
        .unwrap_or_default();
    let port = state.prefs.get_user::<u16>("https_port")?.unwrap_or(8443);
    let reply = advertisement::reply(&id, &name, port, facts)?;
    match request {
        Request::Reply {} => Ok(serde_json::to_value(reply)?),
        Request::Mdns {} => Ok(serde_json::to_value(advertisement::mdns(
            &reply,
            &state
                .prefs
                .get::<String>("mdns_hostname")?
                .unwrap_or_else(|| "plainapp.local".into()),
        )?)?),
        Request::Ble {} => unreachable!(),
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
    let result = tokio::select! { _=stop.changed()=>Err(anyhow::anyhow!("Server stopped")), result=execute(&state,request)=>result };
    match result {
        Ok(value) => Json(json!({"result":value})).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}
#[cfg(test)]
#[path = "../../tests/unit/content_api/discovery_advertisement.rs"]
mod tests;
