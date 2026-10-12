use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    operation: String,
    path: String,
    params: Value,
}

pub fn facts(directory: &Path, params: Value) -> Result<Value, String> {
    let request: Request = serde_json::from_value(params).map_err(|error| error.to_string())?;
    if !request.params.is_object() {
        return Err("Invalid file resource parameters".into());
    }
    match request.operation.as_str() {
        "appDir" => {
            if !directory.is_absolute() {
                return Err("Core data directory must be absolute".into());
            }
            Ok(Value::String(
                directory.join("files").to_string_lossy().into_owned(),
            ))
        }
        "mime" => Ok(Value::String(
            plain_rs::utils::mime::mime_from_ext(&request.path).to_owned(),
        )),
        _ => Err(format!(
            "Unsupported file resource operation: {}",
            request.operation
        )),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/host/file_resource.rs"]
mod tests;
