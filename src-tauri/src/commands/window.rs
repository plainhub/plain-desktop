use std::sync::atomic::{AtomicU32, Ordering};
#[cfg(target_os = "macos")]
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager, PhysicalPosition};

use super::webview_creation;

/// Diagonal offset (physical pixels) between a freshly opened window and
/// the focused window it cascades from, so the new window stays near its
/// parent instead of landing at the platform default position.
const WINDOW_CASCADE_OFFSET: i32 = 32;

/// Number of distinct cascade steps before the offset wraps back to the
/// base value, so windows opened in a row fan out instead of stacking up.
const WINDOW_CASCADE_STEPS: u32 = 6;

static CASCADE_STEP: AtomicU32 = AtomicU32::new(0);

/// Frame (logical coordinates) of the last closed main-view window, so a
/// dock-icon reopen puts the window back where the user left it instead of
/// at the OS default position.
#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
struct MainFrame {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[cfg(target_os = "macos")]
static LAST_MAIN_FRAME: OnceLock<Mutex<Option<MainFrame>>> = OnceLock::new();

#[cfg(target_os = "macos")]
fn last_main_frame() -> &'static Mutex<Option<MainFrame>> {
    LAST_MAIN_FRAME.get_or_init(|| Mutex::new(None))
}

/// Remember the frame of a closing main-view ("/") window. Called from
/// `CloseRequested`, while the window is still alive and readable.
#[cfg(target_os = "macos")]
pub fn remember_main_window_frame(app: &AppHandle, label: &str) {
    let Some(win) = app.get_webview_window(label) else {
        return;
    };
    if win.url().map(|u| u.path() != "/").unwrap_or(true) {
        return;
    }
    let (Ok(pos), Ok(size), Ok(scale)) =
        (win.outer_position(), win.inner_size(), win.scale_factor())
    else {
        return;
    };
    let pos = pos.to_logical::<f64>(scale);
    let size = size.to_logical::<f64>(scale);
    if let Ok(mut frame) = last_main_frame().lock() {
        *frame = Some(MainFrame {
            x: pos.x,
            y: pos.y,
            w: size.width,
            h: size.height,
        });
    }
}

/// Dock-icon reopen: restore the remembered frame of the last closed
/// main-view window, or center a default-sized one when nothing was
/// recorded yet.
#[cfg(target_os = "macos")]
pub fn reopen_main_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.set_focus();
        return;
    }
    let label = format!(
        "window-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let builder = tauri::WebviewWindowBuilder::new(app, &label, tauri::WebviewUrl::App("/".into()))
        .title("")
        .min_inner_size(900.0, 600.0)
        .disable_drag_drop_handler()
        .title_bar_style(tauri::TitleBarStyle::Overlay);
    let builder = match last_main_frame().lock().ok().and_then(|f| *f) {
        Some(f) => builder.position(f.x, f.y).inner_size(f.w, f.h),
        None => builder.inner_size(1200.0, 800.0).center(),
    };
    if let Err(e) = builder.build() {
        log::error!("reopen_main_window failed: {e}");
    }
}

/// Place a freshly built window diagonally offset from the current focused
/// window (macOS-style cascade) so it stays near its parent. Successive
/// opens step further down-right and wrap after `WINDOW_CASCADE_STEPS`.
/// The new window itself is excluded from the focus search — on some
/// platforms it may already be focused by the time this runs.
/// Falls back to the platform default position (i.e. no-op) when there is
/// no focused window or its outer geometry can't be read — we never want
/// to fail a window open because the cascade placement didn't work.
pub fn cascade_from_focused(app: &AppHandle, win: &tauri::WebviewWindow) {
    let windows = app.webview_windows();
    let Some(focused) = windows
        .values()
        .find(|w| w.label() != win.label() && w.is_focused().unwrap_or(false))
    else {
        return;
    };
    let Ok(origin) = focused.outer_position() else {
        return;
    };
    let step = CASCADE_STEP.fetch_add(1, Ordering::Relaxed);
    let new_pos = cascaded_position(origin, step);
    if let Err(e) = win.set_position(new_pos) {
        log::warn!("cascade_from_focused: set_position failed: {e}");
    }
}

fn cascaded_position(origin: PhysicalPosition<i32>, step: u32) -> PhysicalPosition<i32> {
    let delta = WINDOW_CASCADE_OFFSET * (1 + (step % WINDOW_CASCADE_STEPS) as i32);
    PhysicalPosition::new(
        origin.x.saturating_add(delta),
        origin.y.saturating_add(delta),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_open_offsets_diagonally_from_origin() {
        let origin = PhysicalPosition::new(100, 200);
        assert_eq!(
            cascaded_position(origin, 0),
            PhysicalPosition::new(132, 232)
        );
    }

    #[test]
    fn successive_opens_step_further() {
        let origin = PhysicalPosition::new(0, 0);
        assert_eq!(cascaded_position(origin, 1), PhysicalPosition::new(64, 64));
        assert_eq!(cascaded_position(origin, 2), PhysicalPosition::new(96, 96));
    }

    #[test]
    fn offset_wraps_after_max_steps() {
        let origin = PhysicalPosition::new(50, 60);
        let last = cascaded_position(origin, WINDOW_CASCADE_STEPS - 1);
        assert_eq!(last, PhysicalPosition::new(50 + 32 * 6, 60 + 32 * 6));
        assert_eq!(
            cascaded_position(origin, WINDOW_CASCADE_STEPS),
            cascaded_position(origin, 0)
        );
    }

    #[test]
    fn saturates_at_coordinate_bounds() {
        let origin = PhysicalPosition::new(i32::MAX, i32::MIN);
        assert_eq!(
            cascaded_position(origin, 0),
            PhysicalPosition::new(i32::MAX, i32::MIN + WINDOW_CASCADE_OFFSET)
        );
    }
}

/// Core logic — usable from both the Tauri command and the native menu handler.
pub fn create_window(app: &AppHandle, path: String) {
    // Reuse an existing window with the same path if one is already open.
    for win in app.webview_windows().values() {
        if win.url().map(|u| u.path() == path).unwrap_or(false) {
            let _ = win.set_focus();
            return;
        }
    }

    let label = format!(
        "window-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let url = tauri::WebviewUrl::App(path.into());
    // The native drag-drop handler swallows OS drops before the webview sees
    // them, which kills the HTML5 drop zones used to upload files.
    let win = tauri::WebviewWindowBuilder::new(app, &label, url)
        .title("")
        .inner_size(1200.0, 800.0)
        .min_inner_size(900.0, 600.0)
        .disable_drag_drop_handler();
    #[cfg(target_os = "macos")]
    let win = win.title_bar_style(tauri::TitleBarStyle::Overlay);
    match webview_creation::serialized(|| win.build()) {
        Ok(win) => cascade_from_focused(app, &win),
        Err(e) => log::error!("open_window failed: {e}"),
    }
}

/// Open the About window at "/about". Existing window is focused instead.
/// The view runs the update check automatically on mount.
pub fn open_about(app: &AppHandle) {
    let label = "about";
    if let Some(win) = app.get_webview_window(label) {
        let _ = win.set_focus();
        return;
    }
    #[cfg(target_os = "macos")]
    let title = super::macos_menu::about_title(app);
    #[cfg(not(target_os = "macos"))]
    let title = "About PlainApp".to_string();
    let win = tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::App("/about".into()))
        .title(title)
        .inner_size(400.0, 560.0)
        .resizable(false)
        .center();
    #[cfg(target_os = "macos")]
    let win = win.title_bar_style(tauri::TitleBarStyle::Overlay);
    if let Err(e) = win.build() {
        log::error!("open_about failed: {e}");
    }
}

/// Open the About window from the in-app rail menu (Windows/Linux —
/// macOS reaches it via the app menu). Reuses open_about.
/// Must stay async: sync commands run on the main thread, where
/// WebviewWindowBuilder::build() deadlocks on Windows (WebView2).
#[tauri::command]
pub async fn open_about_window(app: AppHandle) {
    tauri::async_runtime::spawn_blocking(move || open_about(&app))
        .await
        .ok();
}

/// Always creates a new window at "/" without checking for an existing one.
/// Used by the macOS dock "New Window" action.
pub fn new_window(app: &AppHandle) {
    let label = format!(
        "window-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let url = tauri::WebviewUrl::App("/".into());
    let win = tauri::WebviewWindowBuilder::new(app, &label, url)
        .title("")
        .inner_size(1200.0, 800.0)
        .min_inner_size(900.0, 600.0)
        .disable_drag_drop_handler();
    #[cfg(target_os = "macos")]
    let win = win.title_bar_style(tauri::TitleBarStyle::Overlay);
    if let Err(e) = win.build() {
        log::error!("new_window failed: {e}");
    }
}

/// Open a new app window at the given path (e.g. "/messages").
/// If a window at that exact path is already open it receives focus instead.
/// Must stay async: sync commands run on the main thread, where
/// WebviewWindowBuilder::build() deadlocks on Windows (WebView2).
#[tauri::command]
pub async fn open_window(app: AppHandle, path: String) {
    if let Err(error) =
        tauri::async_runtime::spawn_blocking(move || create_window(&app, path)).await
    {
        log::error!("open_window worker failed: {error}");
    }
}

/// Update the display name shown for this window in the macOS dock right-click menu.
/// The frontend calls this whenever the active device session changes.
#[tauri::command]
pub fn set_window_device_name(_window: tauri::Window, _name: String) {
    #[cfg(target_os = "macos")]
    crate::commands::macos_dock::set_window_device_name(_window.label(), &_name);
}
