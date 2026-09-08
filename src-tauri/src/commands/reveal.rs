use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

/// Resolve a chat file URI (`fid:`, `app://`, relative or absolute path)
/// to a local on-disk path, returning `None` when the file does not exist.
fn local_file_path(uri: &str, data_dir: &Path) -> Option<PathBuf> {
    let path = crate::local::server::uri::resolve_uri(uri, data_dir);
    path.is_file().then_some(path)
}

fn resolve_existing(app: &AppHandle, uri: &str) -> Result<PathBuf, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    local_file_path(uri, &data_dir).ok_or_else(|| format!("File not found: {uri}"))
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string())
}

/// Reveal a chat file in the platform file manager (Finder on macOS).
#[tauri::command]
pub fn reveal_chat_file(app: AppHandle, uri: String) -> Result<(), String> {
    let path = resolve_existing(&app, &uri)?;
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string())
}

/// Save a chat file to a user-chosen destination via the native save dialog.
#[tauri::command]
pub async fn save_chat_file_as(app: AppHandle, uri: String, name: Option<String>) -> Result<(), String> {
    let src = resolve_existing(&app, &uri)?;
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
    if dest == src {
        return Ok(());
    }
    std::fs::copy(&src, &dest).map_err(|e| e.to_string())?;
    Ok(())
}

/// Save in-memory text content to a user-chosen destination via the native
/// save dialog (WKWebView cannot download blobs, so the webview hands the
/// already-fetched content over instead).
#[tauri::command]
pub async fn save_text_file_as(app: AppHandle, name: String, contents: String) -> Result<(), String> {
    let default_name = name.trim();
    let default_name = if default_name.is_empty() { "file.txt" } else { default_name };
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

/// Copy a chat file to the system clipboard so it can be pasted into
/// Finder or other apps.
#[tauri::command]
pub fn copy_chat_file_to_clipboard(app: AppHandle, uri: String) -> Result<(), String> {
    let path = resolve_existing(&app, &uri)?;
    copy_file_to_clipboard(&path)
}

/// Finder-style file copy: write the file URL as `NSPasteboardTypeFileURL`.
#[cfg(target_os = "macos")]
fn copy_file_to_clipboard(path: &Path) -> Result<(), String> {
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
fn copy_file_to_clipboard(_path: &Path) -> Result<(), String> {
    Err("Copy to clipboard is only supported on macOS".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("plain_reveal_{}_{}", std::process::id(), tag));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn existing_fid_file_resolves() {
        let dir = temp_root("exists");
        std::fs::create_dir_all(dir.join("files/ab/cd")).unwrap();
        let f = dir.join("files/ab/cd/abcdef.jpg");
        std::fs::write(&f, b"x").unwrap();
        assert_eq!(local_file_path("fid:abcdef.jpg", &dir), Some(f));
    }

    #[test]
    fn missing_file_returns_none() {
        let dir = temp_root("missing");
        assert_eq!(local_file_path("fid:abcdef.jpg", &dir), None);
        assert_eq!(local_file_path("app://Pictures/test.png", &dir), None);
    }

    #[test]
    fn display_name_prefers_file_name() {
        assert_eq!(display_name(Path::new("/data/files/cat.jpg")), "cat.jpg");
        assert_eq!(display_name(Path::new("file.txt")), "file.txt");
        assert_eq!(display_name(Path::new("/")), "file");
    }
}
