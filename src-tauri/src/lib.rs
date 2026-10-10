mod bootstrap;
mod capture;
mod commands;
mod host;
mod prefs;
mod utils;

use std::sync::Arc;
use tauri::Manager;

const _: &[u8] = include_bytes!("../icons/icon.icns");

pub use bootstrap::run;

/// macOS dock-icon click with no visible window reopens the main window —
/// the standard `applicationShouldHandleReopen` behavior. With visible
/// windows the default activation already brings the app forward.
#[cfg(target_os = "macos")]
fn handle_run_event(app: &tauri::AppHandle, event: tauri::RunEvent) {
    if let tauri::RunEvent::Reopen {
        has_visible_windows: false,
        ..
    } = event
    {
        crate::commands::window::reopen_main_window(app);
    }
}

#[cfg(not(target_os = "macos"))]
fn handle_run_event(_app: &tauri::AppHandle, _event: tauri::RunEvent) {}

/// Whether any window is (or may be) visible — an `Err` from `is_visible`
/// counts as visible so a flaky query can never exit a live app.
/// Only consulted on Windows/Linux; macOS never exits on window close.
#[cfg_attr(not(any(target_os = "windows", target_os = "linux")), allow(dead_code))]
fn any_window_visible(mut visibilities: impl Iterator<Item = Option<bool>>) -> bool {
    visibilities.any(|v| v.unwrap_or(true))
}

/// Capture sessions intentionally destroy their ephemeral utility webview.
/// That internal lifecycle event must never be interpreted as the user
/// closing Plain's final application window. Media-preview windows retain
/// their existing last-visible-window behavior.
#[cfg_attr(
    not(any(target_os = "windows", target_os = "linux", test)),
    allow(dead_code)
)]
fn should_check_app_exit_after_destroy(label: &str) -> bool {
    !crate::commands::screen_capture::runtime::is_overlay_window_label(label)
}

/// A Windows capture bootstrap is intentionally mapped as an opaque,
/// click-through one-pixel anchor so WebView2 can initialize reliably. It is
/// infrastructure, not user-visible application state, and must not turn a
/// closed Plain instance into a background zombie.
#[cfg_attr(
    not(any(target_os = "windows", target_os = "linux", test)),
    allow(dead_code)
)]
fn keeps_process_alive(label: &str) -> bool {
    !crate::commands::screen_capture::runtime::is_overlay_window_label(label)
}

/// Choose the regular application window that a second launch should reveal.
/// Hidden dynamic windows remain valid recovery targets after a failed capture;
/// utility and capture-overlay windows must never be surfaced as the main UI.
#[cfg_attr(
    not(any(target_os = "windows", target_os = "linux", test)),
    allow(dead_code)
)]
fn select_reopen_window_label<I, S>(windows: I) -> Option<String>
where
    I: IntoIterator<Item = (S, Option<bool>)>,
    S: AsRef<str>,
{
    let mut visible_dynamic = None;
    let mut hidden_dynamic = None;
    for (label, visibility) in windows {
        let label = label.as_ref();
        if !crate::commands::screen_capture::runtime::is_regular_window_label(label) {
            continue;
        }
        if label == "main" {
            return Some(label.to_string());
        }
        if visibility.unwrap_or(true) {
            visible_dynamic.get_or_insert_with(|| label.to_string());
        } else {
            hidden_dynamic.get_or_insert_with(|| label.to_string());
        }
    }
    visible_dynamic.or(hidden_dynamic)
}

#[cfg(test)]
mod tests {
    use super::any_window_visible;
    use super::keeps_process_alive;
    use super::select_reopen_window_label;
    use super::should_check_app_exit_after_destroy;

    #[test]
    fn relaunch_can_recover_a_hidden_dynamic_application_window() {
        assert_eq!(
            select_reopen_window_label([
                ("screen-capture-overlay-7", Some(false)),
                ("media-preview-warm", Some(false)),
                ("window-chat", Some(false)),
            ]),
            Some("window-chat".to_string())
        );
    }

    #[test]
    fn hidden_only_windows_do_not_keep_app_alive() {
        assert!(!any_window_visible([Some(false), Some(false)].into_iter()));
    }

    #[test]
    fn one_visible_window_keeps_app_alive() {
        assert!(any_window_visible([Some(false), Some(true)].into_iter()));
    }

    #[test]
    fn empty_window_set_exits() {
        assert!(!any_window_visible(std::iter::empty()));
    }

    #[test]
    fn unknown_visibility_counts_as_visible() {
        assert!(any_window_visible([None].into_iter()));
    }

    #[test]
    fn capture_overlay_destruction_never_runs_the_application_exit_rule() {
        assert!(!should_check_app_exit_after_destroy(
            "screen-capture-overlay-7"
        ));
        assert!(should_check_app_exit_after_destroy(
            "screen-capture-overlay-07"
        ));
        assert!(should_check_app_exit_after_destroy("main"));
        assert!(should_check_app_exit_after_destroy("window-chat"));
        assert!(should_check_app_exit_after_destroy("media-preview-1"));
    }

    #[test]
    fn capture_bootstrap_does_not_keep_a_closed_application_alive() {
        assert!(!keeps_process_alive("screen-capture-overlay-7"));
        assert!(keeps_process_alive("screen-capture-overlay-07"));
        assert!(keeps_process_alive("main"));
        assert!(keeps_process_alive("window-chat"));
        assert!(keeps_process_alive("media-preview-1"));
    }
}
