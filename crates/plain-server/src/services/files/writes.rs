use crate::services::files::file_access::receipt;
use plain_rs::files::filesystem::writes::Operation;

pub(crate) async fn execute(
    host: &crate::host::bridge::Host,
    operation: Operation,
) -> Result<plain_rs::files::filesystem::record::FileRecord, String> {
    let path = operation.path().to_owned();
    if !std::path::Path::new(&path).is_absolute() {
        return Err("absolute file path required".into());
    }
    receipt(host, "fileTaskAuthorize", &path).await?;
    tokio::task::spawn_blocking(move || operation.apply())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    receipt(host, "fileTaskScan", &path).await?;
    let entry = plain_rs::files::filesystem::stat(std::path::Path::new(&path))
        .await
        .map_err(|e| e.to_string())?;
    let mut record = plain_rs::files::filesystem::record::FileRecord::from(entry);
    record.permission = "rw".into();
    Ok(record)
}
