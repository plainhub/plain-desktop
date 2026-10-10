//! macOS plain-capture backend: ScreenCaptureKit single-frame screenshots
//! over CoreGraphics display enumeration.
//!
//! Enumeration and pointer geometry use CoreGraphics directly (no permission
//! required), so the overlay can be planned before the system prompt. Frame
//! acquisition goes through `SCScreenshotManager` (macOS 14+): one call, one
//! CGImage, no stream machinery.

use std::sync::{Arc, Condvar, Mutex};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::{AllocAnyThread, Message};
use objc2_core_graphics::{
    CGDataProvider, CGDirectDisplayID, CGDisplayBounds, CGDisplayIsActive, CGError, CGEvent,
    CGGetActiveDisplayList, CGGetDisplaysWithPoint, CGImage, CGPreflightScreenCaptureAccess,
    CGRequestScreenCaptureAccess,
};
use objc2_foundation::{NSArray, NSError};
use objc2_screen_capture_kit::{
    SCContentFilter, SCDisplay, SCScreenshotManager, SCShareableContent, SCStreamConfiguration,
};

use crate::capture::Capture;
use crate::capture::CaptureError;
use crate::capture::CaptureErrorCode;
use crate::capture::DisplayInfo;
use crate::capture::LogicalPoint;
use crate::capture::LogicalSize;
use crate::capture::PhysicalPoint;
use crate::capture::PhysicalSize;
use crate::capture::checked_rounded_i32;
use crate::capture::checked_rounded_u32;
use crate::capture::permission_preflight_result;

const MACOS_NATIVE_DISPLAY_PREFIX: &str = "macos-cg-display:";

pub(crate) fn monitor_id(cg_display_id: u32) -> String {
    format!("{MACOS_NATIVE_DISPLAY_PREFIX}{cg_display_id}")
}

pub(crate) fn parse_display_id(value: &str) -> Option<u32> {
    let raw = value.strip_prefix(MACOS_NATIVE_DISPLAY_PREFIX)?;
    let cg_display_id = raw.parse::<u32>().ok()?;
    (cg_display_id != 0 && monitor_id(cg_display_id) == value).then_some(cg_display_id)
}

pub struct MacosCapture;

impl MacosCapture {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MacosCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl Capture for MacosCapture {
    fn displays(&self) -> Result<Vec<DisplayInfo>, CaptureError> {
        monitor_geometries()
    }

    fn display_index_at_cursor(&self, displays: &[DisplayInfo]) -> Result<usize, CaptureError> {
        select_display_by_native_id(displays, core_graphics_display_id_at_cursor()?)
    }

    fn capture_display(
        &self,
        display: &DisplayInfo,
        exclude_window_ids: &[u64],
    ) -> Result<crate::capture::Frame, CaptureError> {
        capture_display(display, exclude_window_ids)
    }
}

use crate::capture::Frame;

pub(crate) fn preflight_capture_permission() -> Result<(), CaptureError> {
    permission_preflight_result(CGPreflightScreenCaptureAccess())
}

/// Request capture access only from the explicit action in Plain's guidance.
pub(crate) fn request_capture_permission() -> bool {
    CGPreflightScreenCaptureAccess() || CGRequestScreenCaptureAccess()
}

/// The monitor geometry for the display under the pointer. macOS overlays are
/// created at their final on-screen geometry, so the capture runtime needs the
/// destination monitor before the frame is acquired.
pub(crate) fn monitor_geometry_at_cursor() -> Result<DisplayInfo, CaptureError> {
    let cg_display_id = core_graphics_display_id_at_cursor()?;
    monitor_geometries()?
        .into_iter()
        .find(|monitor| parse_display_id(&monitor.id) == Some(cg_display_id))
        .ok_or_else(|| {
            CaptureError::new(
                CaptureErrorCode::NoMonitor,
                "the display at the pointer is no longer available",
            )
        })
}

fn select_display_by_native_id(
    displays: &[DisplayInfo],
    cg_display_id: u32,
) -> Result<usize, CaptureError> {
    let mut matches = displays
        .iter()
        .enumerate()
        .filter(|(_, display)| parse_display_id(&display.id) == Some(cg_display_id))
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

fn active_core_graphics_display_ids() -> Result<Vec<u32>, CaptureError> {
    const MAX_ACTIVE_DISPLAYS: u32 = 64;
    let mut displays = vec![0 as CGDirectDisplayID; MAX_ACTIVE_DISPLAYS as usize];
    let mut count = 0;
    let error =
        unsafe { CGGetActiveDisplayList(MAX_ACTIVE_DISPLAYS, displays.as_mut_ptr(), &mut count) };
    if error != CGError::Success {
        return Err(capture_platform_error(
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

fn monitor_geometries() -> Result<Vec<DisplayInfo>, CaptureError> {
    active_core_graphics_display_ids()?
        .into_iter()
        .map(|display_id| {
            let scale_factor = display_scale_factor(display_id)?;
            let bounds = CGDisplayBounds(display_id);
            native_display_geometry(
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

fn native_display_geometry(
    display_id: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    scale_factor: f64,
) -> Result<DisplayInfo, CaptureError> {
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
    let geometry = DisplayInfo {
        id: monitor_id(display_id),
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

/// CoreGraphics display-mode C API. Stable ABI since 10.6; the objc2
/// bindings do not surface the pixel/logical pair needed for the scale factor.
mod display_mode {
    use std::ffi::c_void;

    unsafe extern "C" {
        pub(crate) fn CGDisplayCopyDisplayMode(display: u32) -> *mut c_void;
        pub(crate) fn CGDisplayModeGetPixelWidth(mode: *mut c_void) -> usize;
        pub(crate) fn CGDisplayModeGetWidth(mode: *mut c_void) -> f64;
        pub(crate) fn CFRelease(cf: *mut c_void);
    }
}

fn display_scale_factor(cg_display_id: u32) -> Result<f64, CaptureError> {
    unsafe {
        let mode = display_mode::CGDisplayCopyDisplayMode(cg_display_id);
        if mode.is_null() {
            return Err(CaptureError::new(
                CaptureErrorCode::InvalidMonitor,
                "CoreGraphics returned no display mode for the monitor",
            ));
        }
        let scale = display_mode::CGDisplayModeGetPixelWidth(mode) as f64
            / display_mode::CGDisplayModeGetWidth(mode);
        display_mode::CFRelease(mode);
        if !scale.is_finite() || scale <= 0.0 {
            return Err(CaptureError::new(
                CaptureErrorCode::InvalidMonitor,
                "CoreGraphics returned an invalid monitor scale factor",
            ));
        }
        Ok(scale)
    }
}

fn core_graphics_display_id_at_cursor() -> Result<u32, CaptureError> {
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
        return Err(capture_platform_error(
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

fn capture_platform_error(stage: &str, error: impl std::fmt::Display) -> CaptureError {
    CaptureError::new(
        CaptureErrorCode::CaptureFailed,
        format!("failed to {stage}: {error}"),
    )
}

/// Blocks the calling worker until an SCK completion handler fires on its
/// internal queue. Capture runs on `spawn_blocking` workers, never on the
/// AppKit main thread, so parking here cannot deadlock the UI. The retained
/// ObjC values are not `Send`; the hand-off stays confined to this handshake.
#[derive(Clone)]
struct CompletionSlot<T> {
    #[allow(clippy::arc_with_non_send_sync)]
    inner: Arc<Mutex<Option<Result<T, String>>>>,
    #[allow(clippy::arc_with_non_send_sync)]
    done: Arc<Condvar>,
}

impl<T> CompletionSlot<T> {
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
            done: Arc::new(Condvar::new()),
        }
    }

    fn complete(&self, outcome: Result<T, String>) {
        *self.inner.lock().unwrap() = Some(outcome);
        self.done.notify_one();
    }

    fn wait(self, absent: &str, context: &str) -> Result<T, CaptureError> {
        let mut guard = self
            .done
            .wait_while(self.inner.lock().unwrap(), |slot| slot.is_none())
            .unwrap();
        guard
            .take()
            .unwrap_or_else(|| Err(absent.to_string()))
            .map_err(|message| {
                CaptureError::new(
                    CaptureErrorCode::CaptureFailed,
                    format!("failed to {context}: {message}"),
                )
            })
    }
}

fn shareable_content() -> Result<Retained<SCShareableContent>, CaptureError> {
    let pending: CompletionSlot<Retained<SCShareableContent>> = CompletionSlot::new();
    let waiter = pending.clone();

    let block = RcBlock::new(
        move |content: *mut SCShareableContent, error: *mut NSError| {
            let outcome = if !error.is_null() {
                Err(unsafe { &*error }.localizedDescription().to_string())
            } else if content.is_null() {
                Err("ScreenCaptureKit returned no shareable content".to_string())
            } else {
                unsafe { Retained::retain(content) }.ok_or_else(|| {
                    "ScreenCaptureKit shareable content could not be retained".to_string()
                })
            };
            waiter.complete(outcome);
        },
    );

    unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&block) };

    pending.wait(
        "ScreenCaptureKit content request ended without a result",
        "enumerate ScreenCaptureKit content",
    )
}

fn excluded_sc_windows(
    content: &SCShareableContent,
    exclude_window_ids: &[u64],
) -> Vec<Retained<objc2_screen_capture_kit::SCWindow>> {
    if exclude_window_ids.is_empty() {
        return Vec::new();
    }
    let mut excluded = Vec::new();
    for window in unsafe { content.windows() }.iter() {
        if exclude_window_ids.contains(&u64::from(unsafe { window.windowID() })) {
            excluded.push(window.retain());
        }
    }
    excluded
}

fn sc_display_for(
    content: &SCShareableContent,
    cg_display_id: u32,
) -> Result<Retained<SCDisplay>, CaptureError> {
    let displays = unsafe { content.displays() };
    for display in displays.iter() {
        if unsafe { display.displayID() } == cg_display_id {
            return Ok(display.retain());
        }
    }
    Err(CaptureError::new(
        CaptureErrorCode::NoMonitor,
        "the selected native display disappeared before capture",
    ))
}

/// Blocks until SCScreenshotManager hands back one CGImage.
fn capture_sck_image(
    filter: &SCContentFilter,
    config: &SCStreamConfiguration,
) -> Result<Retained<CGImage>, CaptureError> {
    let pending: CompletionSlot<Retained<CGImage>> = CompletionSlot::new();
    let waiter = pending.clone();

    let block = RcBlock::new(move |image: *mut CGImage, error: *mut NSError| {
        let outcome = if !error.is_null() {
            Err(unsafe { &*error }.localizedDescription().to_string())
        } else if image.is_null() {
            Err("ScreenCaptureKit returned a null screenshot".to_string())
        } else {
            unsafe { Retained::retain(image) }
                .ok_or_else(|| "ScreenCaptureKit screenshot could not be retained".to_string())
        };
        waiter.complete(outcome);
    });

    unsafe {
        SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
            filter,
            config,
            Some(&block),
        );
    }

    pending.wait(
        "ScreenCaptureKit screenshot ended without a result",
        "capture the screen",
    )
}

/// SCScreenshotManager documents BGRA (premultiplied, SDR) image data; swizzle
/// into the capture contract's RGBA layout and strip any row padding.
fn cg_image_to_frame(image: &CGImage) -> Result<Frame, CaptureError> {
    let width = u32::try_from(CGImage::width(Some(image)))
        .map_err(|_| CaptureError::new(CaptureErrorCode::FrameTooLarge, "frame width overflow"))?;
    let height = u32::try_from(CGImage::height(Some(image)))
        .map_err(|_| CaptureError::new(CaptureErrorCode::FrameTooLarge, "frame height overflow"))?;
    if width == 0 || height == 0 {
        return Err(CaptureError::new(
            CaptureErrorCode::InvalidFrame,
            "ScreenCaptureKit returned an empty screenshot",
        ));
    }
    let stride = width.checked_mul(4).ok_or_else(|| {
        CaptureError::new(CaptureErrorCode::FrameTooLarge, "RGBA stride overflow")
    })?;

    let provider = CGImage::data_provider(Some(image)).ok_or_else(|| {
        CaptureError::new(CaptureErrorCode::CaptureFailed, "no image data provider")
    })?;
    let data = CGDataProvider::data(Some(&provider)).ok_or_else(|| {
        CaptureError::new(
            CaptureErrorCode::CaptureFailed,
            "the screenshot data provider returned no bytes",
        )
    })?;
    let raw = data.to_vec();

    let expected = usize::try_from(u64::from(stride) * u64::from(height))
        .map_err(|_| CaptureError::new(CaptureErrorCode::FrameTooLarge, "frame size overflow"))?;
    if raw.len() < expected {
        return Err(CaptureError::new(
            CaptureErrorCode::FrameTooLarge,
            format!(
                "screenshot bytes {} are shorter than the declared layout {expected}",
                raw.len()
            ),
        ));
    }

    let mut bytes = vec![0u8; expected];
    for (row, out_row) in bytes
        .chunks_exact_mut(usize::try_from(stride).unwrap_or(usize::MAX))
        .enumerate()
    {
        let src = &raw[row * stride as usize..][..stride as usize];
        out_row.copy_from_slice(src);
    }
    for pixel in bytes.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }

    Ok(Frame {
        width,
        height,
        stride,
        bytes,
    })
}

fn capture_display(
    display: &DisplayInfo,
    exclude_window_ids: &[u64],
) -> Result<Frame, CaptureError> {
    preflight_capture_permission()?;
    let cg_display_id = parse_display_id(&display.id).ok_or_else(|| {
        CaptureError::new(
            CaptureErrorCode::InvalidMonitor,
            "selected macOS monitor has no native display id",
        )
    })?;

    let content = shareable_content()?;
    let sc_display = sc_display_for(&content, cg_display_id)?;
    // The overlay window shows the frozen frame full-screen while the stream
    // is captured; without the exclusion SCK photographs our own backdrop.
    let excluded = excluded_sc_windows(&content, exclude_window_ids);
    let excluded_refs: Vec<&objc2_screen_capture_kit::SCWindow> =
        excluded.iter().map(|window| window.as_ref()).collect();
    let excluded_windows = NSArray::from_slice(&excluded_refs);
    let filter = unsafe {
        SCContentFilter::initWithDisplay_excludingWindows(
            SCContentFilter::alloc(),
            &sc_display,
            &excluded_windows,
        )
    };

    let config = unsafe { SCStreamConfiguration::new() };
    unsafe {
        config.setWidth(display.physical_size.width as usize);
        config.setHeight(display.physical_size.height as usize);
        config.setShowsCursor(false);
    }

    let image = capture_sck_image(&filter, &config)?;
    cg_image_to_frame(&image)
}

#[cfg(test)]
mod tests {
    use super::monitor_id;
    use super::native_display_geometry;
    use super::parse_display_id;
    use super::select_display_by_native_id;
    use crate::capture::CaptureErrorCode;

    #[test]
    fn mixed_dpi_selection_uses_native_id_instead_of_rescaling_global_coordinates() {
        let monitors = vec![
            native_display_geometry(11, 0.0, 0.0, 1512.0, 982.0, 2.0).expect("Retina geometry"),
            native_display_geometry(22, 1512.0, 0.0, 1920.0, 1080.0, 1.0)
                .expect("external geometry"),
        ];

        assert_eq!(select_display_by_native_id(&monitors, 22).unwrap(), 1);
        assert_eq!(monitors[0].physical_size.width, 3024);
        assert_eq!(monitors[1].physical_size.width, 1920);
    }

    #[test]
    fn native_id_mapping_fails_closed_for_missing_or_duplicate_monitors() {
        let monitor =
            native_display_geometry(42, 0.0, 0.0, 100.0, 100.0, 2.0).expect("valid geometry");

        assert_eq!(
            select_display_by_native_id(std::slice::from_ref(&monitor), 7)
                .expect_err("missing display")
                .code,
            CaptureErrorCode::NoMonitor
        );
        assert_eq!(
            select_display_by_native_id(&[monitor.clone(), monitor], 42)
                .expect_err("ambiguous display")
                .code,
            CaptureErrorCode::InvalidMonitor
        );
    }

    #[test]
    fn display_geometry_rejects_invalid_core_graphics_bounds() {
        for invalid in [
            native_display_geometry(1, f64::NAN, 0.0, 100.0, 100.0, 1.0),
            native_display_geometry(1, 0.0, 0.0, 0.0, 100.0, 1.0),
            native_display_geometry(1, 0.0, 0.0, 100.0, 100.0, 0.0),
        ] {
            assert_eq!(
                invalid.expect_err("invalid display bounds").code,
                CaptureErrorCode::InvalidMonitor
            );
        }
    }

    #[test]
    fn display_id_round_trip_rejects_zero_and_garbage() {
        let id = monitor_id(7);
        assert_eq!(parse_display_id(&id), Some(7));
        assert_eq!(parse_display_id("macos-cg-display:0"), None);
        assert_eq!(parse_display_id("macos-cg-display:xyz"), None);
        assert_eq!(parse_display_id("wayland-winit:0"), None);
    }
}
