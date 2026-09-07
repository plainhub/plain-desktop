use std::sync::Mutex;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use plain_rs::query::url_encode;

const WARM_LABEL: &str = "media-preview-warm";
const WARM_WARMUP_PATH: &str = "/media-preview?__warm__=1";

/// State that survives a single activation: while a preview window is open
/// (whether it was the warm window promoted to visible, or a fresh one we
/// built because the warm window was already in use), we know not to spin
/// up a second warm window. Once the user closes it, the `on_window_event`
/// hook re-fires `init` and a new warm window comes back online.
#[derive(Default)]
pub struct MediaPreviewState {
    warm_label: Mutex<Option<String>>,
}

pub fn init(app: &AppHandle) {
    if let Err(e) = build_warm(app) {
        log::warn!("media_preview warm build failed: {e}");
    }
}

fn build_warm(app: &AppHandle) -> tauri::Result<String> {
    // Already warm? Nothing to do.
    if app.get_webview_window(WARM_LABEL).is_some() {
        return Ok(WARM_LABEL.to_string());
    }

    // If a previous warm window was promoted and is now visible, or a fresh
    // preview window is open, we don't want a second hidden one queued up.
    // `state.warm_label == None` is the only condition that means "no warm
    // window is being kept ready" — but we don't enforce that from here;
    // we just don't build a new warm window if one already exists by label.
    let url = WebviewUrl::App(WARM_WARMUP_PATH.into());
    let win = WebviewWindowBuilder::new(app, WARM_LABEL, url)
        .title("")
        .inner_size(1200.0, 800.0)
        .min_inner_size(900.0, 600.0)
        .visible(false);
    #[cfg(target_os = "macos")]
    let win = win.title_bar_style(tauri::TitleBarStyle::Overlay);
    let win = win.build()?;
    crate::commands::window::cascade_from_focused(app, &win);
    log::info!("media_preview: built warm window");
    Ok(WARM_LABEL.to_string())
}

fn build_query(source: &serde_json::Value) -> String {
    let mut params: Vec<String> = Vec::new();
    let push_str = |k: &str, params: &mut Vec<String>| {
        if let Some(v) = source.get(k).and_then(|x| x.as_str())
            && !v.is_empty()
        {
            params.push(format!("{}={}", k, url_encode(v)));
        }
    };
    let push_num = |k: &str, params: &mut Vec<String>| {
        if let Some(v) = source.get(k).and_then(|x| x.as_i64())
            && v != 0
        {
            params.push(format!("{}={}", k, v));
        }
    };
    let push_bool = |k: &str, params: &mut Vec<String>| {
        if source.get(k).and_then(|x| x.as_bool()).unwrap_or(false) {
            params.push(format!("{}={}", k, 1));
        }
    };

    push_str("src", &mut params);
    push_str("path", &mut params);
    push_str("name", &mut params);
    push_num("size", &mut params);
    push_num("duration", &mut params);
    push_str("fileId", &mut params);
    push_str("ext", &mut params);
    push_str("thumbnail", &mut params);
    push_bool("origin", &mut params);

    if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    }
}

pub fn activate(app: &AppHandle, source: serde_json::Value) -> String {
    let query = build_query(&source);
    let visible_path = format!("/media-preview{}", query);
    let state = app.state::<MediaPreviewState>();

    // Fast path: a hidden warm window is already waiting. Promote it.
    if let Some(warm) = app.get_webview_window(WARM_LABEL)
        && !warm.is_visible().unwrap_or(false)
    {
        navigate_and_show(&warm, &visible_path);
        if let Ok(mut g) = state.warm_label.lock() {
            *g = Some(WARM_LABEL.to_string());
        }
        log::info!("media_preview: promoted warm");
        return WARM_LABEL.to_string();
    }

    // Fallback: warm window is in use, gone, or didn't make it in time.
    // Build a brand-new window. The warm window is rebuilt next time the
    // user closes the visible one (see `on_window_event` in lib.rs).
    let label = format!(
        "media-preview-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let url = WebviewUrl::App(visible_path.into());
    let win = WebviewWindowBuilder::new(app, &label, url)
        .title("")
        .inner_size(1200.0, 800.0)
        .min_inner_size(900.0, 600.0);
    #[cfg(target_os = "macos")]
    let win = win.title_bar_style(tauri::TitleBarStyle::Overlay);
    match win.build()
    {
        Ok(win) => {
            crate::commands::window::cascade_from_focused(app, &win);
            if let Ok(mut g) = state.warm_label.lock() {
                *g = Some(label.clone());
            }
            log::info!("media_preview: built fallback {label}");
            label
        }
        Err(e) => {
            log::error!("media_preview fallback build failed: {e}");
            label
        }
    }
}

/// Drive the warm webview from one `/media-preview?src=…` URL to another
/// without a full reload.
fn navigate_and_show(win: &tauri::WebviewWindow, visible_path: &str) {
    // `location.replace` is cleaner than `href=` (no extra history entry)
    // and it's enough to trigger a vue-router resolution + a re-mount of
    // `MediaPreviewView` because the path matches the route. We also call
    // `show()` first so the user never sees a flicker of the warmup state.
    let _ = win.show();
    let script = format!(
        "window.location.replace('{}');",
        visible_path.replace('\'', "\\'")
    );
    let _ = win.eval(&script);
    let _ = win.set_focus();
}

/// Called from `on_window_event` (Destroyed) in lib.rs when ANY window
/// dies. If the dead window is one we own (the warm window or an
/// active preview), schedule a rebuild so the next click is fast again.
pub fn on_window_destroyed(app: &AppHandle, label: &str) {
    if label != WARM_LABEL && !label.starts_with("media-preview-") {
        return;
    }
    let state = app.state::<MediaPreviewState>();
    if let Ok(mut g) = state.warm_label.lock()
        && g.as_deref() == Some(label)
    {
        *g = None;
    }
    // Don't rebuild synchronously — Tauri is in the middle of disposing
    // the dead window. Yield to the runtime, then build a fresh one.
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio_sleep(50).await;
        if let Err(e) = build_warm(&app2) {
            log::warn!("media_preview warm rebuild failed: {e}");
        }
    });
}

async fn tokio_sleep(ms: u64) {
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(std::time::Duration::from_millis(ms))
    })
    .await
    .ok();
}

/// Must stay async: sync commands run on the main thread, where webview
/// eval/show/build calls deadlock on Windows (WebView2).
#[tauri::command]
pub async fn media_preview_init(app: AppHandle) {
    tauri::async_runtime::spawn_blocking(move || init(&app)).await.ok();
}

#[tauri::command]
pub async fn media_preview_activate(
    app: AppHandle,
    source: serde_json::Value,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || activate(&app, source))
        .await
        .map_err(|e| e.to_string())
}
