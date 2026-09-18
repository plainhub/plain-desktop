use serde_json::{Value, json};
use std::sync::Arc;

use super::context::AppCtx;
use super::schema::LocalSchema;

/// Execute a decrypted GraphQL request through the typed schema.
/// Unsupported operations are short-circuited as empty stubs before reaching the schema.
pub async fn execute_graphql(schema: &LocalSchema, request: Value, ctx: Arc<AppCtx>) -> Value {
    if let Some(stub) = stub_response(&request) {
        return stub;
    }

    let query_str = request
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let vars: async_graphql::Variables = request
        .get("variables")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let response = schema
        .execute(
            async_graphql::Request::new(query_str)
                .variables(vars)
                .data(ctx),
        )
        .await;

    serde_json::to_value(response).unwrap_or_else(|_| json!({ "data": null }))
}

// ── Stub pre-filter ───────────────────────────────────────────────────────────
// Operations that are always unsupported in local mode are returned immediately
// so the schema never sees field names it doesn't define (avoids spurious errors).

fn op_name(request: &Value) -> &str {
    request
        .get("query")
        .and_then(Value::as_str)
        .and_then(|q| {
            let mut words = q.split_whitespace();
            while let Some(w) = words.next() {
                if w == "query" || w == "mutation" {
                    return words
                        .next()
                        .map(|n| n.split(['(', '{']).next().unwrap_or(""));
                }
            }
            None
        })
        .unwrap_or("")
}

fn stub_response(request: &Value) -> Option<Value> {
    let data = match op_name(request) {
        "images" => json!({ "images": [],  "imageCount": 0 }),
        "videos" => json!({ "videos": [],  "videoCount": 0 }),
        "audios" => json!({ "items": [],   "total": 0 }),
        "audioPlaylist" => json!({ "audioPlaylist": { "items": [], "total": 0 } }),
        "docs" => json!({ "items": [],   "total": 0 }),
        "files" => json!({ "files": [] }),
        "recentFiles" => json!({ "recentFiles": [] }),
        "notes" => json!({ "notes": [],   "noteCount": 0 }),
        "feeds" => json!({ "feeds": [] }),
        "feedEntries" => json!({ "items": [],   "total": 0 }),
        "feedsTags" => json!({ "tags": [],    "feeds": [] }),
        "bucketsTags" => json!({ "tags": [],    "mediaBuckets": [] }),
        _ => return None,
    };
    Some(json!({ "data": data }))
}
