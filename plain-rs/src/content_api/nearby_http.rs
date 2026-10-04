use super::server::ServerState;
use crate::chat::nearby_http::{self, Message};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Request {
    ScanBegin,
    ScanSeen {
        id: String,
        short_id: String,
    },
    ScanReply {
        id: String,
        short_id: String,
        generation: String,
        payload: Option<String>,
    },
    ScanEnd {
        id: String,
    },
    Encode {
        message: Message,
    },
    Parse {
        body: String,
    },
    DiscoverReply {
        payload: String,
        short_id: String,
    },
    Send {
        ip: String,
        port: u16,
        message: Message,
    },
    Probe {
        ip: String,
        port: u16,
    },
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let result = async {
        Ok::<_, anyhow::Error>(match request {
            Request::ScanBegin => json!(state.ble_scans.begin()?),
            Request::ScanSeen { id, short_id } => {
                serde_json::to_value(state.ble_scans.seen(&id, &short_id)?)?
            }
            Request::ScanReply {
                id,
                short_id,
                generation,
                payload,
            } => serde_json::to_value(state.ble_scans.reply(
                &id,
                &short_id,
                &generation,
                payload.as_deref(),
            )?)?,
            Request::ScanEnd { id } => {
                state.ble_scans.end(&id);
                json!(true)
            }
            Request::Encode { message } => json!(message.wire()?),
            Request::Parse { body } => serde_json::to_value(Message::parse(&body)?)?,
            Request::DiscoverReply { payload, short_id } => serde_json::to_value(
                crate::chat::nearby_wire::discover_reply(&payload, &short_id)?,
            )?,
            Request::Send { ip, port, message } => {
                json!(nearby_http::send(&ip, port, &message).await?)
            }
            Request::Probe { ip, port } => json!(nearby_http::probe(&ip, port).await?),
        })
    }
    .await;
    match result {
        Ok(value) => Json(json!({"result":value})).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}
