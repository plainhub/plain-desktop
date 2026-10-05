use super::server::ServerState;
use anyhow::{Result, ensure};
use axum::{
    Json,
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::HashSet,
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::io::AsyncReadExt;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Item {
    source_path: PathBuf,
    entry_name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    items: Vec<Item>,
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
struct Cancel(Arc<AtomicBool>);
impl Drop for Cancel {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
fn pack(
    directory: PathBuf,
    items: Vec<Item>,
    canceled: Arc<AtomicBool>,
) -> Result<(std::fs::File, Temporary)> {
    ensure!(
        !items.is_empty() && items.len() <= 100_000,
        "Invalid shared ZIP selection"
    );
    let mut names = HashSet::new();
    for item in &items {
        ensure!(item.source_path.is_absolute(), "Invalid shared ZIP source");
        ensure!(
            !item.entry_name.is_empty()
                && item.entry_name.split('/').all(|part| !part.is_empty()
                    && part != "."
                    && part != ".."
                    && !part.contains(['\\', '\0'])),
            "Invalid shared ZIP entry"
        );
        ensure!(names.insert(&item.entry_name), "Duplicate shared ZIP entry");
    }
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(format!("share-zip-{}.tmp", uuid::Uuid::new_v4()));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)?;
    let temporary = Temporary(path);
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut buffer = vec![0; 64 * 1024];
    for item in items {
        let mut source = std::fs::File::open(item.source_path)?;
        ensure!(
            source.metadata()?.is_file(),
            "Shared ZIP source is not a file"
        );
        writer.start_file(item.entry_name, options)?;
        loop {
            ensure!(!canceled.load(Ordering::SeqCst), "Shared ZIP canceled");
            let read = source.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            writer.write_all(&buffer[..read])?;
        }
    }
    ensure!(!canceled.load(Ordering::SeqCst), "Shared ZIP canceled");
    let mut file = writer.finish()?;
    file.seek(SeekFrom::Start(0))?;
    Ok((file, temporary))
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut stop = state.stop.clone();
    if *stop.borrow() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let canceled = Arc::new(AtomicBool::new(false));
    let cancel = Cancel(canceled.clone());
    let work = async {
        let permit = state.lan.capacity.clone().acquire_owned().await?;
        let directory = state.directory.join("shared-zip");
        let (file, temporary) =
            tokio::task::spawn_blocking(move || pack(directory, request.items, canceled)).await??;
        let size = file.metadata()?.len();
        let stream = futures_util::stream::try_unfold(
            (
                tokio::fs::File::from_std(file),
                temporary,
                permit,
                cancel,
                state.stop.clone(),
            ),
            |(mut file, temporary, permit, cancel, mut stop)| async move {
                if *stop.borrow() {
                    return Ok::<_, std::io::Error>(None);
                }
                let mut buffer = vec![0; 64 * 1024];
                let read = tokio::select! { result=file.read(&mut buffer)=>result?, _=stop.changed()=>return Ok(None) };
                if read == 0 {
                    return Ok(None);
                }
                buffer.truncate(read);
                Ok(Some((buffer, (file, temporary, permit, cancel, stop))))
            },
        );
        Ok::<_, anyhow::Error>(
            Response::builder()
                .header("content-type", "application/zip")
                .header("content-length", size)
                .body(Body::from_stream(stream))?,
        )
    };
    match tokio::select! { result=work=>result, _=stop.changed()=>Err(anyhow::anyhow!("Core stopped")) }
    {
        Ok(response) => response,
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}
#[cfg(test)]
#[path = "../../tests/unit/content_api/shared_zip.rs"]
mod tests;
