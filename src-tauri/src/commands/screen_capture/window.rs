use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};
#[cfg(not(target_os = "windows"))]
use tauri::{LogicalPosition, LogicalSize};
#[cfg(target_os = "windows")]
use tauri::{PhysicalPosition, PhysicalSize};

#[cfg(target_os = "windows")]
use crate::commands::webview_creation;

use super::contract::{CaptureError, CaptureErrorCode, MonitorGeometry};
use super::runtime::FRAME_AVAILABLE_EVENT;
use super::runtime::{
    CaptureWindowPort, CaptureWindowState, DELIVERY_FAILED_EVENT, DeliveryFailedPayload,
    FrameAvailablePayload, OVERLAY_SESSION_ENDED_EVENT, OverlayConcealment,
    OverlaySessionEndedPayload, OverlayWindowSpec, RESULT_AVAILABLE_EVENT, ResultAvailablePayload,
    SESSION_ENDED_EVENT, SESSION_STARTED_EVENT, ScreenCaptureRuntime, SessionEndedPayload,
    SessionStartedPayload, TARGET_UNAVAILABLE_EVENT, TargetUnavailablePayload,
};

/// The production implementation of the small synchronous window interface
/// used by the capture runtime. Keeping Tauri handles out of the coordinator
/// makes lifecycle and ordering behavior testable without a webview.
pub struct TauriCaptureWindowPort<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> TauriCaptureWindowPort<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }

    fn webview(&self, label: &str) -> Result<tauri::WebviewWindow<R>, CaptureError> {
        self.app.get_webview_window(label).ok_or_else(|| {
            CaptureError::new(
                CaptureErrorCode::OverlayFailed,
                format!("capture window '{label}' is unavailable"),
            )
        })
    }

    fn overlay_builder<'a>(
        app: &'a AppHandle<R>,
        spec: &OverlayWindowSpec,
    ) -> WebviewWindowBuilder<'a, R, AppHandle<R>> {
        WebviewWindowBuilder::new(app, &spec.label, WebviewUrl::App(spec.route.clone().into()))
            .title("")
            .focused(false)
            .focusable(false)
            .decorations(false)
            .resizable(false)
            .maximizable(false)
            .minimizable(false)
            .closable(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .shadow(false)
    }

    #[cfg(target_os = "windows")]
    fn show_overlay_for_bootstrap(window: &tauri::WebviewWindow<R>) -> Result<(), CaptureError> {
        log::info!("screen capture bootstrapping mapped Windows overlay anchor");
        window
            .set_ignore_cursor_events(true)
            .map_err(|error| overlay_error("make capture bootstrap click-through", error))?;
        window
            .show()
            .map_err(|error| overlay_error("show capture bootstrap anchor", error))?;
        let webview: &tauri::Webview<R> = window.as_ref();
        webview
            .show()
            .map_err(|error| overlay_error("show capture bootstrap webview", error))
    }

    #[cfg(target_os = "windows")]
    fn create_windows_overlay(&self, spec: &OverlayWindowSpec) -> Result<(), CaptureError> {
        webview_creation::serialized(|| self.create_windows_overlay_serialized(spec))
    }

    #[cfg(target_os = "windows")]
    fn create_windows_overlay_serialized(
        &self,
        spec: &OverlayWindowSpec,
    ) -> Result<(), CaptureError> {
        log::info!("screen capture creating serialized Windows capture overlay");
        let window = Self::overlay_builder(&self.app, spec)
            .inner_size(1.0, 1.0)
            .position(0.0, 0.0)
            .transparent(false)
            .visible(true)
            .build()
            .map_err(|error| overlay_error("create Windows capture overlay", error))?;
        Self::show_overlay_for_bootstrap(&window)
    }
}

impl<R: Runtime> CaptureWindowPort for TauriCaptureWindowPort<R> {
    fn window_exists(&self, label: &str) -> bool {
        self.app.get_webview_window(label).is_some()
    }

    fn create_overlay(&self, spec: &OverlayWindowSpec) -> Result<(), CaptureError> {
        if spec.visible {
            return Err(CaptureError::new(
                CaptureErrorCode::OverlayFailed,
                "capture overlay must be created hidden",
            ));
        }
        log::info!(
            "screen capture creating hidden overlay label={} route={}",
            spec.label,
            spec.route
        );
        #[cfg(target_os = "windows")]
        {
            self.create_windows_overlay(spec)?;
        }
        #[cfg(not(target_os = "windows"))]
        {
            Self::overlay_builder(&self.app, spec)
                .inner_size(800.0, 600.0)
                .visible(false)
                .build()
                .map_err(|error| overlay_error("create hidden capture overlay", error))?;
        }
        log::info!("screen capture hidden overlay created label={}", spec.label);
        Ok(())
    }

    fn defers_overlay_position_until_presented(&self) -> bool {
        cfg!(target_os = "windows")
    }

    fn uses_ephemeral_overlay(&self) -> bool {
        cfg!(target_os = "windows")
    }

    fn conceal_overlay_for_capture(&self, label: &str) -> Result<(), CaptureError> {
        #[cfg(target_os = "windows")]
        {
            let window = self.webview(label)?;
            Self::show_overlay_for_bootstrap(&window)?;
            log::info!("screen capture waking mapped Windows overlay before acquisition");
            return Ok(());
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = label;
            Ok(())
        }
    }

    fn wake_overlay_for_frame_delivery(
        &self,
        label: &str,
        monitor: &MonitorGeometry,
    ) -> Result<(), CaptureError> {
        #[cfg(target_os = "windows")]
        {
            let _window = self.webview(label)?;
            log::info!(
                "screen capture waking Windows overlay for frame delivery at ({},{} {}x{})",
                monitor.physical_origin.x,
                monitor.physical_origin.y,
                monitor.physical_size.width,
                monitor.physical_size.height
            );
            let _ = monitor;
            return Ok(());
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (label, monitor);
            Ok(())
        }
    }

    fn capture_window_state(
        &self,
        label: &str,
    ) -> Result<Option<CaptureWindowState>, CaptureError> {
        let Some(window) = self.app.get_webview_window(label) else {
            return Ok(None);
        };
        Ok(Some(CaptureWindowState {
            visible: window
                .is_visible()
                .map_err(|error| overlay_error("read capture origin visibility", error))?,
            minimized: window
                .is_minimized()
                .map_err(|error| overlay_error("read capture origin minimized state", error))?,
            focused: window
                .is_focused()
                .map_err(|error| overlay_error("read capture origin focus state", error))?,
        }))
    }

    fn hide_window(&self, label: &str) -> Result<(), CaptureError> {
        let Some(window) = self.app.get_webview_window(label) else {
            // Cleanup is idempotent when a window is already gone.
            return Ok(());
        };
        log::info!("screen capture hiding origin label={label}");
        window
            .hide()
            .map_err(|error| overlay_error("hide capture window", error))
    }

    fn conceal_overlay(&self, label: &str) -> Result<OverlayConcealment, CaptureError> {
        let Some(_window) = self.app.get_webview_window(label) else {
            return Ok(OverlayConcealment::Hidden);
        };
        #[cfg(target_os = "windows")]
        {
            let app = self.app.clone();
            let label = label.to_string();
            tauri::async_runtime::spawn(async move {
                tokio::task::yield_now().await;
                if let Some(window) = app.get_webview_window(&label)
                    && let Err(error) = window.destroy()
                {
                    log::warn!("destroy retired capture overlay failed: {error}");
                }
            });
            return Ok(OverlayConcealment::RetirementScheduled);
        }
        #[cfg(not(target_os = "windows"))]
        {
            match _window.hide() {
                Ok(()) => Ok(OverlayConcealment::Hidden),
                Err(error) => {
                    let hide_error = overlay_error("hide capture window", error);
                    let app = self.app.clone();
                    let label = label.to_string();
                    tauri::async_runtime::spawn(async move {
                        tokio::task::yield_now().await;
                        if let Some(window) = app.get_webview_window(&label)
                            && let Err(error) = window.destroy()
                        {
                            log::warn!("destroy visible capture overlay failed: {error}");
                        }
                    });
                    Ok(OverlayConcealment::DestructionDeferred(hide_error))
                }
            }
        }
    }

    fn restore_window(&self, label: &str, state: CaptureWindowState) -> Result<(), CaptureError> {
        let Some(window) = self.app.get_webview_window(label) else {
            // A user may close the origin while capture is active. There is nothing
            // left to restore, so this is a successful terminal cleanup.
            return Ok(());
        };
        if state.visible {
            window
                .show()
                .map_err(|error| overlay_error("restore capture origin visibility", error))?;
        }
        if state.minimized {
            window
                .minimize()
                .map_err(|error| overlay_error("restore capture origin minimized state", error))?;
        } else {
            window.unminimize().map_err(|error| {
                overlay_error("restore capture origin unminimized state", error)
            })?;
        }
        if !state.visible {
            window
                .hide()
                .map_err(|error| overlay_error("restore capture origin hidden state", error))?;
        }
        if state.visible && state.focused {
            window
                .set_focus()
                .map_err(|error| overlay_error("restore capture origin focus state", error))?;
        }
        Ok(())
    }

    fn defer_window_action_retry(&self, delay: Duration) {
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(delay).await;
            let runtime = app.state::<ScreenCaptureRuntime>();
            let windows = TauriCaptureWindowPort::new(app.clone());
            if let Err(error) = runtime.retry_pending_window_actions(&windows) {
                log::warn!("screen capture window cleanup retry failed: {error}");
            }
        });
    }

    fn focus_window(&self, label: &str) -> Result<(), CaptureError> {
        self.webview(label)?
            .set_focus()
            .map_err(|error| overlay_error("focus active capture", error))
    }

    fn position_overlay(&self, label: &str, monitor: &MonitorGeometry) -> Result<(), CaptureError> {
        let window = self.webview(label)?;
        log::info!(
            "screen capture positioning overlay physical=({},{} {}x{}) logical=({},{} {}x{}) scale={}",
            monitor.physical_origin.x,
            monitor.physical_origin.y,
            monitor.physical_size.width,
            monitor.physical_size.height,
            monitor.logical_origin.x,
            monitor.logical_origin.y,
            monitor.logical_size.width,
            monitor.logical_size.height,
            monitor.scale_factor
        );
        #[cfg(target_os = "linux")]
        if super::shortcut::current_linux_shortcut_backend()
            == super::shortcut::LinuxShortcutBackend::WaylandPortalRequired
        {
            return position_wayland_overlay(&window, monitor);
        }
        #[cfg(target_os = "windows")]
        {
            // Tauri monitor snapshots and xcap both report Windows desktop
            // bounds in physical pixels. Moving a one-pixel bootstrap window
            // with logical coordinates would use its old monitor's DPI and can
            // misplace or mis-size the overlay on mixed-DPI/negative displays.
            window
                .set_focusable(false)
                .map_err(|error| overlay_error("disable capture overlay focus", error))?;
            window
                .set_position(PhysicalPosition::new(
                    monitor.physical_origin.x,
                    monitor.physical_origin.y,
                ))
                .map_err(|error| overlay_error("position capture overlay", error))?;
            window
                .set_size(PhysicalSize::new(
                    monitor.physical_size.width,
                    monitor.physical_size.height,
                ))
                .map_err(|error| overlay_error("size capture overlay", error))?;
            return Ok(());
        }
        #[cfg(not(target_os = "windows"))]
        {
            window
                .set_position(LogicalPosition::new(
                    monitor.logical_origin.x,
                    monitor.logical_origin.y,
                ))
                .map_err(|error| overlay_error("position capture overlay", error))?;
            window
                .set_size(LogicalSize::new(
                    monitor.logical_size.width,
                    monitor.logical_size.height,
                ))
                .map_err(|error| overlay_error("size capture overlay", error))
        }
    }

    fn show_overlay(&self, label: &str) -> Result<(), CaptureError> {
        let window = self.webview(label)?;
        log::info!("screen capture showing overlay label={label}");
        window
            .set_ignore_cursor_events(false)
            .map_err(|error| overlay_error("make capture overlay interactive", error))?;
        window
            .set_focusable(true)
            .map_err(|error| overlay_error("enable capture overlay focus", error))?;
        window
            .show()
            .map_err(|error| overlay_error("show capture overlay", error))?;
        window
            .unminimize()
            .map_err(|error| overlay_error("unminimize capture overlay", error))?;
        #[cfg(target_os = "windows")]
        {
            let webview: &tauri::Webview<R> = window.as_ref();
            webview
                .show()
                .map_err(|error| overlay_error("show capture overlay webview", error))?;
        }
        window
            .set_focus()
            .map_err(|error| overlay_error("focus capture overlay", error))
    }

    fn emit_frame_available(
        &self,
        label: &str,
        payload: &FrameAvailablePayload,
    ) -> Result<(), CaptureError> {
        self.app
            .emit_to(label, FRAME_AVAILABLE_EVENT, payload.clone())
            .map_err(|error| overlay_error("publish capture frame metadata", error))
    }

    fn emit_result_available(
        &self,
        label: &str,
        payload: &ResultAvailablePayload,
    ) -> Result<(), CaptureError> {
        if !self.window_exists(label) {
            return Err(CaptureError::new(
                CaptureErrorCode::TargetUnavailable,
                "capture delivery target disappeared before result publication",
            ));
        }
        self.app
            .emit_to(label, RESULT_AVAILABLE_EVENT, payload.clone())
            .map_err(|_| {
                CaptureError::new(
                    CaptureErrorCode::TargetUnavailable,
                    "could not publish capture result metadata to the frozen target",
                )
            })
    }

    fn emit_delivery_failed(
        &self,
        label: &str,
        payload: &DeliveryFailedPayload,
    ) -> Result<(), CaptureError> {
        if !self.window_exists(label) {
            return Err(CaptureError::new(
                CaptureErrorCode::OverlayFailed,
                "capture overlay is unavailable for delivery status publication",
            ));
        }
        self.app
            .emit_to(label, DELIVERY_FAILED_EVENT, payload.clone())
            .map_err(|error| overlay_error("publish capture delivery failure", error))
    }

    fn emit_session_ended(
        &self,
        label: &str,
        payload: &SessionEndedPayload,
    ) -> Result<(), CaptureError> {
        if !self.window_exists(label) {
            return Err(CaptureError::new(
                CaptureErrorCode::TargetUnavailable,
                "capture session target is unavailable",
            ));
        }
        self.app
            .emit_to(label, SESSION_ENDED_EVENT, payload.clone())
            .map_err(|_| {
                CaptureError::new(
                    CaptureErrorCode::TargetUnavailable,
                    "could not publish capture session completion metadata",
                )
            })
    }

    fn emit_session_started(
        &self,
        label: &str,
        payload: &SessionStartedPayload,
    ) -> Result<(), CaptureError> {
        if !self.window_exists(label) {
            return Err(CaptureError::new(
                CaptureErrorCode::TargetUnavailable,
                "capture target disappeared before session publication",
            ));
        }
        self.app
            .emit_to(label, SESSION_STARTED_EVENT, payload.clone())
            .map_err(|_| {
                CaptureError::new(
                    CaptureErrorCode::TargetUnavailable,
                    "could not publish capture session metadata to the frozen target",
                )
            })
    }

    fn emit_target_unavailable(
        &self,
        label: &str,
        payload: &TargetUnavailablePayload,
    ) -> Result<(), CaptureError> {
        if !self.window_exists(label) {
            return Err(CaptureError::new(
                CaptureErrorCode::OverlayFailed,
                "capture overlay is unavailable for target status publication",
            ));
        }
        self.app
            .emit_to(label, TARGET_UNAVAILABLE_EVENT, payload.clone())
            .map_err(|error| overlay_error("publish capture target status", error))
    }

    fn emit_overlay_session_ended(
        &self,
        label: &str,
        payload: &OverlaySessionEndedPayload,
    ) -> Result<(), CaptureError> {
        if !self.window_exists(label) {
            return Err(CaptureError::new(
                CaptureErrorCode::OverlayFailed,
                "capture overlay is unavailable for terminal cleanup publication",
            ));
        }
        self.app
            .emit_to(label, OVERLAY_SESSION_ENDED_EVENT, payload.clone())
            .map_err(|error| overlay_error("publish overlay capture completion metadata", error))
    }
}

fn overlay_error(stage: &str, error: impl std::fmt::Display) -> CaptureError {
    CaptureError::new(CaptureErrorCode::OverlayFailed, format!("{stage}: {error}"))
}

#[cfg(target_os = "linux")]
fn wayland_monitor_index(monitor_id: &str) -> Result<i32, CaptureError> {
    let index = monitor_id
        .strip_prefix("wayland-winit:")
        .and_then(|rest| rest.split(':').next())
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|index| *index >= 0)
        .ok_or_else(|| {
            CaptureError::new(
                CaptureErrorCode::InvalidMonitor,
                "the Wayland capture monitor identifier is invalid",
            )
        })?;
    Ok(index)
}

#[cfg(target_os = "linux")]
fn position_wayland_overlay<R: Runtime>(
    window: &tauri::WebviewWindow<R>,
    monitor: &MonitorGeometry,
) -> Result<(), CaptureError> {
    use gtk::prelude::{GtkWindowExt, MonitorExt, WidgetExt};

    let monitor_index = wayland_monitor_index(&monitor.id)?;
    let gtk_window = window
        .gtk_window()
        .map_err(|error| overlay_error("access Wayland capture overlay", error))?;
    let display = gtk_window.display();
    let gdk_monitor = display.monitor(monitor_index).ok_or_else(|| {
        CaptureError::new(
            CaptureErrorCode::InvalidMonitor,
            "the selected Wayland monitor is no longer available",
        )
    })?;
    let geometry = gdk_monitor.geometry();
    let scale = gdk_monitor.scale_factor();
    let actual_origin = (
        geometry.x().saturating_mul(scale),
        geometry.y().saturating_mul(scale),
    );
    let actual_size = (
        u32::try_from(geometry.width().saturating_mul(scale)).unwrap_or(0),
        u32::try_from(geometry.height().saturating_mul(scale)).unwrap_or(0),
    );
    if actual_origin != (monitor.physical_origin.x, monitor.physical_origin.y)
        || actual_size != (monitor.physical_size.width, monitor.physical_size.height)
    {
        return Err(CaptureError::new(
            CaptureErrorCode::InvalidMonitor,
            "the selected Wayland monitor changed before overlay placement",
        ));
    }
    gtk_window.fullscreen_on_monitor(&display.default_screen(), monitor_index);
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn wayland_monitor_index_accepts_only_owned_snapshot_identifiers() {
        assert_eq!(
            wayland_monitor_index("wayland-winit:2:Display:0:0:1920:1080").unwrap(),
            2
        );
        for invalid in [
            "xcap:2",
            "wayland-winit:-1:Display:0:0:1920:1080",
            "wayland-winit:not-a-number:Display:0:0:1920:1080",
        ] {
            assert_eq!(
                wayland_monitor_index(invalid).unwrap_err().code,
                CaptureErrorCode::InvalidMonitor
            );
        }
    }
}
