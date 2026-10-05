use super::server::ServerState;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
    data_type: String,
    items: Vec<ItemFact>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ItemFact {
    id: String,
    name: String,
    size: i64,
    path: String,
    sort_name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Bucket {
    id: String,
    name: String,
    item_count: u64,
    size: i64,
    top_items: Vec<String>,
    sort_name: String,
}

fn aggregate(data_type: &str, items: Vec<ItemFact>) -> anyhow::Result<Vec<Bucket>> {
    anyhow::ensure!(
        matches!(data_type, "IMAGE" | "VIDEO" | "AUDIO" | "DOC"),
        "unsupported media bucket type"
    );
    let mut positions = HashMap::<String, usize>::new();
    let mut buckets = Vec::<Bucket>::new();
    for item in items {
        anyhow::ensure!(item.size >= 0, "invalid media bucket item size");
        if let Some(&index) = positions.get(&item.id) {
            let bucket = &mut buckets[index];
            anyhow::ensure!(bucket.name == item.name, "inconsistent media bucket name");
            bucket.item_count = bucket
                .item_count
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("media bucket item count overflow"))?;
            bucket.size = bucket
                .size
                .checked_add(item.size)
                .ok_or_else(|| anyhow::anyhow!("media bucket size overflow"))?;
            if bucket.top_items.len() < 4 {
                bucket.top_items.push(item.path);
            }
        } else {
            positions.insert(item.id.clone(), buckets.len());
            buckets.push(Bucket {
                id: item.id,
                name: item.name,
                item_count: 1,
                size: item.size,
                top_items: vec![item.path],
                sort_name: item.sort_name.to_lowercase(),
            });
        }
    }
    buckets.sort_by(|a, b| a.sort_name.cmp(&b.sort_name));
    Ok(buckets)
}

async fn execute(request: Request) -> anyhow::Result<Value> {
    Ok(json!({"items": aggregate(&request.data_type, request.items)?}))
}

pub(super) async fn call(
    State(_state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !_state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match execute(request).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": error.to_string()})),
        )
            .into_response(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/media_buckets.rs"]
mod tests;
