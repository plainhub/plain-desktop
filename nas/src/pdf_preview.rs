//! PDF preview for Office documents via LibreOffice.
//!
//! 1:1 port of Go's `internal/fs/pdf_preview.go`. Converts Office documents
//! (.doc/.docx/.xls/.xlsx/.ppt/.pptx) to PDF via `soffice --headless` and
//! caches the result on disk.

use anyhow::Result;
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Error returned when preview is not available.
#[derive(Debug)]
pub enum PreviewError {
    NotSupported,
    ToolMissing,
}

impl std::fmt::Display for PreviewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PreviewError::NotSupported => write!(f, "preview not supported"),
            PreviewError::ToolMissing => write!(f, "preview tool missing"),
        }
    }
}

impl std::error::Error for PreviewError {}

/// Per-path mutexes to prevent concurrent LibreOffice conversions of the
/// same file.
static CONVERT_LOCKS: std::sync::LazyLock<
    Mutex<std::collections::HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
> = std::sync::LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

fn get_convert_lock(key: &str) -> Arc<tokio::sync::Mutex<()>> {
    let mut locks = CONVERT_LOCKS.lock().unwrap();
    locks
        .entry(key.to_string())
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

/// Check if a file extension looks like an Office document.
fn is_office_doc_like(path: &str) -> bool {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    matches!(
        ext.as_str(),
        "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx"
    )
}

/// Find the LibreOffice binary.
fn find_libreoffice_binary() -> Result<PathBuf> {
    // Try PATH first.
    for name in &["soffice", "libreoffice"] {
        if let Some(p) = which(name) {
            return Ok(p);
        }
    }

    // Probe common locations (systemd services may run with minimal PATH).
    let candidates = [
        "/usr/bin/soffice",
        "/usr/bin/libreoffice",
        "/usr/lib/libreoffice/program/soffice",
        "/usr/lib64/libreoffice/program/soffice",
        "/snap/bin/libreoffice",
        "/var/lib/snapd/snap/bin/libreoffice",
        "/app/bin/libreoffice",
        "/app/bin/soffice",
    ];
    for p in &candidates {
        let path = Path::new(p);
        if path.is_file() {
            return Ok(PathBuf::from(p));
        }
    }

    Err(PreviewError::ToolMissing.into())
}

fn which(name: &str) -> Option<PathBuf> {
    let path_env = std::env::var_os("PATH")?;
    for p in std::env::split_paths(&path_env) {
        let candidate = p.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Compute the cache key for a PDF preview.
fn pdf_preview_key(path: &str, mod_unix: i64, size: i64) -> String {
    let mut h = Sha1::new();
    h.update(path.as_bytes());
    h.update(b"|");
    h.update(format!("{mod_unix}|{size}").as_bytes());
    hex::encode(h.finalize())
}

fn pdf_preview_cache_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("preview_pdf")
}

/// Check if LibreOffice is available for PDF conversion.
pub fn libreoffice_available() -> bool {
    find_libreoffice_binary().is_ok()
}

/// Convert an Office document to PDF and return the cached PDF path.
///
/// The result is cached on disk keyed by (path, mtime, size). If the
/// cached PDF already exists and is non-empty, it is returned immediately.
pub async fn get_or_create_pdf_preview(
    data_dir: &Path,
    src_path: &str,
    mod_unix: i64,
    size: i64,
) -> Result<PathBuf> {
    if !is_office_doc_like(src_path) {
        return Err(PreviewError::NotSupported.into());
    }

    let key = pdf_preview_key(src_path, mod_unix, size);
    let cache_dir = pdf_preview_cache_dir(data_dir);
    std::fs::create_dir_all(&cache_dir)?;

    let out_path = cache_dir.join(format!("{key}.pdf"));

    // Check cache.
    if let Ok(fi) = std::fs::metadata(&out_path) {
        if fi.is_file() && fi.len() > 0 {
            return Ok(out_path);
        }
    }

    // Acquire per-key lock to prevent concurrent conversions.
    let lock = get_convert_lock(&key);
    let _guard = lock.lock().await;

    // Double-check after acquiring lock.
    if let Ok(fi) = std::fs::metadata(&out_path) {
        if fi.is_file() && fi.len() > 0 {
            return Ok(out_path);
        }
    }

    let lo_bin = find_libreoffice_binary()?;

    let tmp_dir = tempfile::tempdir_in(&cache_dir)?;

    // Run soffice --headless --convert-to pdf.
    let base = Path::new(src_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("input");
    let stem = Path::new(base)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let pdf_name = format!("{stem}.pdf");
    let tmp_pdf = tmp_dir.path().join(&pdf_name);

    let status = tokio::process::Command::new(&lo_bin)
        .args([
            "--headless",
            "--nologo",
            "--nofirststartwizard",
            "--norestore",
            "--convert-to",
            "pdf",
            "--outdir",
            tmp_dir.path().to_str().unwrap(),
            src_path,
        ])
        .env("HOME", tmp_dir.path().to_str().unwrap())
        .status()
        .await?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "libreoffice convert failed (exit {})",
            status
        ));
    }

    if !tmp_pdf.exists() {
        return Err(anyhow::anyhow!("libreoffice produced no pdf"));
    }

    // Move to cache.
    std::fs::rename(&tmp_pdf, &out_path).or_else(|_| {
        // Cross-device rename fallback: copy.
        std::fs::copy(&tmp_pdf, &out_path)?;
        std::fs::remove_file(&tmp_pdf)
    })?;

    Ok(out_path)
}
