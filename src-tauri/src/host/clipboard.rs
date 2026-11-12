use std::future::Future;
use tauri::AppHandle;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use tauri::Manager;
use tokio::sync::watch;

#[cfg(any(target_os = "windows", test))]
mod dib;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub(crate) use linux::LinuxClipboard;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

const MAX_TEXT_UNITS: usize = 256 * 1024;

#[derive(Clone, PartialEq, Eq, serde::Serialize)]
pub(super) struct Sample {
    pub text: String,
    pub sensitive: bool,
}

pub(super) fn read<R: tauri::Runtime>(_app: &AppHandle<R>) -> Result<Option<Sample>, String> {
    #[cfg(target_os = "macos")]
    let sample = macos::read();
    #[cfg(target_os = "linux")]
    let text = _app.state::<LinuxClipboard>().read_text()?;
    #[cfg(target_os = "windows")]
    let text = windows::read_text().map_err(|error| error.to_string())?;
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    let sample = text.map(|text| Sample {
        text,
        sensitive: false,
    });

    Ok(sample.filter(|sample| {
        !sample.text.trim().is_empty() && sample.text.encode_utf16().count() <= MAX_TEXT_UNITS
    }))
}

pub(super) fn write_text<R: tauri::Runtime>(_app: &AppHandle<R>, text: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return macos::write_text(text);
    #[cfg(target_os = "linux")]
    return _app.state::<LinuxClipboard>().write_text(text);
    #[cfg(target_os = "windows")]
    return windows::write_text(window_owner(_app)?, text).map_err(|error| error.to_string());
}

pub(crate) fn write_image<R: tauri::Runtime>(
    _app: &AppHandle<R>,
    _png: &[u8],
    _decoded: plain_libs::image::DynamicImage,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return macos::write_png(_png);
    #[cfg(target_os = "linux")]
    return _app.state::<LinuxClipboard>().write_png(_png);
    #[cfg(target_os = "windows")]
    {
        let rgba = _decoded.into_rgba8();
        windows::write_image(
            window_owner(_app)?,
            _png,
            rgba.width(),
            rgba.height(),
            rgba.as_raw(),
        )
    }
}

#[cfg(target_os = "windows")]
fn window_owner<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<*mut std::ffi::c_void, String> {
    app.webview_windows()
        .into_values()
        .find_map(|window| window.hwnd().ok())
        .map(|owner| owner.0.cast())
        .ok_or_else(|| "Desktop clipboard owner window is unavailable".to_string())
}

#[cfg(target_os = "windows")]
pub(crate) fn write_file(
    path: &std::path::Path,
    window: &tauri::WebviewWindow,
) -> Result<(), String> {
    let owner = window.hwnd().map_err(|error| error.to_string())?;
    windows::write_file(owner.0.cast(), path).map_err(|error| error.to_string())
}

pub(super) async fn observe<Read, Reading, Publish, Publishing>(
    mut running: watch::Receiver<bool>,
    mut read: Read,
    mut publish: Publish,
) where
    Read: FnMut() -> Reading,
    Reading: Future<Output = Result<Option<Sample>, String>>,
    Publish: FnMut(Sample) -> Publishing,
    Publishing: Future<Output = Result<(), String>>,
{
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut previous = None;
    while *running.borrow_and_update() {
        tokio::select! {
            biased;
            result = running.changed() => {
                if result.is_err() { break; }
                continue;
            },
            _ = interval.tick() => {},
        }
        let sample = match read().await {
            Ok(sample) => sample,
            Err(error) => {
                log::warn!("clipboard observation failed: {error}");
                continue;
            }
        };
        if !*running.borrow() {
            break;
        }
        if sample == previous {
            continue;
        }
        if let Some(value) = sample.as_ref() {
            if let Err(error) = publish(value.clone()).await {
                log::warn!("clipboard report failed: {error}");
                continue;
            }
        }
        previous = sample;
    }
}

#[cfg(test)]
#[path = "../../tests/unit/host/clipboard.rs"]
mod tests;
