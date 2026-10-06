use super::server::ServerState;
use crate::{db::Db, prefs::Prefs, utils::search_dsl};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
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

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_lowercase()
}
fn instant(value: &Value, key: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    value[key]
        .as_str()
        .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
}
pub(super) fn packages(
    db: &Db,
    mut facts: Vec<Value>,
    query: &str,
    sort_by: &str,
) -> anyhow::Result<Vec<Value>> {
    let groups = search_dsl::parse(query);
    let first = |key| {
        groups
            .iter()
            .find(|g| g.name == key)
            .map(|g| g.value.as_str())
            .unwrap_or_default()
    };
    let filter_type = first("type");
    let filter_text = first("text").to_lowercase();
    let filter_ids = first("ids")
        .split(',')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    let tag_ids = first("tag_id")
        .split(',')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    let tagged = if tag_ids.is_empty() {
        None
    } else {
        let mut ids = std::collections::HashSet::new();
        for tag in tag_ids {
            ids.extend(crate::db::tag::keys_for_tag(db, tag)?);
        }
        Some(ids)
    };
    facts.retain(|fact| {
        let item = &fact["item"];
        let id = item["id"].as_str().unwrap_or_default();
        (filter_type.is_empty() || item["type"].as_str() == Some(filter_type))
            && (filter_ids.is_empty() || filter_ids.contains(&id))
            && tagged.as_ref().is_none_or(|ids| ids.contains(id))
            && (filter_text.is_empty()
                || text(item, "id").contains(&filter_text)
                || text(item, "name").contains(&filter_text)
                || item["certs"].as_array().is_some_and(|certs| {
                    certs.iter().any(|c| {
                        text(c, "issuer").contains(&filter_text)
                            || text(c, "subject").contains(&filter_text)
                    })
                }))
    });
    facts.sort_by(|a, b| match sort_by {
        "SIZE_ASC" => a["item"]["size"].as_i64().cmp(&b["item"]["size"].as_i64()),
        "SIZE_DESC" => b["item"]["size"].as_i64().cmp(&a["item"]["size"].as_i64()),
        "DATE_ASC" => instant(&a["item"], "updatedAt").cmp(&instant(&b["item"], "updatedAt")),
        "DATE_DESC" => instant(&b["item"], "updatedAt").cmp(&instant(&a["item"], "updatedAt")),
        _ => a["nameSortKey"].as_str().cmp(&b["nameSortKey"].as_str()),
    });
    Ok(facts.into_iter().map(|v| v["item"].clone()).collect())
}
pub(super) fn notifications(prefs: &Prefs, mut facts: Vec<Value>, query: &str) -> Vec<Value> {
    let groups = search_dsl::parse(query);
    let filter_text = groups
        .iter()
        .find(|g| g.name == "text")
        .map(|g| g.value.trim().to_lowercase())
        .unwrap_or_default();
    let filter = prefs.get_user_or::<String>("notification_filter", String::new());
    let filter: Value =
        serde_json::from_str(&filter).unwrap_or_else(|_| json!({"mode":"blacklist","apps":[]}));
    facts.retain(|item| {
        let listed = filter["apps"]
            .as_array()
            .is_some_and(|apps| apps.contains(&item["appId"]));
        let allowed = match filter["mode"].as_str() {
            Some("allowlist") => listed,
            Some("blacklist") => !listed,
            _ => true,
        };
        allowed
            && (filter_text.is_empty()
                || ["appName", "title", "body"]
                    .into_iter()
                    .any(|key| text(item, key).contains(&filter_text)))
    });
    facts.sort_by(|a, b| instant(b, "time").cmp(&instant(a, "time")));
    facts
}
pub(super) fn page(items: Vec<Value>, offset: i64, limit: i64) -> Vec<Value> {
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
        packages(&state.db, facts, &request.query, &request.sort_by)?
    } else {
        notifications(&state.prefs, facts, &request.query)
    };
    Ok(if is_count {
        json!({"count":items.len()})
    } else {
        json!({"items":page(items,request.offset,request.limit)})
    })
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
#[path = "../../tests/unit/content_api/system_providers.rs"]
mod tests;
