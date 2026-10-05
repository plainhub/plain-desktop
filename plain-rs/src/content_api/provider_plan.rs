use super::server::ServerState;
use crate::{
    db::Db,
    utils::search_dsl::{self, FilterField},
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(super) enum Provider {
    Call,
    Contact,
    Audio,
    Video,
    Image,
    Doc,
    File,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
    provider: Provider,
    query: String,
    #[serde(default)]
    resolved_parent_id: Option<String>,
}
#[derive(Default, Serialize)]
pub(super) struct Plan {
    pub clauses: Vec<String>,
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ids_column: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trash: Option<bool>,
}
impl Plan {
    pub(super) fn add(&mut self, clause: String, values: Vec<String>) {
        self.clauses.push(clause);
        self.args.extend(values);
    }
    pub(super) fn equal(&mut self, column: &str, value: &str) {
        self.add(format!("{column} = ?"), vec![value.to_owned()]);
    }
    pub(super) fn contains(&mut self, column: &str, value: &str) {
        let value = value
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        self.add(
            format!("{column} LIKE '%' || ? || '%' ESCAPE '\\'"),
            vec![value],
        );
    }
    pub(super) fn ids(&mut self, column: &str, ids: Vec<String>) {
        if !ids.is_empty() {
            self.ids_column = Some(column.to_owned());
            self.ids = ids;
        }
    }
}
pub(super) fn fields(db: &Db, query: &str) -> anyhow::Result<Vec<FilterField>> {
    let mut fields = search_dsl::parse(query);
    let tag_ids = fields
        .iter()
        .filter(|f| f.name == "tag_id")
        .map(|f| f.value.clone())
        .collect::<std::collections::HashSet<_>>();
    fields.retain(|f| f.name != "tag_id");
    if !tag_ids.is_empty() {
        let mut ids = std::collections::BTreeSet::new();
        for id in tag_ids {
            ids.extend(crate::db::tag::keys_for_tag(db, &id)?);
        }
        fields.push(FilterField {
            name: "ids".into(),
            op: "=".into(),
            value: if ids.is_empty() {
                "invalid_ids".into()
            } else {
                ids.into_iter().collect::<Vec<_>>().join(",")
            },
        });
    }
    Ok(fields)
}
fn comparison(field: &FilterField) -> (&str, &str) {
    let mut value = field.value.trim();
    let mut op = field.op.as_str();
    if matches!(op, "" | ":" | "=") {
        op = "=";
        for prefix in [">=", "<=", "!=", ">", "<", "="] {
            if let Some(rest) = value.strip_prefix(prefix) {
                op = prefix;
                value = rest.trim();
                break;
            }
        }
    } else if let Some(rest) = value.strip_prefix(op) {
        value = rest.trim();
    }
    (op, value)
}
fn plan(
    provider: Provider,
    fields: &[FilterField],
    dates: &Value,
    resolved_parent_id: Option<&str>,
    query_is_empty: bool,
) -> Plan {
    let mut plan = Plan::default();
    if matches!(provider, Provider::Contact) {
        plan.equal("mimetype", "vnd.android.cursor.item/name");
    }
    if matches!(provider, Provider::Doc) {
        plan.add(
            "(mime_type LIKE ? OR mime_type IN (?,?,?,?,?))".into(),
            vec![
                "text/%".into(),
                "application/pdf".into(),
                "application/msword".into(),
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document".into(),
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".into(),
                "application/javascript".into(),
            ],
        );
        plan.add("size > 0".into(), vec![]);
    }
    if matches!(provider, Provider::File) && !query_is_empty {
        let show_hidden = fields
            .iter()
            .any(|field| field.name == "show_hidden" && field.value.eq_ignore_ascii_case("true"));
        if !show_hidden {
            plan.add(
                "_display_name NOT LIKE ? ESCAPE '\\'".into(),
                vec![".%".into()],
            );
        }
    }
    for field in fields {
        match (provider, field.name.as_str()) {
            (Provider::Call, "text") => plan.contains("number", &field.value),
            (Provider::Contact, "text") => plan.contains("data2", &field.value),
            (Provider::Call, "ids") => {
                plan.ids("_id", field.value.split(',').map(str::to_owned).collect())
            }
            (Provider::Contact, "ids") => plan.ids(
                "raw_contact_id",
                field.value.split(',').map(str::to_owned).collect(),
            ),
            (Provider::Contact, "id") => plan.equal("raw_contact_id", &field.value),
            (Provider::Call, "type") => plan.equal("type", &field.value),
            (Provider::Call, "duration") | (Provider::Call, "start_time") => {
                let (op, value) = comparison(field);
                if !["=", "!=", ">", ">=", "<", "<="].contains(&op) {
                    continue;
                }
                let (column, number) = if field.name == "duration" {
                    ("duration", value.parse::<i64>().ok())
                } else {
                    ("date", dates[value].as_i64())
                };
                if let Some(number) = number {
                    plan.add(format!("{column} {op} ?"), vec![number.to_string()]);
                }
            }
            (Provider::Audio, "text") => plan.add("(title LIKE '%' || ? || '%' ESCAPE '\\' OR artist LIKE '%' || ? || '%' ESCAPE '\\')".into(), vec![escape_like(&field.value), escape_like(&field.value)]),
            (Provider::Audio, "path") => plan.equal("_data", &field.value),
            (Provider::Audio, "name") => plan.equal("title", &field.value),
            (Provider::Audio, "artist") => plan.equal("artist", &field.value),
            (Provider::Video | Provider::Image, "text") => {
                let column = if matches!(provider, Provider::Image) { "(title LIKE '%' || ? || '%' ESCAPE '\\' OR _data LIKE '%' || ? || '%' ESCAPE '\\')" } else { "title LIKE '%' || ? || '%' ESCAPE '\\'" };
                let value = escape_like(&field.value);
                plan.add(column.into(), if matches!(provider, Provider::Image) { vec![value.clone(), value] } else { vec![value] });
            }
            (Provider::Audio | Provider::Video | Provider::Image | Provider::Doc, "ids") => plan.ids("_id", field.value.split(',').map(str::to_owned).collect()),
            (Provider::Audio | Provider::Video | Provider::Image | Provider::Doc, "bucket_id") => plan.equal("bucket_id", &field.value),
            (Provider::Audio | Provider::Video | Provider::Image | Provider::Doc, "excluded_dir") => plan.add("_data NOT LIKE ? || '%' ESCAPE '\\'".into(), vec![escape_like(&field.value)]),
            (Provider::Doc, "text") => plan.add("_display_name LIKE '%' || ? || '%' ESCAPE '\\'".into(), vec![escape_like(&field.value)]),
            (Provider::Doc, "ext") => plan.add("_display_name LIKE ? ESCAPE '\\'".into(), vec![format!("%.{}", escape_like(&field.value))]),
            (Provider::Doc, "parent") => plan.add("_data LIKE ? ESCAPE '\\'".into(), vec![format!("{}/%", escape_like(field.value.trim_end_matches('/')))]),
            (Provider::Doc, "type") => plan.equal("mime_type", &field.value),
            (Provider::Doc, "file_size") => {
                let (op, value) = comparison(field);
                if ["=", "!=", ">", ">=", "<", "<="].contains(&op) {
                    if let Some(bytes) = parse_size_to_bytes(value) { plan.add(format!("size {op} ?"), vec![bytes.to_string()]); }
                }
            }
            (Provider::File, "text") => plan.add("_display_name LIKE '%' || ? || '%' ESCAPE '\\'".into(), vec![escape_like(&field.value)]),
            (Provider::File, "parent") => plan.add("parent = ?".into(), vec![resolved_parent_id.unwrap_or("-1").to_owned()]),
            (Provider::File, "type") => plan.equal("mime_type", &field.value),
            (Provider::File, "ids") => plan.ids("_id", field.value.split(',').map(str::to_owned).collect()),
            (Provider::File, "file_size") => {
                let (op, value) = comparison(field);
                if ["=", "!=", ">", ">=", "<", "<="] .contains(&op) {
                    if let Some(bytes) = parse_size_to_bytes(value) { plan.add(format!("size {op} ?"), vec![bytes.to_string()]); }
                }
            }
            (Provider::Audio | Provider::Video | Provider::Image | Provider::Doc, "trash") => plan.trash = field.value.parse::<bool>().ok(),
            _ => {}
        }
    }
    plan
}
fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}
fn parse_size_to_bytes(value: &str) -> Option<u64> {
    let value = value.trim();
    let split = value
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(value.len());
    let number = value[..split].parse::<f64>().ok()?;
    let unit = value[split..].trim().to_ascii_lowercase();
    let multiplier = match unit.as_str() {
        "" | "b" => 1.0,
        "kb" | "kib" => 1024.0,
        "mb" | "mib" => 1024.0_f64.powi(2),
        "gb" | "gib" => 1024.0_f64.powi(3),
        "tb" | "tib" => 1024.0_f64.powi(4),
        _ => return None,
    };
    let bytes = number * multiplier;
    (bytes.is_finite() && bytes >= 0.0 && bytes <= u64::MAX as f64).then_some(bytes as u64)
}
async fn execute(state: &ServerState, request: Request) -> anyhow::Result<Value> {
    let fields = fields(&state.db, &request.query)?;
    let dates = fields
        .iter()
        .filter(|f| f.name == "start_time")
        .map(|f| comparison(f).1.to_owned())
        .collect::<Vec<_>>();
    let dates = if dates.is_empty() {
        Value::Null
    } else {
        state
            .host
            .call("systemEpochMillis", json!({"values":dates}))
            .await
            .map_err(anyhow::Error::msg)?
    };
    Ok(serde_json::to_value(plan(
        request.provider,
        &fields,
        &dates,
        request.resolved_parent_id.as_deref(),
        request.query.is_empty(),
    ))?)
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
#[path = "../../tests/unit/content_api/provider_plan.rs"]
mod tests;
