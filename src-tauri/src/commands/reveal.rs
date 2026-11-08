use std::path::{Path, PathBuf};

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

fn existing_file(path: &str) -> Result<PathBuf, String> {
    plain_libs::filesystem::export::existing_file(Path::new(path))
        .map_err(|error| error.to_string())
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
    tokio::task::spawn_blocking(move || plain_libs::filesystem::export::copy_file(&src, &dest))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn copy_file_to_clipboard(
    window: tauri::WebviewWindow,
    path: String,
) -> Result<(), String> {
    let path = existing_file(&path)?;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let native_window = window.clone();
    window
        .run_on_main_thread(move || {
            let _ = sender.send(write_file_clipboard(&path, &native_window));
        })
        .map_err(|error| error.to_string())?;
    receiver.await.map_err(|error| error.to_string())?
}

/// Finder-style file copy: write the file URL as `NSPasteboardTypeFileURL`.
#[cfg(target_os = "macos")]
fn write_file_clipboard(path: &Path, _window: &tauri::WebviewWindow) -> Result<(), String> {
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

#[cfg(target_os = "linux")]
fn write_file_clipboard(path: &Path, _window: &tauri::WebviewWindow) -> Result<(), String> {
    use gtk::gio::prelude::FileExt;

    let display = gtk::gdk::Display::default().ok_or("Desktop display is unavailable")?;
    let clipboard = gtk::Clipboard::default(&display).ok_or("Desktop clipboard is unavailable")?;
    let uri = gtk::gio::File::for_path(path).uri();
    let targets = [
        gtk::TargetEntry::new("text/uri-list", gtk::TargetFlags::empty(), 0),
        gtk::TargetEntry::new("x-special/gnome-copied-files", gtk::TargetFlags::empty(), 1),
        gtk::TargetEntry::new(
            "application/x-kde-cutselection",
            gtk::TargetFlags::empty(),
            2,
        ),
    ];
    if !clipboard.set_with_data(&targets, move |_, selection, format| {
        let data = match format {
            1 => format!("copy\n{uri}"),
            2 => "0".to_string(),
            _ => format!("{uri}\r\n"),
        };
        selection.set(&selection.target(), 8, data.as_bytes());
    }) {
        return Err("Failed to write files to the desktop clipboard".to_string());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
use crate::host::clipboard::write_file as write_file_clipboard;

#[cfg(test)]
#[path = "../../tests/unit/commands/reveal.rs"]
mod tests;
