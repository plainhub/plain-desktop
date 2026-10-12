use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    permissions: Vec<String>,
}

pub fn facts(granted: &[String], params: Value) -> Result<Value, String> {
    let request: Request = serde_json::from_value(params).map_err(|error| error.to_string())?;
    let values: serde_json::Map<String, Value> = request
        .permissions
        .into_iter()
        .map(|permission| {
            let allowed = granted.contains(&permission);
            (permission, Value::Bool(allowed))
        })
        .collect();
    Ok(json!({"granted":values}))
}

#[cfg(test)]
#[path = "../../tests/unit/host/permission_facts.rs"]
mod tests;
