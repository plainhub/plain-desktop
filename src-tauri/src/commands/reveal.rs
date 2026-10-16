use std::path::{Path, PathBuf};

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

fn existing_file(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);
    if !path.is_absolute() {
        return Err("Absolute file path required".into());
    }
    let path = std::fs::canonicalize(path).map_err(|error| error.to_string())?;
    if !path.is_file() {
        return Err("Regular file required".into());
    }
    Ok(path)
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string())
}

#[tauri::command]
pub fn reveal_file(path: String) -> Result<(), String> {
    let path = existing_file(&path)?;
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_file_as(
    app: AppHandle,
    path: String,
    name: Option<String>,
) -> Result<(), String> {
    let src = existing_file(&path)?;
    let default_name = name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| display_name(&src));
    let target = app
        .dialog()
        .file()
        .set_file_name(&default_name)
        .blocking_save_file();
    let Some(target) = target else { return Ok(()) };
    let dest = target.into_path().map_err(|e| e.to_string())?;
    copy_to(&src, &dest)
}

fn copy_to(source: &Path, destination: &Path) -> Result<(), String> {
    if destination == source || std::fs::canonicalize(destination).ok().as_deref() == Some(source) {
        return Ok(());
    }
    std::fs::copy(source, destination).map_err(|error| error.to_string())?;
    Ok(())
}

/// Save in-memory text content to a user-chosen destination via the native
/// save dialog (WKWebView cannot download blobs, so the webview hands the
/// already-fetched content over instead).
#[tauri::command]
pub async fn save_text_file_as(
    app: AppHandle,
    name: String,
    contents: String,
) -> Result<(), String> {
    let default_name = name.trim();
    let default_name = if default_name.is_empty() {
        "file.txt"
    } else {
        default_name
    };
    let target = app
        .dialog()
        .file()
        .set_file_name(default_name)
        .blocking_save_file();
    let Some(target) = target else { return Ok(()) };
    let dest = target.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&dest, contents).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn copy_file_to_clipboard(path: String) -> Result<(), String> {
    write_file_clipboard(&existing_file(&path)?)
}

/// Finder-style file copy: write the file URL as `NSPasteboardTypeFileURL`.
#[cfg(target_os = "macos")]
fn write_file_clipboard(path: &Path) -> Result<(), String> {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeFileURL};
    use objc2_foundation::{NSData, NSString, NSURL};

    let ns_path = NSString::from_str(&path.to_string_lossy());
    let absolute = NSURL::fileURLWithPath(&ns_path)
        .absoluteString()
        .ok_or_else(|| format!("Invalid file URL: {}", path.display()))?;
    let data = NSData::from_vec(absolute.to_string().into_bytes());
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    if !pasteboard.setData_forType(Some(&data), unsafe { NSPasteboardTypeFileURL }) {
        return Err(format!(
            "Failed to write file URL to clipboard: {}",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn write_file_clipboard(_path: &Path) -> Result<(), String> {
    Err("Copy to clipboard is only supported on macOS".to_string())
}

#[cfg(test)]
#[path = "../../tests/unit/commands/reveal.rs"]
mod tests;
