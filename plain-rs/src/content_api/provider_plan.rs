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
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
    provider: Provider,
    query: String,
}
#[derive(Default, Serialize)]
pub(super) struct Plan {
    pub clauses: Vec<String>,
    pub args: Vec<String>,
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
        let markers = vec!["?"; ids.len()].join(",");
        if !ids.is_empty() {
            self.add(format!("{column} IN ({markers})"), ids);
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
fn plan(provider: Provider, fields: &[FilterField], dates: &Value) -> Plan {
    let mut plan = Plan::default();
    if matches!(provider, Provider::Contact) {
        plan.equal("mimetype", "vnd.android.cursor.item/name");
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
            _ => {}
        }
    }
    plan
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
