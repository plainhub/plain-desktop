use super::{
    host::Host, image_models::Runtime, public_image_index::ImageSearchStatusType,
    server::ServerState,
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashSet;

fn enabled(models: &Runtime, text: &str) -> bool {
    !text.trim().is_empty() && models.snapshot().status.status == ImageSearchStatusType::Ready
}
async fn filename_rows(
    host: &Host,
    query: &str,
    offset: i32,
    limit: i32,
    sort: &str,
) -> Result<Vec<Value>, String> {
    let rows = host
        .call(
            "systemMediaRows",
            json!({"dataType":"IMAGE", "query":query,"offset":offset,"limit":limit,"sortBy":sort}),
        )
        .await?;
    serde_json::from_value(rows).map_err(|e| e.to_string())
}
pub(super) async fn rows(
    host: &Host,
    models: &Runtime,
    text: &str,
    query: &str,
    offset: i32,
    limit: i32,
    sort: &str,
) -> Result<Vec<Value>, String> {
    if offset < 0 || limit < 0 {
        return Err("Invalid image pagination".into());
    }
    if !enabled(models, text) {
        return filename_rows(host, query, offset, limit, sort).await;
    }
    let semantic = models.search(text, 50).await?;
    let mut items = filename_rows(host, query, 0, i32::MAX, sort).await?;
    let ids = items
        .iter()
        .filter_map(|row| row["id"].as_str())
        .collect::<HashSet<_>>();
    let extra = semantic
        .into_iter()
        .map(|row| row.image_id)
        .filter(|id| !ids.contains(id.as_str()))
        .collect::<Vec<_>>();
    if !extra.is_empty() {
        let query = format!("ids:{} trash:false", extra.join(","));
        let mut found = filename_rows(host, &query, 0, extra.len() as i32, sort).await?;
        found.sort_by_key(|row| {
            extra
                .iter()
                .position(|id| Some(id.as_str()) == row["id"].as_str())
                .unwrap_or(usize::MAX)
        });
        items.extend(found);
    }
    Ok(items
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect())
}
pub(super) async fn count(
    host: &Host,
    models: &Runtime,
    text: &str,
    query: &str,
) -> Result<i32, String> {
    if !enabled(models, text) {
        return host
            .call(
                "systemMediaCount",
                json!({"dataType":"IMAGE","query":query}),
            )
            .await?
            .as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .ok_or("Invalid image count".into());
    }
    let semantic = models.search(text, 50).await?;
    let ids: Vec<String> = serde_json::from_value(
        host.call("systemImageIdsFacts", json!({"query":query}))
            .await?,
    )
    .map_err(|e| e.to_string())?;
    let mut ids = ids.into_iter().collect::<HashSet<_>>();
    ids.extend(semantic.into_iter().map(|row| row.image_id));
    i32::try_from(ids.len()).map_err(|e| e.to_string())
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub(super) enum Command {
    Rows {
        text: String,
        query: String,
        offset: i32,
        limit: i32,
        #[serde(rename = "sortBy")]
        sort_by: String,
    },
    Count {
        text: String,
        query: String,
    },
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<Command>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let result = match command {
        Command::Rows {
            text,
            query,
            offset,
            limit,
            sort_by,
        } => rows(
            &state.host,
            &state.image_models,
            &text,
            &query,
            offset,
            limit,
            &sort_by,
        )
        .await
        .map(|items| json!({"items":items})),
        Command::Count { text, query } => count(&state.host, &state.image_models, &text, &query)
            .await
            .map(|count| json!({"count":count})),
    };
    match result {
        Ok(value) => Json(value).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, Json(json!({"error":error}))).into_response(),
    }
}
