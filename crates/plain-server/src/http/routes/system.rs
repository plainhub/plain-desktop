use crate::assembly::server::ServerState;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use plain_rs::db::Db;
use plain_rs::prefs::Prefs;
use plain_rs::utils::search_dsl;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Request {
    action: Action,
    query: String,
    offset: i64,
    limit: i64,
    sort_by: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum Action {
    Packages,
    PackageCount,
    Notifications,
    NotificationCount,
}

pub(crate) fn page(items: Vec<Value>, offset: i64, limit: i64) -> Vec<Value> {
    items
        .into_iter()
        .skip(offset.max(0) as usize)
        .take(limit.max(0) as usize)
        .collect()
}
async fn execute(state: &ServerState, request: Request) -> anyhow::Result<Value> {
    let (method, is_count) = match request.action {
        Action::Packages => ("systemPackageFacts", false),
        Action::PackageCount => ("systemPackageFacts", true),
        Action::Notifications => ("systemNotificationFacts", false),
        Action::NotificationCount => ("systemNotificationFacts", true),
    };
    let facts = state
        .host
        .call(method, json!({}))
        .await
        .map_err(anyhow::Error::msg)?;
    let facts: Vec<Value> = serde_json::from_value(facts)?;
    let items = if method == "systemPackageFacts" {
        #[cfg(feature = "mobile")]
        {
            crate::mobile::packages::query::packages(
                &state.db,
                facts,
                &request.query,
                &request.sort_by,
            )?
        }
        #[cfg(not(feature = "mobile"))]
        {
            anyhow::bail!("unsupported provider")
        }
    } else {
        crate::services::notifications::query::notifications(&state.prefs, facts, &request.query)?
    };
    Ok(if is_count {
        json!({"count":items.len()})
    } else {
        json!({"items":page(items,request.offset,request.limit)})
    })
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
#[cfg(all(test, feature = "mobile"))]
#[path = "../../../tests/unit/http/routes/system.rs"]
mod tests;
