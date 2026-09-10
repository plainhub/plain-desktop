use tauri::{AppHandle, Runtime};
use xcap::Monitor as XcapMonitor;

use super::backend::{NativeFrame, ScreenCaptureBackend};
use super::contract::{
    CaptureError, CaptureErrorCode, CssPoint, CssRect, CssSize, LogicalPoint, LogicalSize,
    MonitorGeometry, PhysicalPoint, PhysicalRect, PhysicalSize,
};

#[cfg_attr(target_os = "macos", allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometryConvention {
    Physical,
    Logical,
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn xcap_bounds_to_physical(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale_factor: f64,
    convention: GeometryConvention,
) -> Result<PhysicalRect, CaptureError> {
    if !scale_factor.is_finite() || scale_factor <= 0.0 || width == 0 || height == 0 {
        return Err(CaptureError::new(
            CaptureErrorCode::InvalidMonitor,
            "xcap returned invalid monitor geometry",
        ));
    }

    let multiplier = match convention {
        GeometryConvention::Physical => 1.0,
        GeometryConvention::Logical => scale_factor,
    };
    let physical_x = checked_rounded_i32(f64::from(x) * multiplier)?;
    let physical_y = checked_rounded_i32(f64::from(y) * multiplier)?;
    let physical_width = checked_rounded_u32(f64::from(width) * multiplier)?;
    let physical_height = checked_rounded_u32(f64::from(height) * multiplier)?;

    Ok(PhysicalRect {
        origin: PhysicalPoint {
            x: physical_x,
            y: physical_y,
        },
        size: PhysicalSize {
            width: physical_width,
            height: physical_height,
        },
    })
}

#[cfg(test)]
pub fn select_logical_monitor(monitors: &[MonitorGeometry], x: f64, y: f64) -> Option<usize> {
    monitors.iter().position(|monitor| {
        let right = monitor.logical_origin.x + monitor.logical_size.width;
        let bottom = monitor.logical_origin.y + monitor.logical_size.height;
        x >= monitor.logical_origin.x && x < right && y >= monitor.logical_origin.y && y < bottom
    })
}

/// Convert a native top-left physical monitor work area into overlay-local CSS
/// coordinates. Tauri sources this rectangle from Win32 `GetMonitorInfoW` on
/// Windows and GDK's monitor work area on Linux.
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

const MACOS_NATIVE_DISPLAY_PREFIX: &str = "macos-cg-display:";

fn macos_monitor_id(display_id: u32) -> String {
    format!("{MACOS_NATIVE_DISPLAY_PREFIX}{display_id}")
}

pub(super) fn parse_macos_monitor_id(value: &str) -> Option<u32> {
    let raw = value.strip_prefix(MACOS_NATIVE_DISPLAY_PREFIX)?;
    let display_id = raw.parse::<u32>().ok()?;
    (display_id != 0 && macos_monitor_id(display_id) == value).then_some(display_id)
}

fn macos_monitor_geometry(
    display_id: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    scale_factor: f64,
) -> Result<MonitorGeometry, CaptureError> {
    if display_id == 0
        || !x.is_finite()
        || !y.is_finite()
        || !width.is_finite()
        || !height.is_finite()
        || !scale_factor.is_finite()
        || width <= 0.0
        || height <= 0.0
        || scale_factor <= 0.0
    {
        return Err(CaptureError::new(
            CaptureErrorCode::InvalidMonitor,
            "CoreGraphics returned invalid monitor geometry",
        ));
    }
    let geometry = MonitorGeometry {
        id: macos_monitor_id(display_id),
        physical_origin: PhysicalPoint {
            x: checked_rounded_i32(x * scale_factor)?,
            y: checked_rounded_i32(y * scale_factor)?,
        },
        physical_size: PhysicalSize {
            width: checked_rounded_u32(width * scale_factor)?,
            height: checked_rounded_u32(height * scale_factor)?,
        },
        logical_origin: LogicalPoint { x, y },
        logical_size: LogicalSize { width, height },
        scale_factor,
    };
    geometry.validate()?;
    Ok(geometry)
}

fn select_monitor_by_native_id(
    monitors: &[MonitorGeometry],
    display_id: u32,
) -> Result<usize, CaptureError> {
    let mut matches = monitors
        .iter()
        .enumerate()
        .filter(|(_, monitor)| parse_macos_monitor_id(&monitor.id) == Some(display_id))
        .map(|(index, _)| index);
    let selected = matches.next().ok_or_else(|| {
        CaptureError::new(
            CaptureErrorCode::NoMonitor,
            "the native display disappeared before selection",
        )
    })?;
    if matches.next().is_some() {
        return Err(CaptureError::new(
            CaptureErrorCode::InvalidMonitor,
            "multiple monitors claim the same native display id",
        ));
    }
    Ok(selected)
}

fn find_matching_native_candidate<T>(
    candidates: impl IntoIterator<Item = Result<(u32, T), CaptureError>>,
    selected_display_id: u32,
) -> Result<T, CaptureError> {
    let mut matched = None;
    let mut last_error = None;
    for candidate in candidates {
        match candidate {
            Ok((display_id, value)) if display_id == selected_display_id => {
                if matched.is_some() {
                    return Err(CaptureError::new(
                        CaptureErrorCode::InvalidMonitor,
                        "xcap returned duplicate native display ids",
                    ));
                }
                matched = Some(value);
            }
            Ok(_) => {}
            Err(error) => last_error = Some(error),
        }
    }
    matched.ok_or_else(|| {
        last_error.unwrap_or_else(|| {
            CaptureError::new(
                CaptureErrorCode::NoMonitor,
                "the selected native display disappeared before capture",
            )
        })
    })
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn wayland_cursor_is_unavailable(
    session_type: Option<&str>,
    wayland_display_present: bool,
) -> bool {
    session_type.is_some_and(|value| value.eq_ignore_ascii_case("wayland"))
        || wayland_display_present
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

fn checked_rounded_i32(value: f64) -> Result<i32, CaptureError> {
    let rounded = value.round();
    if !rounded.is_finite() || rounded < f64::from(i32::MIN) || rounded > f64::from(i32::MAX) {
        return Err(CaptureError::new(
            CaptureErrorCode::InvalidMonitor,
            "monitor origin exceeds the physical coordinate range",
        ));
    }
    Ok(rounded as i32)
}

fn checked_rounded_u32(value: f64) -> Result<u32, CaptureError> {
    let rounded = value.round();
    if !rounded.is_finite() || rounded <= 0.0 || rounded > f64::from(u32::MAX) {
        return Err(CaptureError::new(
            CaptureErrorCode::InvalidMonitor,
            "monitor size exceeds the physical coordinate range",
        ));
    }
    Ok(rounded as u32)
}

#[cfg(target_os = "linux")]
const XCAP_GEOMETRY_CONVENTION: GeometryConvention = GeometryConvention::Logical;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
const XCAP_GEOMETRY_CONVENTION: GeometryConvention = GeometryConvention::Physical;

pub struct XcapBackend<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> XcapBackend<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

impl<R: Runtime> ScreenCaptureBackend for XcapBackend<R> {
    fn monitors(&self) -> Result<Vec<MonitorGeometry>, CaptureError> {
        #[cfg(target_os = "macos")]
        {
            macos_monitor_geometries()
        }
        #[cfg(not(target_os = "macos"))]
        {
            self.app
                .available_monitors()
                .map_err(|error| platform_error("enumerate monitors", error))?
                .into_iter()
                .enumerate()
                .map(|(index, monitor)| {
                    let position = monitor.position();
                    let size = monitor.size();
                    let scale_factor = monitor.scale_factor();
                    let name = monitor.name().map(String::as_str).unwrap_or("unnamed");
                    let geometry = MonitorGeometry {
                        id: format!(
                            "{index}:{name}:{}:{}:{}:{}",
                            position.x, position.y, size.width, size.height
                        ),
                        physical_origin: PhysicalPoint {
                            x: position.x,
                            y: position.y,
                        },
                        physical_size: PhysicalSize {
                            width: size.width,
                            height: size.height,
                        },
                        logical_origin: LogicalPoint {
                            x: f64::from(position.x) / scale_factor,
                            y: f64::from(position.y) / scale_factor,
                        },
                        logical_size: LogicalSize {
                            width: f64::from(size.width) / scale_factor,
                            height: f64::from(size.height) / scale_factor,
                        },
                        scale_factor,
                    };
                    geometry.validate()?;
                    Ok(geometry)
                })
                .collect()
        }
    }

    fn monitor_index_at_cursor(&self, monitors: &[MonitorGeometry]) -> Result<usize, CaptureError> {
        monitor_index_at_cursor(&self.app, monitors)
    }

    fn capture_monitor(&self, monitor: &MonitorGeometry) -> Result<NativeFrame, CaptureError> {
        preflight_capture_permission()?;
        let candidates =
            XcapMonitor::all().map_err(|error| capture_error("enumerate xcap monitors", error))?;
        let xcap_monitor = select_xcap_capture_monitor(candidates, monitor)?;
        let image = xcap_monitor
            .capture_image()
            .map_err(|error| capture_error("capture monitor", error))?;
        let width = image.width();
        let height = image.height();
        let stride = width.checked_mul(4).ok_or_else(|| {
            CaptureError::new(CaptureErrorCode::FrameTooLarge, "RGBA stride overflow")
        })?;

        Ok(NativeFrame {
            width,
            height,
            stride,
            bytes: image.into_raw(),
        })
    }
}

#[cfg(target_os = "macos")]
fn active_core_graphics_display_ids() -> Result<Vec<u32>, CaptureError> {
    use objc2_core_graphics::{CGDirectDisplayID, CGError, CGGetActiveDisplayList};

    const MAX_ACTIVE_DISPLAYS: u32 = 64;
    let mut displays = vec![0 as CGDirectDisplayID; MAX_ACTIVE_DISPLAYS as usize];
    let mut count = 0;
    let error =
        unsafe { CGGetActiveDisplayList(MAX_ACTIVE_DISPLAYS, displays.as_mut_ptr(), &mut count) };
    if error != CGError::Success {
        return Err(platform_error(
            "enumerate CoreGraphics displays",
            format!("{error:?}"),
        ));
    }
    if count == 0 || count >= MAX_ACTIVE_DISPLAYS {
        return Err(CaptureError::new(
            CaptureErrorCode::InvalidMonitor,
            "CoreGraphics returned an empty or truncated display list",
        ));
    }
    displays.truncate(count as usize);
    if displays
        .iter()
        .enumerate()
        .any(|(index, display_id)| *display_id == 0 || displays[..index].contains(display_id))
    {
        return Err(CaptureError::new(
            CaptureErrorCode::InvalidMonitor,
            "CoreGraphics returned invalid or duplicate display ids",
        ));
    }
    Ok(displays)
}

#[cfg(target_os = "macos")]
fn macos_monitor_geometries() -> Result<Vec<MonitorGeometry>, CaptureError> {
    use objc2_core_graphics::CGDisplayBounds;

    let candidates =
        XcapMonitor::all().map_err(|error| capture_error("enumerate xcap monitors", error))?;
    active_core_graphics_display_ids()?
        .into_iter()
        .map(|display_id| {
            let candidate = find_matching_native_candidate(
                candidates.iter().map(|candidate| {
                    candidate
                        .id()
                        .map(|candidate_id| (candidate_id, candidate))
                        .map_err(|error| capture_error("read xcap monitor id", error))
                }),
                display_id,
            )?;
            let scale_factor = f64::from(
                candidate
                    .scale_factor()
                    .map_err(|error| capture_error("read xcap monitor scale", error))?,
            );
            let bounds = CGDisplayBounds(display_id);
            macos_monitor_geometry(
                display_id,
                bounds.origin.x,
                bounds.origin.y,
                bounds.size.width,
                bounds.size.height,
                scale_factor,
            )
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn core_graphics_display_id_at_cursor() -> Result<u32, CaptureError> {
    use objc2_core_graphics::{
        CGDirectDisplayID, CGDisplayIsActive, CGError, CGEvent, CGGetDisplaysWithPoint,
    };

    let event = CGEvent::new(None).ok_or_else(|| {
        CaptureError::new(
            CaptureErrorCode::MonitorSelectionUnavailable,
            "CoreGraphics could not read the global pointer location",
        )
    })?;
    let point = CGEvent::location(Some(&event));
    if !point.x.is_finite() || !point.y.is_finite() {
        return Err(CaptureError::new(
            CaptureErrorCode::MonitorSelectionUnavailable,
            "CoreGraphics returned an invalid global pointer location",
        ));
    }

    const MAX_MATCHING_DISPLAYS: u32 = 16;
    let mut displays = vec![0 as CGDirectDisplayID; MAX_MATCHING_DISPLAYS as usize];
    let mut count = 0;
    let error = unsafe {
        CGGetDisplaysWithPoint(
            point,
            MAX_MATCHING_DISPLAYS,
            displays.as_mut_ptr(),
            &mut count,
        )
    };
    if error != CGError::Success {
        return Err(platform_error(
            "select CoreGraphics display at cursor",
            format!("{error:?}"),
        ));
    }
    if count == 0 {
        return Err(CaptureError::new(
            CaptureErrorCode::NoMonitor,
            "the pointer is not inside an active monitor",
        ));
    }
    if count != 1 {
        return Err(CaptureError::new(
            CaptureErrorCode::InvalidMonitor,
            "the pointer maps to multiple CoreGraphics displays",
        ));
    }
    let display_id = displays[0];
    if display_id == 0 || !CGDisplayIsActive(display_id) {
        return Err(CaptureError::new(
            CaptureErrorCode::NoMonitor,
            "the display at the pointer is no longer active",
        ));
    }
    Ok(display_id)
}

#[cfg(target_os = "macos")]
fn select_xcap_capture_monitor(
    candidates: Vec<XcapMonitor>,
    monitor: &MonitorGeometry,
) -> Result<XcapMonitor, CaptureError> {
    let display_id = parse_macos_monitor_id(&monitor.id).ok_or_else(|| {
        CaptureError::new(
            CaptureErrorCode::InvalidMonitor,
            "selected macOS monitor has no native display id",
        )
    })?;
    find_matching_native_candidate(
        candidates.into_iter().map(|candidate| {
            candidate
                .id()
                .map(|candidate_id| (candidate_id, candidate))
                .map_err(|error| capture_error("read xcap monitor id", error))
        }),
        display_id,
    )
}

#[cfg(not(target_os = "macos"))]
fn select_xcap_capture_monitor(
    candidates: Vec<XcapMonitor>,
    monitor: &MonitorGeometry,
) -> Result<XcapMonitor, CaptureError> {
    let selected = PhysicalRect {
        origin: monitor.physical_origin,
        size: monitor.physical_size,
    };
    find_matching_candidate(
        candidates
            .into_iter()
            .map(|candidate| xcap_monitor_bounds(&candidate).map(|bounds| (bounds, candidate))),
        selected,
    )
}

/// Check capture access without displaying a system prompt. Keep the request
/// API separate so Plain's guidance is always presented first.
#[cfg(target_os = "macos")]
pub(crate) fn preflight_capture_permission() -> Result<(), CaptureError> {
    use objc2_core_graphics::CGPreflightScreenCaptureAccess;

    permission_preflight_result(CGPreflightScreenCaptureAccess())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn preflight_capture_permission() -> Result<(), CaptureError> {
    Ok(())
}

/// Request capture access only from the explicit action in Plain's guidance.
#[cfg(target_os = "macos")]
pub(crate) fn request_capture_permission() -> bool {
    use objc2_core_graphics::{CGPreflightScreenCaptureAccess, CGRequestScreenCaptureAccess};

    CGPreflightScreenCaptureAccess() || CGRequestScreenCaptureAccess()
}

#[cfg(target_os = "macos")]
fn monitor_index_at_cursor<R: Runtime>(
    _app: &AppHandle<R>,
    monitors: &[MonitorGeometry],
) -> Result<usize, CaptureError> {
    select_monitor_by_native_id(monitors, core_graphics_display_id_at_cursor()?)
}

#[cfg(target_os = "linux")]
fn monitor_index_at_cursor<R: Runtime>(
    app: &AppHandle<R>,
    monitors: &[MonitorGeometry],
) -> Result<usize, CaptureError> {
    if wayland_cursor_is_unavailable(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
    ) {
        return Err(CaptureError::new(
            CaptureErrorCode::MonitorSelectionUnavailable,
            "global pointer location is unavailable on Wayland; use the portal chooser",
        ));
    }
    physical_monitor_index_at_cursor(app, monitors)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn monitor_index_at_cursor<R: Runtime>(
    app: &AppHandle<R>,
    monitors: &[MonitorGeometry],
) -> Result<usize, CaptureError> {
    physical_monitor_index_at_cursor(app, monitors)
}

#[cfg(not(target_os = "macos"))]
fn physical_monitor_index_at_cursor<R: Runtime>(
    app: &AppHandle<R>,
    monitors: &[MonitorGeometry],
) -> Result<usize, CaptureError> {
    let position = app
        .cursor_position()
        .map_err(|error| platform_error("read cursor position", error))?;
    let point = PhysicalPoint {
        x: checked_rounded_i32(position.x)?,
        y: checked_rounded_i32(position.y)?,
    };
    super::contract::select_monitor_at(monitors, point)
        .and_then(|selected| {
            monitors
                .iter()
                .position(|monitor| monitor.id == selected.id)
        })
        .ok_or_else(|| {
            CaptureError::new(
                CaptureErrorCode::NoMonitor,
                "the pointer is not inside an active monitor",
            )
        })
}

#[cfg(not(target_os = "macos"))]
fn xcap_monitor_bounds(monitor: &XcapMonitor) -> Result<PhysicalRect, CaptureError> {
    let x = monitor
        .x()
        .map_err(|error| capture_error("read xcap monitor x", error))?;
    let y = monitor
        .y()
        .map_err(|error| capture_error("read xcap monitor y", error))?;
    let width = monitor
        .width()
        .map_err(|error| capture_error("read xcap monitor width", error))?;
    let height = monitor
        .height()
        .map_err(|error| capture_error("read xcap monitor height", error))?;
    let scale_factor = f64::from(
        monitor
            .scale_factor()
            .map_err(|error| capture_error("read xcap monitor scale", error))?,
    );
    xcap_bounds_to_physical(x, y, width, height, scale_factor, XCAP_GEOMETRY_CONVENTION)
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
fn physical_rects_match(left: PhysicalRect, right: PhysicalRect) -> bool {
    const ROUNDING_TOLERANCE: u64 = 4;
    i64::from(left.origin.x).abs_diff(i64::from(right.origin.x)) <= ROUNDING_TOLERANCE
        && i64::from(left.origin.y).abs_diff(i64::from(right.origin.y)) <= ROUNDING_TOLERANCE
        && u64::from(left.size.width).abs_diff(u64::from(right.size.width)) <= ROUNDING_TOLERANCE
        && u64::from(left.size.height).abs_diff(u64::from(right.size.height)) <= ROUNDING_TOLERANCE
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn find_matching_candidate<T>(
    candidates: impl IntoIterator<Item = Result<(PhysicalRect, T), CaptureError>>,
    selected: PhysicalRect,
) -> Result<T, CaptureError> {
    let mut last_error = None;
    for candidate in candidates {
        match candidate {
            Ok((bounds, value)) if physical_rects_match(bounds, selected) => return Ok(value),
            Ok(_) => {}
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        CaptureError::new(
            CaptureErrorCode::NoMonitor,
            "the selected monitor disappeared before capture",
        )
    }))
}

fn platform_error(stage: &str, error: impl std::fmt::Display) -> CaptureError {
    CaptureError::new(
        CaptureErrorCode::CaptureFailed,
        format!("failed to {stage}: {error}"),
    )
}

fn capture_error(stage: &str, error: impl std::fmt::Display) -> CaptureError {
    CaptureError::new(
        CaptureErrorCode::CaptureFailed,
        format!("failed to {stage}: {error}"),
    )
}

#[cfg(test)]
mod platform_tests {
    use super::{
        find_matching_native_candidate, macos_monitor_geometry, select_monitor_by_native_id,
    };
    use crate::commands::screen_capture::contract::{CaptureError, CaptureErrorCode};

    #[test]
    fn mixed_dpi_selection_uses_native_id_instead_of_rescaling_global_coordinates() {
        let monitors = vec![
            macos_monitor_geometry(11, 0.0, 0.0, 1512.0, 982.0, 2.0).expect("Retina geometry"),
            macos_monitor_geometry(22, 1512.0, 0.0, 1920.0, 1080.0, 1.0)
                .expect("external geometry"),
        ];

        assert_eq!(select_monitor_by_native_id(&monitors, 22).unwrap(), 1);
        assert_eq!(monitors[0].physical_size.width, 3024);
        assert_eq!(monitors[1].physical_size.width, 1920);
    }

    #[test]
    fn native_id_mapping_fails_closed_for_missing_or_duplicate_monitors() {
        let monitor =
            macos_monitor_geometry(42, 0.0, 0.0, 100.0, 100.0, 2.0).expect("valid geometry");

        assert_eq!(
            select_monitor_by_native_id(std::slice::from_ref(&monitor), 7)
                .expect_err("missing display")
                .code,
            CaptureErrorCode::NoMonitor
        );
        assert_eq!(
            select_monitor_by_native_id(&[monitor.clone(), monitor], 42)
                .expect_err("ambiguous display")
                .code,
            CaptureErrorCode::InvalidMonitor
        );
    }

    #[test]
    fn xcap_mapping_skips_stale_candidates_but_rejects_duplicate_native_ids() {
        let stale_error = CaptureError::new(CaptureErrorCode::CaptureFailed, "stale candidate");
        let selected = find_matching_native_candidate(
            [Err(stale_error), Ok((7, "wrong")), Ok((9, "selected"))],
            9,
        )
        .expect("exact native display");
        assert_eq!(selected, "selected");

        assert_eq!(
            find_matching_native_candidate([Ok((9, "first")), Ok((9, "second"))], 9)
                .expect_err("duplicate display ids must fail closed")
                .code,
            CaptureErrorCode::InvalidMonitor
        );
    }

    #[test]
    fn macos_geometry_rejects_invalid_core_graphics_bounds() {
        for invalid in [
            macos_monitor_geometry(1, f64::NAN, 0.0, 100.0, 100.0, 1.0),
            macos_monitor_geometry(1, 0.0, 0.0, 0.0, 100.0, 1.0),
            macos_monitor_geometry(1, 0.0, 0.0, 100.0, 100.0, 0.0),
        ] {
            assert_eq!(
                invalid.expect_err("invalid display bounds").code,
                CaptureErrorCode::InvalidMonitor
            );
        }
    }
}
