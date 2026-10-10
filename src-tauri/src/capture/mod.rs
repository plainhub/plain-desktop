//! plain-capture: self-owned screen capture over native OS APIs.
//!
//! This module is deliberately free of Tauri types. The app layer
//! (`commands::screen_capture`) adapts it, injects Wayland display data that
//! only the compositor-facing windowing stack can provide, and owns the
//! session/overlay runtime.

use serde::{Deserialize, Serialize};

#[cfg(target_os = "linux")]
pub(crate) mod linux;
#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(target_os = "windows")]
pub(crate) mod windows;

pub const MAX_RAW_FRAME_BYTES: usize = 256 * 1024 * 1024;

use std::sync::atomic::{AtomicBool, Ordering};

static NATIVE_ACQUISITION_GATE: CaptureAcquisitionGate = CaptureAcquisitionGate::new();

struct CaptureAcquisitionGate {
    active: AtomicBool,
}

impl CaptureAcquisitionGate {
    const fn new() -> Self {
        Self {
            active: AtomicBool::new(false),
        }
    }

    fn try_acquire(&self) -> Result<CaptureAcquisitionLease<'_>, CaptureError> {
        self.active
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .map_err(|_| {
                CaptureError::new(
                    CaptureErrorCode::Busy,
                    "another native screen acquisition is still running",
                )
            })?;
        Ok(CaptureAcquisitionLease { gate: self })
    }
}

struct CaptureAcquisitionLease<'a> {
    gate: &'a CaptureAcquisitionGate,
}

impl Drop for CaptureAcquisitionLease<'_> {
    fn drop(&mut self) {
        self.gate.active.store(false, Ordering::Release);
    }
}

/// Serialize every blocking native acquisition, including portal PipeWire
/// consumers that outlive a cancelled async task. This prevents a stale
/// backend thread and its replacement from retaining full-screen buffers at
/// the same time. A timed-out `spawn_blocking` task cannot be cancelled, so
/// its lease remains held by the blocking closure until that backend call
/// really exits.
pub fn with_native_acquisition_lease<T>(
    operation: impl FnOnce() -> Result<T, CaptureError>,
) -> Result<T, CaptureError> {
    let _lease = NATIVE_ACQUISITION_GATE.try_acquire()?;
    operation()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub struct PhysicalRect {
    pub origin: PhysicalPoint,
    pub size: PhysicalSize,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CssPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CssSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CssRect {
    pub origin: CssPoint,
    pub size: CssSize,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogicalPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogicalSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    pub id: String,
    pub physical_origin: PhysicalPoint,
    pub physical_size: PhysicalSize,
    pub logical_origin: LogicalPoint,
    pub logical_size: LogicalSize,
    pub scale_factor: f64,
}

impl DisplayInfo {
    pub fn validate(&self) -> Result<(), CaptureError> {
        let logical_values = [
            self.logical_origin.x,
            self.logical_origin.y,
            self.logical_size.width,
            self.logical_size.height,
            self.scale_factor,
        ];
        if self.id.trim().is_empty()
            || self.physical_size.width == 0
            || self.physical_size.height == 0
            || logical_values.iter().any(|value| !value.is_finite())
            || self.logical_size.width <= 0.0
            || self.logical_size.height <= 0.0
            || self.scale_factor <= 0.0
        {
            return Err(CaptureError::new(
                CaptureErrorCode::InvalidMonitor,
                "monitor geometry is incomplete or invalid",
            ));
        }
        Ok(())
    }

    fn contains(&self, point: PhysicalPoint) -> bool {
        let left = i64::from(self.physical_origin.x);
        let top = i64::from(self.physical_origin.y);
        let right = left + i64::from(self.physical_size.width);
        let bottom = top + i64::from(self.physical_size.height);
        let x = i64::from(point.x);
        let y = i64::from(point.y);
        x >= left && x < right && y >= top && y < bottom
    }
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn select_display_at(displays: &[DisplayInfo], point: PhysicalPoint) -> Option<&DisplayInfo> {
    displays.iter().find(|display| display.contains(point))
}

/// The pixels delivered by every backend: 8-bit RGBA, top-left origin,
/// `stride >= width * 4`.
#[derive(Debug)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub bytes: Vec<u8>,
}

pub trait Capture: Send + Sync {
    fn displays(&self) -> Result<Vec<DisplayInfo>, CaptureError>;

    fn display_index_at_cursor(&self, displays: &[DisplayInfo]) -> Result<usize, CaptureError>;

    /// `exclude_window_ids` carries the platform-native window ids (macOS
    /// CGWindowID) of our own overlay windows so the capture can cut them out
    /// of the frame instead of photographing the frozen-screen backdrop.
    fn capture_display(
        &self,
        display: &DisplayInfo,
        exclude_window_ids: &[u64],
    ) -> Result<Frame, CaptureError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureErrorCode {
    Busy,
    PermissionDenied,
    NoMonitor,
    MonitorSelectionUnavailable,
    InvalidMonitor,
    CaptureFailed,
    InvalidFrame,
    FrameTooLarge,
    OverlayFailed,
    InvalidSession,
    InvalidPhase,
    UnauthorizedCaller,
    TargetUnavailable,
    EncodeFailed,
    ClipboardFailed,
    SaveFailed,
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureError {
    pub code: CaptureErrorCode,
    pub detail: String,
}

impl CaptureError {
    pub fn new(code: CaptureErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.detail)
    }
}

impl std::error::Error for CaptureError {}

pub fn checked_rounded_i32(value: f64) -> Result<i32, CaptureError> {
    let rounded = value.round();
    if !rounded.is_finite() || rounded < f64::from(i32::MIN) || rounded > f64::from(i32::MAX) {
        return Err(CaptureError::new(
            CaptureErrorCode::InvalidMonitor,
            "monitor origin exceeds the physical coordinate range",
        ));
    }
    Ok(rounded as i32)
}

pub fn checked_rounded_u32(value: f64) -> Result<u32, CaptureError> {
    let rounded = value.round();
    if !rounded.is_finite() || rounded <= 0.0 || rounded > f64::from(u32::MAX) {
        return Err(CaptureError::new(
            CaptureErrorCode::InvalidMonitor,
            "monitor size exceeds the physical coordinate range",
        ));
    }
    Ok(rounded as u32)
}

pub fn permission_preflight_result(granted: bool) -> Result<(), CaptureError> {
    if granted {
        Ok(())
    } else {
        Err(CaptureError::new(
            CaptureErrorCode::PermissionDenied,
            "screen capture permission is required",
        ))
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn wayland_cursor_is_unavailable(
    session_type: Option<&str>,
    wayland_display_present: bool,
) -> bool {
    session_type.is_some_and(|value| value.eq_ignore_ascii_case("wayland"))
        || wayland_display_present
}

/// Convert a native top-left physical monitor work area into overlay-local CSS
/// coordinates.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn top_left_physical_work_area_to_local_css(
    frame_origin: PhysicalPoint,
    frame_size: PhysicalSize,
    work_origin: PhysicalPoint,
    work_size: PhysicalSize,
    scale_factor: f64,
) -> Result<CssRect, CaptureError> {
    if !scale_factor.is_finite()
        || scale_factor <= 0.0
        || frame_size.width == 0
        || frame_size.height == 0
        || work_size.width == 0
        || work_size.height == 0
    {
        return Err(CaptureError::new(
            CaptureErrorCode::InvalidMonitor,
            "the native monitor work area is invalid",
        ));
    }

    let left = i64::from(work_origin.x) - i64::from(frame_origin.x);
    let top = i64::from(work_origin.y) - i64::from(frame_origin.y);
    let right = left + i64::from(work_size.width);
    let bottom = top + i64::from(work_size.height);
    if left < 0
        || top < 0
        || right > i64::from(frame_size.width)
        || bottom > i64::from(frame_size.height)
    {
        return Err(CaptureError::new(
            CaptureErrorCode::InvalidMonitor,
            "the native monitor work area falls outside its display frame",
        ));
    }

    Ok(CssRect {
        origin: CssPoint {
            x: left as f64 / scale_factor,
            y: top as f64 / scale_factor,
        },
        size: CssSize {
            width: f64::from(work_size.width) / scale_factor,
            height: f64::from(work_size.height) / scale_factor,
        },
    })
}

/// Convert AppKit's bottom-left screen coordinates into overlay-local,
/// top-left CSS coordinates. `NSScreen::visibleFrame` is authoritative for the
/// menu bar and Dock; WebKit's `screen.avail*` values are not on every macOS
/// release/webview combination.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn bottom_left_visible_frame_to_top_left_area(
    frame_origin: LogicalPoint,
    frame_size: LogicalSize,
    visible_origin: LogicalPoint,
    visible_size: LogicalSize,
) -> Result<CssRect, CaptureError> {
    let values = [
        frame_origin.x,
        frame_origin.y,
        frame_size.width,
        frame_size.height,
        visible_origin.x,
        visible_origin.y,
        visible_size.width,
        visible_size.height,
    ];
    if values.iter().any(|value| !value.is_finite())
        || frame_size.width <= 0.0
        || frame_size.height <= 0.0
        || visible_size.width <= 0.0
        || visible_size.height <= 0.0
    {
        return Err(CaptureError::new(
            CaptureErrorCode::InvalidMonitor,
            "AppKit returned invalid screen work-area geometry",
        ));
    }

    let left = visible_origin.x - frame_origin.x;
    let bottom = visible_origin.y - frame_origin.y;
    let right = left + visible_size.width;
    let top_from_bottom = bottom + visible_size.height;
    const TOLERANCE: f64 = 0.01;
    if left < -TOLERANCE
        || bottom < -TOLERANCE
        || right > frame_size.width + TOLERANCE
        || top_from_bottom > frame_size.height + TOLERANCE
    {
        return Err(CaptureError::new(
            CaptureErrorCode::InvalidMonitor,
            "AppKit screen work area falls outside its display frame",
        ));
    }

    let x = left.clamp(0.0, frame_size.width);
    let y = (frame_size.height - top_from_bottom).clamp(0.0, frame_size.height);
    let clamped_right = right.clamp(0.0, frame_size.width);
    let clamped_bottom = (frame_size.height - bottom).clamp(0.0, frame_size.height);
    let width = clamped_right - x;
    let height = clamped_bottom - y;
    if width <= 0.0 || height <= 0.0 {
        return Err(CaptureError::new(
            CaptureErrorCode::InvalidMonitor,
            "AppKit returned an empty screen work area",
        ));
    }
    Ok(CssRect {
        origin: CssPoint { x, y },
        size: CssSize { width, height },
    })
}

pub fn platform_capture() -> Result<Box<dyn Capture>, CaptureError> {
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(macos::MacosCapture::new()))
    }
    #[cfg(target_os = "windows")]
    {
        windows::WindowsCapture::connect().map(|backend| Box::new(backend) as Box<dyn Capture>)
    }
    #[cfg(target_os = "linux")]
    {
        linux::platform_capture()
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn preflight_capture_permission() -> Result<(), CaptureError> {
    macos::preflight_capture_permission()
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn preflight_capture_permission() -> Result<(), CaptureError> {
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn request_capture_permission() -> bool {
    macos::request_capture_permission()
}

#[cfg(test)]
pub fn select_logical_monitor(displays: &[DisplayInfo], x: f64, y: f64) -> Option<usize> {
    displays.iter().position(|display| {
        let right = display.logical_origin.x + display.logical_size.width;
        let bottom = display.logical_origin.y + display.logical_size.height;
        x >= display.logical_origin.x && x < right && y >= display.logical_origin.y && y < bottom
    })
}

#[cfg(test)]
mod tests {
    use super::CaptureError;
    use super::CaptureErrorCode;
    use super::DisplayInfo;
    use super::LogicalPoint;
    use super::LogicalSize;
    use super::NATIVE_ACQUISITION_GATE;
    use super::PhysicalPoint;
    use super::PhysicalRect;
    use super::PhysicalSize;
    use super::bottom_left_visible_frame_to_top_left_area;
    use super::checked_rounded_i32;
    use super::checked_rounded_u32;
    use super::permission_preflight_result;
    use super::select_logical_monitor;
    use super::top_left_physical_work_area_to_local_css;
    use super::wayland_cursor_is_unavailable;

    #[test]
    fn acquisition_gate_rejects_overlap_and_releases_only_when_the_lease_drops() {
        let first = NATIVE_ACQUISITION_GATE
            .try_acquire()
            .expect("first acquisition");
        let overlap = match NATIVE_ACQUISITION_GATE.try_acquire() {
            Ok(_) => panic!("overlap must fail"),
            Err(error) => error,
        };
        assert_eq!(overlap.code, CaptureErrorCode::Busy);
        drop(first);
        NATIVE_ACQUISITION_GATE
            .try_acquire()
            .expect("gate released after the lease dropped");
    }

    fn display(
        id: &str,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        scale_factor: f64,
    ) -> DisplayInfo {
        DisplayInfo {
            id: id.to_string(),
            physical_origin: PhysicalPoint { x, y },
            physical_size: PhysicalSize { width, height },
            logical_origin: LogicalPoint {
                x: f64::from(x) / scale_factor,
                y: f64::from(y) / scale_factor,
            },
            logical_size: LogicalSize {
                width: f64::from(width) / scale_factor,
                height: f64::from(height) / scale_factor,
            },
            scale_factor,
        }
    }

    #[test]
    fn mixed_dpi_selection_picks_the_display_under_the_logical_point() {
        let displays = vec![
            display("a", 0, 0, 3024, 1964, 2.0),
            display("b", 1512, 0, 1920, 1080, 1.0),
        ];
        assert_eq!(select_logical_monitor(&displays, 1600.0, 10.0), Some(1));
        assert_eq!(select_logical_monitor(&displays, 100.0, 10.0), Some(0));
        assert_eq!(select_logical_monitor(&displays, 5000.0, 10.0), None);
        assert_eq!(displays[0].physical_size.width, 3024);
    }

    #[test]
    fn display_validation_rejects_incomplete_geometry() {
        for invalid in [
            display("", 0, 0, 100, 100, 1.0),
            display("id", 0, 0, 0, 100, 1.0),
            display("id", 0, 0, 100, 100, f64::NAN),
        ] {
            assert_eq!(
                invalid.validate().expect_err("invalid display").code,
                CaptureErrorCode::InvalidMonitor
            );
        }
        display("id", -1920, 0, 1920, 1080, 1.5)
            .validate()
            .expect("valid display");
    }

    #[test]
    fn coordinate_rounding_rejects_out_of_range_values() {
        assert_eq!(checked_rounded_i32(-1920.4).expect("rounded"), -1920);
        assert_eq!(
            checked_rounded_i32(f64::from(i32::MAX) + 1.0)
                .expect_err("overflow")
                .code,
            CaptureErrorCode::InvalidMonitor
        );
        assert!(checked_rounded_u32(0.0).is_err());
        assert_eq!(checked_rounded_u32(1079.6).expect("rounded"), 1080);
    }

    #[test]
    fn preflight_maps_denied_state_to_the_permission_error() {
        permission_preflight_result(true).expect("granted");
        assert_eq!(
            permission_preflight_result(false).expect_err("denied").code,
            CaptureErrorCode::PermissionDenied
        );
    }

    #[test]
    fn wayland_detection_matches_session_type_or_display_variable() {
        assert!(wayland_cursor_is_unavailable(Some("Wayland"), false));
        assert!(wayland_cursor_is_unavailable(Some("x11"), true));
        assert!(!wayland_cursor_is_unavailable(Some("x11"), false));
        assert!(!wayland_cursor_is_unavailable(None, false));
    }

    #[test]
    fn windows_work_area_maps_into_overlay_local_css_coordinates() {
        let rect = top_left_physical_work_area_to_local_css(
            PhysicalPoint { x: -1920, y: 0 },
            PhysicalSize {
                width: 1920,
                height: 1080,
            },
            PhysicalPoint { x: -1920, y: 48 },
            PhysicalSize {
                width: 1920,
                height: 1032,
            },
            1.5,
        )
        .expect("work area");
        assert_eq!(rect.origin.x, 0.0);
        assert_eq!(rect.origin.y, 32.0);
        assert_eq!(rect.size.width, 1280.0);

        assert_eq!(
            top_left_physical_work_area_to_local_css(
                PhysicalPoint { x: 0, y: 0 },
                PhysicalSize {
                    width: 100,
                    height: 100
                },
                PhysicalPoint { x: 0, y: 0 },
                PhysicalSize {
                    width: 200,
                    height: 100
                },
                1.0,
            )
            .expect_err("out of bounds")
            .code,
            CaptureErrorCode::InvalidMonitor
        );
    }

    #[test]
    fn appkit_visible_frame_maps_to_top_left_css_area() {
        let rect = bottom_left_visible_frame_to_top_left_area(
            LogicalPoint { x: 0.0, y: 0.0 },
            LogicalSize {
                width: 1512.0,
                height: 982.0,
            },
            LogicalPoint { x: 0.0, y: 25.0 },
            LogicalSize {
                width: 1512.0,
                height: 955.0,
            },
        )
        .expect("visible frame");
        assert_eq!(rect.origin.y, 2.0);
        assert_eq!(rect.size.height, 955.0);

        assert_eq!(
            bottom_left_visible_frame_to_top_left_area(
                LogicalPoint { x: 0.0, y: 0.0 },
                LogicalSize {
                    width: 100.0,
                    height: 100.0
                },
                LogicalPoint { x: 0.0, y: 50.0 },
                LogicalSize {
                    width: 200.0,
                    height: 100.0
                },
            )
            .expect_err("exceeds frame")
            .code,
            CaptureErrorCode::InvalidMonitor
        );
    }

    #[test]
    fn physical_rect_round_trips_through_serde() {
        let rect = PhysicalRect {
            origin: PhysicalPoint { x: -1, y: 2 },
            size: PhysicalSize {
                width: 3,
                height: 4,
            },
        };
        let json = serde_json::to_string(&rect).expect("serialize");
        assert_eq!(
            json,
            r#"{"origin":{"x":-1,"y":2},"size":{"width":3,"height":4}}"#
        );
        assert_eq!(
            serde_json::from_str::<PhysicalRect>(&json).expect("deserialize"),
            rect
        );
    }

    #[test]
    fn capture_error_is_a_displayable_std_error() {
        let error = CaptureError::new(CaptureErrorCode::Busy, "busy now");
        assert_eq!(error.to_string(), "busy now");
        let _: &dyn std::error::Error = &error;
    }
}
