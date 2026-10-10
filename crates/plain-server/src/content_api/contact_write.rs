use super::host::Host;
use super::server::ServerState;
use crate::prefs::Prefs;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

const MAX_NAME_LEN: usize = 200;
const MAX_NOTES_LEN: usize = 10_000;
const MAX_SOURCE_LEN: usize = 256;
const MAX_GROUP_NAME_LEN: usize = 128;
const MAX_VALUE_LEN: usize = 512;
const MAX_ENTRIES: usize = 128;

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) enum Action {
    CreateContact,
    UpdateContact,
    CreateGroup,
    UpdateGroup,
    DeleteGroup,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
    pub(super) action: Action,
    #[serde(default)]
    pub(super) id: Option<String>,
    #[serde(default)]
    pub(super) input: Option<Value>,
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) account_name: Option<String>,
    #[serde(default)]
    pub(super) account_type: Option<String>,
}

fn permission_allowed(configured: &[String]) -> bool {
    configured.iter().any(|name| name == "WRITE_CONTACTS")
}

fn required<'a>(field: Option<&'a String>, name: &str) -> anyhow::Result<&'a str> {
    let value = field.map(String::as_str).unwrap_or_default().trim();
    anyhow::ensure!(!value.is_empty(), "{name} is empty");
    Ok(value)
}

fn bounded(value: &str, max: usize, name: &str) -> anyhow::Result<()> {
    anyhow::ensure!(value.chars().count() <= max, "{name} is too long");
    Ok(())
}

/// Phone/email/address rows arrive with an empty `value` while the desktop
/// edit form is still being filled in, so blank rows stay legal here. Only
/// their shape and size are checked.
fn entries<'a>(input: &'a Value, field: &str) -> anyhow::Result<&'a [Value]> {
    let items: &[Value] = match input.get(field) {
        Some(value) => value
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("{field} must be an array"))?,
        None => &[],
    };
    anyhow::ensure!(items.len() <= MAX_ENTRIES, "{field} has too many items");
    for item in items {
        let object = item
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("{field} item must be an object"))?;
        if let Some(value) = object.get("value") {
            let value = value
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("{field} value must be a string"))?;
            bounded(value, MAX_VALUE_LEN, &format!("{field} value"))?;
        }
    }
    Ok(items)
}

fn text_field(input: &Value, field: &str, max: usize) -> anyhow::Result<()> {
    let Some(value) = input.get(field) else {
        return Ok(());
    };
    let value = value
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("{field} must be a string"))?;
    bounded(value, max, field)
}

fn bounded_field(
    object: &serde_json::Map<String, Value>,
    field: &str,
    max: usize,
) -> anyhow::Result<()> {
    let Some(value) = object.get(field) else {
        return Ok(());
    };
    let value = value
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("{field} must be a string"))?;
    bounded(value, max, field)
}

/// Android creates the raw contact first and only then resolves the account
/// behind `source`, so an unknown or empty source aborts the whole write.
fn validate_contact_input(input: &Value, require_source: bool) -> anyhow::Result<()> {
    let object = input
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("input must be an object"))?;
    for field in [
        "prefix",
        "firstName",
        "middleName",
        "lastName",
        "suffix",
        "nickname",
    ] {
        text_field(input, field, MAX_NAME_LEN)?;
    }
    text_field(input, "notes", MAX_NOTES_LEN)?;
    text_field(input, "source", MAX_SOURCE_LEN)?;
    if require_source {
        let source = object
            .get("source")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        anyhow::ensure!(!source.is_empty(), "contact source is empty");
    }
    let phones = entries(input, "phoneNumbers")?;
    let emails = entries(input, "emails")?;
    entries(input, "addresses")?;
    entries(input, "events")?;
    entries(input, "websites")?;
    entries(input, "ims")?;
    if let Some(groups) = object.get("groupIds") {
        let groups = groups
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("groupIds must be an array"))?;
        anyhow::ensure!(groups.len() <= MAX_ENTRIES, "groupIds has too many items");
    }
    if let Some(organization) = object.get("organization") {
        anyhow::ensure!(
            organization.is_null() || organization.is_object(),
            "organization must be an object"
        );
        if let Some(organization) = organization.as_object() {
            bounded_field(organization, "company", MAX_NAME_LEN)?;
            bounded_field(organization, "title", MAX_NAME_LEN)?;
        }
    }
    let named = [
        "prefix",
        "firstName",
        "middleName",
        "lastName",
        "suffix",
        "nickname",
    ]
    .iter()
    .any(|field| {
        object
            .get(*field)
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
    }) || object
        .get("organization")
        .and_then(|value| value.get("company"))
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    let filled = |items: &[Value]| {
        items.iter().any(|item| {
            item.get("value")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
        })
    };
    anyhow::ensure!(
        named || filled(phones) || filled(emails),
        "contact needs a name, a phone number or an email"
    );
    Ok(())
}

pub(super) async fn execute(prefs: &Prefs, host: &Host, request: Request) -> anyhow::Result<Value> {
    let configured: Vec<String> = prefs.get_or("api_permissions", Vec::new());
    anyhow::ensure!(permission_allowed(&configured), "no_permission");
    match request.action {
        Action::CreateContact | Action::UpdateContact => {
            let input = request
                .input
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("input is missing"))?;
            let creating = request.action == Action::CreateContact;
            anyhow::ensure!(
                creating || request.id.is_some(),
                "updateContact requires an id"
            );
            anyhow::ensure!(
                !creating || request.id.is_none(),
                "createContact must not carry an id"
            );
            let mut params = json!({"input": input});
            if !creating {
                let id = required(request.id.as_ref(), "contact id")?;
                params["id"] = Value::String(id.to_owned());
            }
            validate_contact_input(input, creating)?;
            let result = host
                .call(
                    if creating {
                        "systemCreateContact"
                    } else {
                        "systemUpdateContact"
                    },
                    params,
                )
                .await
                .map_err(anyhow::Error::msg)?;
            if !creating {
                return Ok(json!({"ok": true}));
            }
            let created = result.as_str().unwrap_or_default().trim().to_owned();
            anyhow::ensure!(!created.is_empty(), "Failed to create contact");
            Ok(json!({"id": created}))
        }
        Action::CreateGroup => {
            let name = required(request.name.as_ref(), "group name")?;
            bounded(name, MAX_GROUP_NAME_LEN, "group name")?;
            let account_name = required(request.account_name.as_ref(), "account name")?;
            bounded(account_name, MAX_NAME_LEN, "account name")?;
            let account_type = required(request.account_type.as_ref(), "account type")?;
            bounded(account_type, MAX_NAME_LEN, "account type")?;
            let result = host
                .call(
                    "systemCreateContactGroup",
                    json!({
                        "name": name,
                        "accountName": account_name,
                        "accountType": account_type,
                    }),
                )
                .await
                .map_err(anyhow::Error::msg)?;
            let created = result.as_str().unwrap_or_default().trim().to_owned();
            anyhow::ensure!(!created.is_empty(), "Failed to create contact group");
            Ok(json!({"id": created}))
        }
        Action::UpdateGroup | Action::DeleteGroup => {
            let id = required(request.id.as_ref(), "group id")?;
            let method = match request.action {
                Action::UpdateGroup => "systemUpdateContactGroup",
                _ => "systemDeleteContactGroup",
            };
            let mut params = json!({"id": id});
            if request.action == Action::UpdateGroup {
                let name = required(request.name.as_ref(), "group name")?;
                bounded(name, MAX_GROUP_NAME_LEN, "group name")?;
                params["name"] = Value::String(name.to_owned());
            }
            host.call(method, params)
                .await
                .map_err(anyhow::Error::msg)?;
            Ok(json!({"ok": true}))
        }
    }
}

pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match execute(&state.prefs, &state.host, request).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/contact_write.rs"]
mod tests;
