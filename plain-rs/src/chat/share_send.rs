use crate::{
    chat::app_file_store,
    db::{Db, ShareRow},
    shares::Service,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PickedFile {
    pub uri: String,
    pub name: String,
    pub size: i64,
    pub mime_type: String,
}
pub fn placeholders(files: &[PickedFile], normalize: bool) -> Result<Vec<Value>> {
    files.iter().map(|file| {
        ensure!(!file.uri.is_empty() && file.size>=0,"Invalid picker facts");
        let mut name=file.name.clone();
        if normalize {{ let ext=crate::utils::mime::mime_extension(&file.mime_type);
            if ext != "bin" {name=format!("{}.{}",name.rsplit_once('.').map(|(base,_)|base).unwrap_or(&name),ext);}
        }}
        Ok(json!({"id":crate::utils::short_uuid::short_uuid(),"uri":file.uri,"size":file.size,"fileName":name,"summary":"","width":0,"height":0,"durationMs":0}))
    }).collect()
}
pub fn image(name: &str) -> bool {
    matches!(
        name.rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .as_deref(),
        Some(
            "jpg"
                | "png"
                | "jpeg"
                | "bmp"
                | "webp"
                | "heic"
                | "heif"
                | "apng"
                | "avif"
                | "gif"
                | "tiff"
                | "tif"
                | "svg"
        )
    )
}
pub fn visual(name: &str) -> bool {
    image(name)
        || matches!(
            name.rsplit_once('.')
                .map(|(_, ext)| ext.to_ascii_lowercase())
                .as_deref(),
            Some("mp4" | "mkv" | "webm" | "avi" | "3gp" | "mov" | "m4v" | "3gpp")
        )
}
pub fn text(db: &Db, directory: &Path, text: &str) -> Result<Value> {
    if text.encode_utf16().count() <= 2048 {
        return Ok(json!({"type":"TEXT","value":{"text":text}}));
    }
    let imported = app_file_store::import_bytes(db, directory, text.as_bytes(), "text/plain")?;
    let summary: String = text
        .chars()
        .scan(0usize, |units, c| {
            *units += c.len_utf16();
            (*units <= 250).then_some(c)
        })
        .collect();
    Ok(
        json!({"type":"FILES","value":{"items":[{"id":crate::utils::short_uuid::short_uuid(),"uri":format!("fid:{}",imported.fid_suffix),"fileName":format!("message-{}.txt",chrono::Utc::now().timestamp_millis()),"size":text.len(),"summary":summary,"width":0,"height":0,"durationMs":0}]}}),
    )
}
pub fn folder_card(
    service: &Service,
    row: &ShareRow,
    actor: &str,
    ip: &str,
    port: u16,
) -> Result<Value> {
    let roots = Service::roots(row)?;
    let mut count = 0usize;
    let mut size = 0u64;
    if roots.len() == 1 && roots[0].is_dir {
        for entry in std::fs::read_dir(&roots[0].real_path)? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            count += 1;
            let metadata = entry.metadata()?;
            if metadata.is_file() {
                size = size
                    .checked_add(metadata.len())
                    .ok_or_else(|| anyhow::anyhow!("Share size overflow"))?;
            }
        }
    } else {
        count = roots.len();
        for root in roots {
            let m = std::fs::metadata(&root.real_path)?;
            if m.is_file() {
                size = size
                    .checked_add(m.len())
                    .ok_or_else(|| anyhow::anyhow!("Share size overflow"))?;
            }
        }
    }
    Ok(
        json!({"type":"SHARE","value":{"shareId":row.id,"urlToken":service.token(&row.id)?,"peerInfo":{"id":actor,"ip":ip,"port":port},"name":row.name,"itemCount":count,"totalSize":size,"expiresAt":row.expires_at}}),
    )
}
