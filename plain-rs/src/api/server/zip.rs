//! `/zip/dir` and `/zip/files` endpoints.
//!
//! Mirrors Go `cmd/services/api/zip.go`:
//!
//!   * `GET /zip/dir?id=<encrypted_folder_id>` — decrypts the id to a
//!     folder path, walks the tree, and streams `folderName.zip`.
//!   * `GET /zip/files?id=<encrypted_json_id>` — decrypts the id to a
//!     JSON document `{id, type, query, name}` that selects which files
//!     to include (`type` ∈ `AUDIO|VIDEO|IMAGE|FILE`). For `FILE`,
//!     `req.id` is a `temp:` key in the KV store holding a JSON array of
//!     `{path, name}` items; for the media types, a (currently stubbed)
//!     media scan is performed.
//!
//! Both handlers write the archive to an in-memory buffer before
//! streaming. The Go side streams directly to `c.Writer` (a
//! `http.ResponseWriter`); doing the same in axum would require a
//! `Body::from_stream` over a channel that the zip writer feeds. We
//! accept the buffer trade-off — typical archives are <100 MiB and the
//! latency is dominated by file I/O, not the extra copy.

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

use super::ServerState;

#[derive(Deserialize)]
pub struct IdQuery {
    id: Option<String>,
    /// Optional filename override (without extension); only used by
    /// `/zip/dir`. Mirrors Go's `c.Query("name")`.
    name: Option<String>,
}

pub async fn zip_dir_handler(State(state): State<ServerState>, Query(q): Query<IdQuery>) -> Response {
    let id = q.id.as_deref().unwrap_or("").trim();
    if id.is_empty() {
        return (StatusCode::BAD_REQUEST, "").into_response();
    }
    let folder_path = match crate::media::fsx::path_from_file_id(id, &state.ctx.prefs) {
        Ok(p) => p,
        Err(_) => return (StatusCode::FORBIDDEN, "").into_response(),
    };
    let folder_path = folder_path.trim();
    if folder_path.is_empty() {
        return (StatusCode::BAD_REQUEST, "").into_response();
    }

    let folder_meta = match tokio::fs::metadata(folder_path).await {
        Ok(m) => m,
        Err(_) => return (StatusCode::NOT_FOUND, "").into_response(),
    };
    if !folder_meta.is_dir() {
        return (StatusCode::NOT_FOUND, "").into_response();
    }

    let base_name = Path::new(folder_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("archive")
        .to_string();
    let mut file_name = q.name.as_deref().unwrap_or("").trim().to_string();
    if file_name.is_empty() {
        file_name = format!("{base_name}.zip");
    }
    if !file_name.to_lowercase().ends_with(".zip") {
        file_name.push_str(".zip");
    }

    let mut buf: Vec<u8> = Vec::new();
    {
        let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        // Walk synchronously — `crate::media::walk::Walk` uses blocking std::fs
        // I/O. The handler is already running on axum's blocking-eligible
        // task pool, and typical folders are <10k entries, so we skip the
        // extra `spawn_blocking` hop (it would also require `Send` bounds
        // the borrow of `zw` doesn't satisfy).
        let _ = zip_folder_to_writer(&mut zw, folder_path, &base_name, &opts);
        let _ = zw.finish();
    }

    let encoded = url_query_escape(&file_name);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/zip")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{file_name}\"; filename*=utf-8''{encoded}"),
        )
        .header(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            header::CONTENT_DISPOSITION.as_str(),
        )
        .body(Body::from(buf))
        .unwrap()
}

#[derive(Deserialize, Debug)]
struct ZipFilesRequest {
    #[serde(default)]
    #[cfg_attr(not(feature = "nas"), allow(dead_code))]
    id: String,
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    #[allow(dead_code)]
    query: String,
    #[serde(default)]
    name: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct ZipPathItem {
    path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    name: String,
}

pub async fn zip_files_handler(
    State(state): State<ServerState>,
    Query(q): Query<IdQuery>,
) -> Response {
    let id = q.id.as_deref().unwrap_or("").trim();
    if id.is_empty() {
        return (StatusCode::BAD_REQUEST, "").into_response();
    }
    // Mirrors Go: zip_files uses 400 (not 403) on decrypt failure.
    let plain = match crate::media::fsx::path_from_file_id(id, &state.ctx.prefs) {
        Ok(p) => p,
        Err(_) => return (StatusCode::BAD_REQUEST, "").into_response(),
    };

    let req: ZipFilesRequest = match serde_json::from_str(&plain) {
        Ok(r) => r,
        Err(_) => return (StatusCode::BAD_REQUEST, "").into_response(),
    };
    let type_str = req.r#type.trim().to_uppercase();
    if type_str.is_empty() {
        return (StatusCode::BAD_REQUEST, "").into_response();
    }

    let mut file_name = req.name.trim().to_string();
    if file_name.is_empty() {
        file_name = "download.zip".to_string();
    }
    if !file_name.to_lowercase().ends_with(".zip") {
        file_name.push_str(".zip");
    }

    // Resolve the item list based on the request type.
    let mut items: Vec<ZipPathItem> = match type_str.as_str() {
        "AUDIO" | "VIDEO" | "IMAGE" => {
            // The Rust port does not yet have a media scanner API
            // equivalent to Go's `helpers.ScanAudios/ScanVideos/ScanImages`.
            // Until that lands (TODO 8.8), we return an empty item list —
            // matching the existing `audios` / `videos` / `images` GraphQL
            // queries which also return empty.
            Vec::new()
        }
        "FILE" => {
            #[cfg(feature = "nas")]
            {
                let tmp_key = req.id.trim();
                if tmp_key.is_empty() {
                    return (StatusCode::BAD_REQUEST, "").into_response();
                }
                let raw = match crate::nas::temp_store::take(tmp_key) {
                    Some(v) => v,
                    None => return (StatusCode::NOT_FOUND, "").into_response(),
                };
                match serde_json::from_str::<Vec<ZipPathItem>>(&raw) {
                    Ok(v) => v,
                    Err(_) => return (StatusCode::BAD_REQUEST, "").into_response(),
                }
            }
            #[cfg(not(feature = "nas"))]
            Vec::new()
        }
        _ => return (StatusCode::BAD_REQUEST, "").into_response(),
    };

    // Filter to existing paths (dedup + stat check).
    items = filter_existing_zip_items(items);
    // Drop items that live inside one of the selected directories (so we
    // don't double-add children of a folder that's also in the list).
    items = drop_items_inside_selected_dirs(items);

    let mut buf: Vec<u8> = Vec::new();
    {
        let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for it in &items {
            let p = it.path.trim();
            if p.is_empty() {
                continue;
            }
            let meta = match std::fs::metadata(p) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let mut entry_name = it.name.trim().to_string();
            if entry_name.is_empty() {
                entry_name = Path::new(p)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file")
                    .to_string();
            }
            entry_name = safe_zip_entry_name(&entry_name);
            if entry_name.is_empty() {
                entry_name = Path::new(p)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file")
                    .to_string();
                entry_name = safe_zip_entry_name(&entry_name);
                if entry_name.is_empty() {
                    continue;
                }
            }

            if meta.is_dir() {
                let _ = zip_folder_to_writer(&mut zw, p, &entry_name, &opts);
            } else {
                let _ = zip_add_file(&mut zw, p, &entry_name, &meta, &opts);
            }
        }
        let _ = zw.finish();
    }

    let encoded = url_query_escape(&file_name);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/zip")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{file_name}\"; filename*=utf-8''{encoded}"),
        )
        .header(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            header::CONTENT_DISPOSITION.as_str(),
        )
        .body(Body::from(buf))
        .unwrap()
}

/// Same `application/x-www-form-urlencoded` escape used by `fs.rs` —
/// matches Go's `url.QueryEscape` + `strings.ReplaceAll(..., "+", "%20")`.
fn url_query_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        let c = b as char;
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
            out.push(c);
        } else if c == ' ' {
            out.push('+');
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out.replace('+', "%20")
}

/// Mirrors Go `safeZipEntryName`: trim, normalise separators, `path.Clean`,
/// strip leading `/`, reject `.`/`..`/`../`.
fn safe_zip_entry_name(name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        return String::new();
    }
    let name = name.replace('\\', "/");
    // Minimal `path.Clean`-equivalent — collapse `//`, `./`, resolve `..`.
    let mut segments: Vec<&str> = Vec::new();
    for seg in name.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            _ => segments.push(seg),
        }
    }
    let cleaned = segments.join("/");
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        return String::new();
    }
    // Reject any surviving leading "../" prefix.
    if cleaned.starts_with("../") {
        return String::new();
    }
    cleaned
}

/// Walk a folder recursively and write every entry to the zip writer
/// under `prefix/...`. Mirrors Go `zipFolderToWriter`. Returns
/// `io::Result` so callers can choose to ignore errors per-entry.
fn zip_folder_to_writer<W: Write + std::io::Seek>(
    zw: &mut zip::ZipWriter<W>,
    folder_path: &str,
    prefix: &str,
    opts: &SimpleFileOptions,
) -> std::io::Result<()> {
    let folder_path = folder_path.trim();
    if folder_path.is_empty() {
        return Ok(());
    }
    let mut prefix = safe_zip_entry_name(prefix);
    if prefix.is_empty() {
        prefix = safe_zip_entry_name(
            Path::new(folder_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(""),
        );
    }
    if prefix.is_empty() {
        return Ok(());
    }

    // Top-level directory entry.
    let _ = zw.add_directory(format!("{prefix}/"), *opts);

    let root = PathBuf::from(folder_path);
    for entry in crate::media::walk::Walk::new(&root)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let p = entry.path();
        if p == root {
            continue;
        }
        let rel = match p.strip_prefix(&root) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let rel_str = safe_zip_entry_name(&rel_str);
        if rel_str.is_empty() {
            continue;
        }
        let zip_name = format!("{prefix}/{rel_str}");
        if p.is_dir() {
            let _ = zw.add_directory(format!("{zip_name}/"), *opts);
        } else {
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let _ = zip_add_file(zw, &p.to_string_lossy(), &zip_name, &meta, opts);
        }
    }
    Ok(())
}

/// Add a single file to the zip writer. Mirrors Go `zipAddFile`.
fn zip_add_file<W: Write + std::io::Seek>(
    zw: &mut zip::ZipWriter<W>,
    file_path: &str,
    zip_name: &str,
    _meta: &std::fs::Metadata,
    opts: &SimpleFileOptions,
) -> std::io::Result<()> {
    let zip_name = safe_zip_entry_name(zip_name);
    if zip_name.is_empty() {
        return Ok(());
    }
    zw.start_file(zip_name, *opts)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let mut f = std::fs::File::open(file_path)?;
    std::io::copy(&mut f, zw)?;
    Ok(())
}

/// Drop items whose path doesn't exist on disk; dedup by cleaned path.
/// Mirrors Go `filterExistingZipItems`.
fn filter_existing_zip_items(items: Vec<ZipPathItem>) -> Vec<ZipPathItem> {
    let mut out = Vec::with_capacity(items.len());
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for it in items {
        let p = it.path.trim();
        if p.is_empty() {
            continue;
        }
        let cleaned = PathBuf::from(p)
            .canonicalize()
            .map(|c| c.to_string_lossy().to_string())
            .unwrap_or_else(|_| p.to_string());
        if !seen.insert(cleaned.clone()) {
            continue;
        }
        if std::fs::metadata(&cleaned).is_ok() {
            out.push(ZipPathItem {
                path: cleaned,
                name: it.name.trim().to_string(),
            });
        }
    }
    out
}

/// Drop items that live inside one of the selected directories.
/// Mirrors Go `dropItemsInsideSelectedDirs`.
fn drop_items_inside_selected_dirs(items: Vec<ZipPathItem>) -> Vec<ZipPathItem> {
    let mut dirs: Vec<String> = items
        .iter()
        .filter_map(|it| {
            let p = it.path.trim();
            if p.is_empty() {
                return None;
            }
            match std::fs::metadata(p) {
                Ok(m) if m.is_dir() => Some(
                    PathBuf::from(p)
                        .canonicalize()
                        .map(|c| c.to_string_lossy().to_string())
                        .unwrap_or_else(|_| p.to_string()),
                ),
                _ => None,
            }
        })
        .collect();
    if dirs.is_empty() {
        return items;
    }
    // Shorter first (matches Go sort: parent before child).
    dirs.sort_by(|a, b| {
        if a.len() != b.len() {
            a.len().cmp(&b.len())
        } else {
            a.cmp(b)
        }
    });

    let is_inside_any_dir = |p: &str| -> bool {
        let cleaned = PathBuf::from(p)
            .canonicalize()
            .map(|c| c.to_string_lossy().to_string())
            .unwrap_or_else(|_| p.to_string());
        for d in &dirs {
            if cleaned == *d {
                continue;
            }
            if cleaned.starts_with(&format!("{d}/")) {
                return true;
            }
        }
        false
    };

    items
        .into_iter()
        .filter(|it| !it.path.trim().is_empty() && !is_inside_any_dir(&it.path))
        .collect()
}
