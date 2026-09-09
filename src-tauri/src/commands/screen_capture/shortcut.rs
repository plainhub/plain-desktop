use tauri::{AppHandle, Manager, Runtime, plugin::TauriPlugin};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use super::commands::{finish_reserved_capture, new_capture_session_id, schedule_capture_timeouts};
use super::contract::{CaptureError, CaptureErrorCode, CaptureOrigin};
use super::runtime::ScreenCaptureRuntime;
use super::window::TauriCaptureWindowPort;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureShortcutPlatform {
    MacOs,
    OtherDesktop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxShortcutBackend {
    OrdinaryPlugin,
    WaylandPortalRequired,
}

pub fn capture_shortcut_accelerator(platform: CaptureShortcutPlatform) -> &'static str {
    match platform {
        CaptureShortcutPlatform::MacOs => "Option+Command+A",
        CaptureShortcutPlatform::OtherDesktop => "Alt+A",
    }
}

/// The ordinary Tauri plugin constructs an X11 global-hotkey manager during
/// plugin setup. Never attach it to a native Wayland process: use the portal
/// adapter instead so an unavailable X display cannot abort application setup.
pub fn linux_shortcut_backend(
    session_type: Option<&str>,
    wayland_display_present: bool,
) -> LinuxShortcutBackend {
    if session_type.is_some_and(|value| value.eq_ignore_ascii_case("wayland"))
        || wayland_display_present
    {
        LinuxShortcutBackend::WaylandPortalRequired
    } else {
        LinuxShortcutBackend::OrdinaryPlugin
    }
}

pub fn ordinary_shortcut_plugin<R: Runtime>() -> Option<TauriPlugin<R>> {
    #[cfg(target_os = "linux")]
    if current_linux_shortcut_backend() == LinuxShortcutBackend::WaylandPortalRequired {
        return None;
    }
    Some(tauri_plugin_global_shortcut::Builder::new().build())
}

pub fn register_ordinary_capture_shortcut<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<(), CaptureError> {
    #[cfg(target_os = "linux")]
    if current_linux_shortcut_backend() == LinuxShortcutBackend::WaylandPortalRequired {
        return Err(CaptureError::new(
            CaptureErrorCode::CaptureFailed,
            "the X11 global shortcut plugin is disabled for a Wayland session",
        ));
    }
    app.global_shortcut()
        .on_shortcut(current_capture_accelerator(), |app, _, event| {
            if event.state == ShortcutState::Pressed {
                log::info!("global screen capture shortcut pressed");
                trigger_global_capture(app);
            }
        })
        .map_err(|_| {
            CaptureError::new(
                CaptureErrorCode::CaptureFailed,
                "could not register the screen capture global shortcut",
            )
        })
}

fn current_capture_accelerator() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        capture_shortcut_accelerator(CaptureShortcutPlatform::MacOs)
    }
    #[cfg(not(target_os = "macos"))]
    {
        capture_shortcut_accelerator(CaptureShortcutPlatform::OtherDesktop)
    }
}

fn trigger_global_capture<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let reservation_app = app.clone();
        let reservation = tauri::async_runtime::spawn_blocking(move || {
            let runtime = reservation_app.state::<ScreenCaptureRuntime>();
            let windows = TauriCaptureWindowPort::new(reservation_app.clone());
            let origin = focused_regular_window_label(&reservation_app)
                .map(|window_label| CaptureOrigin { window_label });
            runtime.reserve_global_with_registered_target_capture(
                new_capture_session_id(),
                origin,
                &windows,
            )
        })
        .await;
        let reservation = match reservation {
            Ok(reservation) => reservation,
            Err(error) => {
                log::warn!("global screen capture reservation worker failed: {error}");
                return;
            }
        };
        let runtime = app.state::<ScreenCaptureRuntime>();
        match reservation {
            Ok(reservation) => {
                log::info!(
                    "global screen capture reserved generation={} phase={:?} immediate_capture={}",
                    reservation.response.overlay_generation,
                    reservation.response.phase,
                    reservation.ticket.is_some()
                );
                schedule_capture_timeouts(app.clone(), &reservation.response);
                if let Some(ticket) = reservation.ticket {
                    match finish_reserved_capture(app.clone(), &runtime, ticket).await {
                        Ok(outcome) => {
                            log::info!(
                                "global screen capture acquisition completed outcome={outcome:?}"
                            )
                        }
                        Err(error) if error.code != CaptureErrorCode::Busy => {
                            log::warn!("global screen capture trigger failed: {error}");
                        }
                        Err(_) => {}
                    }
                }
            }
            Err(error) if error.code != CaptureErrorCode::Busy => {
                log::warn!("global screen capture trigger failed: {error}");
            }
            Err(_) => {}
        }
    });
}

fn focused_regular_window_label<R: Runtime>(app: &AppHandle<R>) -> Option<String> {
    select_global_capture_origin(app.webview_windows().into_iter().map(|(label, window)| {
        let focused = window.is_focused().unwrap_or(false);
        (label, focused)
    }))
}

fn select_global_capture_origin<I, S>(windows: I) -> Option<String>
where
    I: IntoIterator<Item = (S, bool)>,
    S: AsRef<str>,
{
    windows
        .into_iter()
        .find(|(label, focused)| {
            *focused && super::runtime::is_regular_window_label(label.as_ref())
        })
        .map(|(label, _)| label.as_ref().to_string())
}

#[cfg(target_os = "linux")]
pub struct WaylandPortalCaptureShortcut {
    listener: tauri::async_runtime::JoinHandle<()>,
}

#[cfg(target_os = "linux")]
impl Drop for WaylandPortalCaptureShortcut {
    fn drop(&mut self) {
        self.listener.abort();
    }
}

/// Registers the issue-defined capture shortcut through the sanctioned XDG
/// portal. The returned guard owns the listener and must be retained in Tauri
/// managed state for the application lifetime.
#[cfg(target_os = "linux")]
pub async fn register_wayland_portal_capture_shortcut<R: Runtime>(
    app: AppHandle<R>,
) -> Result<WaylandPortalCaptureShortcut, CaptureError> {
    use ashpd::desktop::CreateSessionOptions;
    use ashpd::desktop::global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut};
    use futures_util::StreamExt;

    if current_linux_shortcut_backend() != LinuxShortcutBackend::WaylandPortalRequired {
        return Err(CaptureError::new(
            CaptureErrorCode::CaptureFailed,
            "the Wayland shortcut portal is not required for this desktop session",
        ));
    }
    let proxy = GlobalShortcuts::new().await.map_err(portal_error)?;
    let session = proxy
        .create_session(CreateSessionOptions::default())
        .await
        .map_err(portal_error)?;
    let preferred = to_portal_trigger(current_capture_accelerator());
    let shortcut = NewShortcut::new("plain-screen-capture", "Open Plain screen capture")
        .preferred_trigger(preferred.as_deref());
    let response = proxy
        .bind_shortcuts(&session, &[shortcut], None, BindShortcutsOptions::default())
        .await
        .map_err(portal_error)?
        .response()
        .map_err(portal_error)?;
    if !response
        .shortcuts()
        .iter()
        .any(|shortcut| shortcut.id() == "plain-screen-capture")
    {
        return Err(CaptureError::new(
            CaptureErrorCode::PermissionDenied,
            "the desktop portal did not grant the screen capture shortcut",
        ));
    }
    let mut activated = proxy.receive_activated().await.map_err(portal_error)?;
    let listener = tauri::async_runtime::spawn(async move {
        // Keep the portal session alive for the complete signal stream lifetime.
        let _session = session;
        while let Some(event) = activated.next().await {
            if event.shortcut_id() == "plain-screen-capture" {
                trigger_global_capture(&app);
            }
        }
    });
    Ok(WaylandPortalCaptureShortcut { listener })
}

#[cfg(target_os = "linux")]
fn portal_error(_: impl std::fmt::Display) -> CaptureError {
    CaptureError::new(
        CaptureErrorCode::CaptureFailed,
        "the Wayland global shortcut portal is unavailable",
    )
}

#[cfg(target_os = "linux")]
fn to_portal_trigger(combo: &str) -> Option<String> {
    let trimmed = combo.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(
        trimmed
            .split('+')
            .map(str::trim)
            .enumerate()
            .map(|(index, part)| {
                let is_key = index == trimmed.split('+').count() - 1;
                if is_key {
                    part.strip_prefix("Key")
                        .unwrap_or(part)
                        .to_ascii_lowercase()
                } else if part.eq_ignore_ascii_case("alt") || part.eq_ignore_ascii_case("option") {
                    "ALT".to_string()
                } else if part.eq_ignore_ascii_case("command")
                    || part.eq_ignore_ascii_case("super")
                    || part.eq_ignore_ascii_case("meta")
                {
                    "LOGO".to_string()
                } else if part.eq_ignore_ascii_case("control") || part.eq_ignore_ascii_case("ctrl")
                {
                    "CTRL".to_string()
                } else if part.eq_ignore_ascii_case("shift") {
                    "SHIFT".to_string()
                } else {
                    part.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("+"),
    )
}

#[cfg(target_os = "linux")]
pub fn current_linux_shortcut_backend() -> LinuxShortcutBackend {
    linux_shortcut_backend(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_defined_shortcuts_map_exactly_by_platform() {
        assert_eq!(
            capture_shortcut_accelerator(CaptureShortcutPlatform::MacOs),
            "Option+Command+A"
        );
        assert_eq!(
            capture_shortcut_accelerator(CaptureShortcutPlatform::OtherDesktop),
            "Alt+A"
        );
    }

    #[test]
    fn any_wayland_signal_blocks_the_x11_only_plugin() {
        for (session, display) in [
            (Some("wayland"), false),
            (Some("WAYLAND"), false),
            (None, true),
            (Some("x11"), true),
        ] {
            assert_eq!(
                linux_shortcut_backend(session, display),
                LinuxShortcutBackend::WaylandPortalRequired
            );
        }
    }

    #[test]
    fn x11_and_non_wayland_sessions_allow_the_ordinary_plugin() {
        for session in [None, Some("x11"), Some("tty")] {
            assert_eq!(
                linux_shortcut_backend(session, false),
                LinuxShortcutBackend::OrdinaryPlugin
            );
        }
    }

    #[test]
    fn global_capture_hides_only_the_focused_regular_plain_window() {
        assert_eq!(
            select_global_capture_origin([
                ("screen-capture-overlay-7", true),
                ("window-background", false),
                ("main", true),
            ]),
            Some("main".to_string())
        );
        assert_eq!(
            select_global_capture_origin([("main", false), ("window-chat", true)]),
            Some("window-chat".to_string())
        );
    }

    #[test]
    fn global_capture_keeps_external_apps_visible_when_plain_is_not_focused() {
        assert_eq!(
            select_global_capture_origin([("main", false), ("screen-capture-overlay-7", false)]),
            None
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn portal_trigger_uses_xdg_modifier_and_keysym_names() {
        assert_eq!(to_portal_trigger("Alt+A").as_deref(), Some("ALT+a"));
        assert_eq!(
            to_portal_trigger("Option+Command+KeyA").as_deref(),
            Some("ALT+LOGO+a")
        );
    }
}
