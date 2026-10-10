use super::server::ServerState;
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
    Begin {
        message_id: String,
        id: String,
        uri: String,
    },
    Finish {
        token: String,
    },
    Abort {
        token: String,
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
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<serde_json::Value> {
        Ok(match request {
            Request::Begin {message_id,id,uri} => serde_json::to_value(state.attachments.begin(&state.db,&state.directory,&message_id,&id,&uri)?)?,
            Request::Finish {token} => {
                let result = state.attachments.finish(&state.db,&state.directory,&token)?;
                json!({"uri":format!("fid:{}",result.fid_suffix),"path":result.real_path,"chat":result.chat})
            },
            Request::Abort {token} => json!(state.attachments.abort(&token)?),
        })
    }).await;
    match result {
        Ok(Ok(value)) => Json(json!({"result":value})).into_response(),
        error => (StatusCode::BAD_REQUEST,Json(json!({"error":match error {Ok(Err(e))=>e.to_string(),Err(e)=>e.to_string(),_=>unreachable!()}}))).into_response(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/attachment_imports.rs"]
mod tests;
