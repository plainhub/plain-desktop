use plain_rs::{prefs::Prefs, utils::search_dsl};
use serde_json::{Value, json};

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_lowercase()
}
fn instant(value: &Value, key: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    value[key]
        .as_str()
        .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
}
pub(crate) fn notifications(
    prefs: &Prefs,
    mut facts: Vec<Value>,
    query: &str,
) -> anyhow::Result<Vec<Value>> {
    let groups = search_dsl::parse(query);
    let filter_text = groups
        .iter()
        .find(|g| g.name == "text")
        .map(|g| g.value.trim().to_lowercase())
        .unwrap_or_default();
    let filter = plain_rs::prefs::user::snapshot(prefs)?.notification_filter;
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
    Ok(facts)
}
