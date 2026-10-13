use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    pub(crate) operation: String,
    pub(crate) path: String,
    pub(crate) params: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccessRequest {
    operation: String,
    paths: Vec<String>,
}

pub fn authorize_read(params: Value) -> Result<bool, String> {
    let request: AccessRequest =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    if request.operation != "read" || request.paths.is_empty() {
        return Err("Explicit file read operation and paths required".into());
    }
    for path in request.paths {
        let path = Path::new(&path);
        if !path.is_absolute() {
            return Err("Absolute file path required".into());
        }
        let path = path.canonicalize().map_err(|error| error.to_string())?;
        let metadata = path.metadata().map_err(|error| error.to_string())?;
        if metadata.is_file() {
            std::fs::File::open(&path).map_err(|error| error.to_string())?;
        } else if metadata.is_dir() {
            std::fs::read_dir(&path).map_err(|error| error.to_string())?;
        } else {
            return Err("Regular file or directory required".into());
        }
    }
    Ok(true)
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
