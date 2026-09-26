use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime, plugin::TauriPlugin};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use super::commands::{finish_reserved_capture, new_capture_session_id, schedule_capture_timeouts};
use super::contract::{CaptureError, CaptureErrorCode, CaptureOrigin};
use super::runtime::ScreenCaptureRuntime;
use super::window::TauriCaptureWindowPort;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureShortcutPlatform {
    MacOs,
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    OtherDesktop,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxShortcutBackend {
    OrdinaryPlugin,
    WaylandPortalRequired,
}

/// Conflict-free defaults: QQ uses ⌃⌘A, WeChat covers ⌥⌘A/⌘⇧A/Alt+A and
/// DingTalk ⌘⇧A, so the two-modifier A combos are all taken. The
/// three-modifier A keeps the ecosystem's "A = capture" muscle memory without
/// colliding; Windows/Linux move off the A family entirely (Ctrl+Alt+X,
/// Xnip-style "X = select a region").
pub fn capture_shortcut_accelerator(platform: CaptureShortcutPlatform) -> &'static str {
    match platform {
        CaptureShortcutPlatform::MacOs => "Control+Option+Command+A",
        CaptureShortcutPlatform::OtherDesktop => "Ctrl+Alt+X",
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureShortcutStatus {
    pub registered: bool,
    pub accelerator: String,
    pub error: Option<String>,
}

static SHORTCUT_STATUS: std::sync::Mutex<Option<CaptureShortcutStatus>> =
    std::sync::Mutex::new(None);

fn record_status(status: CaptureShortcutStatus) {
    *SHORTCUT_STATUS.lock().expect("shortcut status lock") = Some(status);
}

pub fn snapshot_status() -> CaptureShortcutStatus {
    SHORTCUT_STATUS
        .lock()
        .expect("shortcut status lock")
        .clone()
        .unwrap_or_else(|| CaptureShortcutStatus {
            registered: false,
            accelerator: default_capture_accelerator().to_string(),
            error: None,
        })
}

fn default_capture_accelerator() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        capture_shortcut_accelerator(CaptureShortcutPlatform::MacOs)
    }
    #[cfg(not(target_os = "macos"))]
    {
        capture_shortcut_accelerator(CaptureShortcutPlatform::OtherDesktop)
    }
}

/// Resolve the accelerator to register: the persisted user choice when it
/// parses, otherwise the platform default.
fn configured_capture_accelerator<R: Runtime>(app: &AppHandle<R>) -> String {
    let prefs = app.state::<std::sync::Arc<crate::prefs::Prefs>>();
    crate::prefs::get_capture_shortcut(&prefs)
        .filter(|value| value.parse::<Shortcut>().is_ok())
        .unwrap_or_else(|| default_capture_accelerator().to_string())
}

/// Reject bare-key accelerators: a global hotkey without modifiers would
/// swallow ordinary typing system-wide.
fn validate_accelerator(accelerator: &str) -> Result<Shortcut, CaptureError> {
    let shortcut: Shortcut = accelerator.parse().map_err(|_| {
        CaptureError::new(
            CaptureErrorCode::CaptureFailed,
            "the shortcut is not a valid accelerator",
        )
    })?;
    if shortcut.mods.is_empty() {
        return Err(CaptureError::new(
            CaptureErrorCode::CaptureFailed,
            "the shortcut must include at least one modifier key",
        ));
    }
    Ok(shortcut)
}

/// Apply a user shortcut change: unregister the current binding, persist the
/// choice (`None` restores the platform default), and register the new one.
/// The Wayland portal binds shortcuts at session creation, so a change only
/// takes effect after an app restart there.
pub(crate) fn apply_shortcut_change<R: Runtime>(
    app: &AppHandle<R>,
    accelerator: Option<String>,
) -> CaptureShortcutStatus {
    #[cfg(target_os = "linux")]
    if current_linux_shortcut_backend() == LinuxShortcutBackend::WaylandPortalRequired {
        let prefs = app.state::<std::sync::Arc<crate::prefs::Prefs>>();
        crate::prefs::set_capture_shortcut(&prefs, accelerator.as_deref());
        let status = CaptureShortcutStatus {
            registered: false,
            accelerator: accelerator.unwrap_or_else(|| default_capture_accelerator().to_string()),
            error: Some(
                "the shortcut is saved; restart the app for Wayland to rebind it".to_string(),
            ),
        };
        record_status(status.clone());
        return status;
    }

    let choice = accelerator.filter(|value| !value.trim().is_empty());
    if let Some(value) = &choice
        && let Err(error) = validate_accelerator(value)
    {
        let status = CaptureShortcutStatus {
            registered: snapshot_status().registered,
            accelerator: value.clone(),
            error: Some(error.to_string()),
        };
        record_status(status.clone());
        return status;
    }

    if let Ok(current) = configured_capture_accelerator(app).parse::<Shortcut>() {
        let _ = app.global_shortcut().unregister(current);
    }

    let prefs = app.state::<std::sync::Arc<crate::prefs::Prefs>>();
    crate::prefs::set_capture_shortcut(&prefs, choice.as_deref());
    let wanted = configured_capture_accelerator(app);

    let status = match register_ordinary_capture_shortcut(app) {
        Ok(()) => CaptureShortcutStatus {
            registered: true,
            accelerator: wanted,
            error: None,
        },
        Err(error) => CaptureShortcutStatus {
            registered: false,
            accelerator: wanted,
            error: Some(error.to_string()),
        },
    };
    record_status(status.clone());
    status
}

/// The ordinary Tauri plugin constructs an X11 global-hotkey manager during
/// plugin setup. Never attach it to a native Wayland process: use the portal
/// adapter instead so an unavailable X display cannot abort application setup.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
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
    let accelerator = configured_capture_accelerator(app);
    let registration = app
        .global_shortcut()
        .on_shortcut(accelerator.as_str(), |app, _, event| {
            if event.state == ShortcutState::Pressed {
                log::info!("global screen capture shortcut pressed");
                trigger_global_capture(app);
            }
        });
    let status = match registration {
        Ok(()) => CaptureShortcutStatus {
            registered: true,
            accelerator: accelerator.clone(),
            error: None,
        },
        Err(_) => CaptureShortcutStatus {
            registered: false,
            accelerator: accelerator.clone(),
            error: Some("could not register the screen capture global shortcut".to_string()),
        },
    };
    record_status(status);
    registration.map_err(|_| {
        CaptureError::new(
            CaptureErrorCode::CaptureFailed,
            "could not register the screen capture global shortcut",
        )
    })
}

pub(crate) fn trigger_global_capture<R: Runtime>(app: &AppHandle<R>) {
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
    let preferred = to_portal_trigger(configured_capture_accelerator(&app).as_str());
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
    record_status(CaptureShortcutStatus {
        registered: true,
        accelerator: configured_capture_accelerator(&app),
        error: None,
    });
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
    fn issue_defined_shortcuts_avoid_the_im_squatted_a_combos() {
        assert_eq!(
            capture_shortcut_accelerator(CaptureShortcutPlatform::MacOs),
            "Control+Option+Command+A"
        );
        assert_eq!(
            capture_shortcut_accelerator(CaptureShortcutPlatform::OtherDesktop),
            "Ctrl+Alt+X"
        );
    }

    #[test]
    fn bare_keys_are_rejected_and_modifiers_are_required() {
        assert!(validate_accelerator("A").is_err());
        assert!(validate_accelerator("NotARealThing").is_err());
        assert!(validate_accelerator("").is_err());
        validate_accelerator("Ctrl+Alt+X").expect("valid accelerator");
        validate_accelerator("Control+Option+Command+A").expect("valid mac accelerator");
    }

    #[test]
    fn status_snapshot_defaults_to_the_platform_default_unregistered() {
        let status = snapshot_status();
        assert!(!status.registered);
        assert_eq!(status.accelerator, default_capture_accelerator());
        assert!(status.error.is_none());
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
