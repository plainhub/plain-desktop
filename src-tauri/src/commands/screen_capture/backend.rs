use std::sync::atomic::{AtomicBool, Ordering};

use super::contract::{CaptureError, CaptureErrorCode, CapturedFrame, MonitorGeometry};

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

#[derive(Debug)]
pub struct NativeFrame {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub bytes: Vec<u8>,
}

pub trait ScreenCaptureBackend: Send + Sync {
    fn monitors(&self) -> Result<Vec<MonitorGeometry>, CaptureError>;

    fn monitor_index_at_cursor(&self, monitors: &[MonitorGeometry]) -> Result<usize, CaptureError>;

    fn capture_monitor(&self, monitor: &MonitorGeometry) -> Result<NativeFrame, CaptureError>;
}

pub fn capture_frame_at_cursor(
    backend: &dyn ScreenCaptureBackend,
    session_id: &str,
) -> Result<CapturedFrame, CaptureError> {
    let monitors = backend.monitors()?;
    if monitors.is_empty() {
        return Err(CaptureError::new(
            CaptureErrorCode::NoMonitor,
            "no active monitor is available",
        ));
    }
    for monitor in &monitors {
        monitor.validate()?;
    }

    let monitor_index = backend.monitor_index_at_cursor(&monitors)?;
    let monitor = monitors.get(monitor_index).ok_or_else(|| {
        CaptureError::new(CaptureErrorCode::NoMonitor, "the selected monitor is stale")
    })?;
    let stride = monitor.physical_size.width.checked_mul(4).ok_or_else(|| {
        CaptureError::new(CaptureErrorCode::FrameTooLarge, "RGBA stride overflow")
    })?;
    let expected_len = usize::try_from(u64::from(stride) * u64::from(monitor.physical_size.height))
        .map_err(|_| {
            CaptureError::new(
                CaptureErrorCode::FrameTooLarge,
                "frame size exceeds this platform's address space",
            )
        })?;
    CapturedFrame::validate_layout(
        monitor.physical_size.width,
        monitor.physical_size.height,
        stride,
        expected_len,
    )?;
    let native = backend.capture_monitor(monitor)?;
    CapturedFrame::new(
        session_id,
        monitor.clone(),
        native.width,
        native.height,
        native.stride,
        native.bytes,
    )
}

/// Serialize xcap/X11 access across session lifetimes. A timed-out
/// `spawn_blocking` task cannot be cancelled, so its lease remains held by the
/// blocking closure until that backend call really exits. A new session fails
/// closed instead of concurrently entering the X11 display backend.
pub fn capture_frame_at_cursor_exclusive(
    backend: &dyn ScreenCaptureBackend,
    session_id: &str,
) -> Result<CapturedFrame, CaptureError> {
    with_native_acquisition_lease(|| capture_frame_at_cursor(backend, session_id))
}

/// Serialize every blocking native acquisition, including portal PipeWire
/// consumers that outlive a cancelled async task. This prevents a stale
/// backend thread and its replacement from retaining full-screen buffers at
/// the same time.
pub fn with_native_acquisition_lease<T>(
    operation: impl FnOnce() -> Result<T, CaptureError>,
) -> Result<T, CaptureError> {
    let _lease = NATIVE_ACQUISITION_GATE.try_acquire()?;
    operation()
}

#[cfg(test)]
mod acquisition_gate_tests {
    use super::*;

    #[test]
    fn gate_rejects_overlap_and_releases_only_when_the_first_lease_drops() {
        let gate = CaptureAcquisitionGate::new();
        let first = gate.try_acquire().expect("first acquisition");
        let overlap = match gate.try_acquire() {
            Ok(_) => panic!("overlap must fail"),
            Err(error) => error,
        };
        assert_eq!(overlap.code, CaptureErrorCode::Busy);
        drop(first);
        gate.try_acquire()
            .expect("gate released after backend exit");
    }
}
