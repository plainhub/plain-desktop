use super::contract::{CaptureError, CaptureErrorCode, CapturedFrame};
use crate::capture::{Capture, with_native_acquisition_lease};

pub(crate) fn capture_frame_at_cursor(
    backend: &dyn Capture,
    session_id: &str,
    exclude_window_ids: &[u64],
) -> Result<CapturedFrame, CaptureError> {
    let displays = backend.displays()?;
    if displays.is_empty() {
        return Err(CaptureError::new(
            CaptureErrorCode::NoMonitor,
            "no active monitor is available",
        ));
    }
    for display in &displays {
        display.validate()?;
    }

    let display_index = backend.display_index_at_cursor(&displays)?;
    let display = displays.get(display_index).ok_or_else(|| {
        CaptureError::new(CaptureErrorCode::NoMonitor, "the selected monitor is stale")
    })?;
    let stride = display.physical_size.width.checked_mul(4).ok_or_else(|| {
        CaptureError::new(CaptureErrorCode::FrameTooLarge, "RGBA stride overflow")
    })?;
    let expected_len = usize::try_from(u64::from(stride) * u64::from(display.physical_size.height))
        .map_err(|_| {
            CaptureError::new(
                CaptureErrorCode::FrameTooLarge,
                "frame size exceeds this platform's address space",
            )
        })?;
    CapturedFrame::validate_layout(
        display.physical_size.width,
        display.physical_size.height,
        stride,
        expected_len,
    )?;
    let native = backend.capture_display(display, exclude_window_ids)?;
    CapturedFrame::new(
        session_id,
        display.clone(),
        native.width,
        native.height,
        native.stride,
        native.bytes,
    )
}

/// Serialize native acquisitions across session lifetimes and take the frame
/// under the cursor's display.
pub fn capture_frame_at_cursor_exclusive(
    backend: &dyn Capture,
    session_id: &str,
    exclude_window_ids: &[u64],
) -> Result<CapturedFrame, CaptureError> {
    with_native_acquisition_lease(|| {
        capture_frame_at_cursor(backend, session_id, exclude_window_ids)
    })
}
