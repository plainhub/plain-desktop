use serde::{Deserialize, Serialize};

pub const MAX_RAW_FRAME_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_PNG_RESULT_BYTES: usize = 160 * 1024 * 1024;

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
pub struct PhysicalRect {
    pub origin: PhysicalPoint,
    pub size: PhysicalSize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FramePoint {
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameRect {
    pub origin: FramePoint,
    pub size: FrameSize,
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
pub struct MonitorGeometry {
    pub id: String,
    pub physical_origin: PhysicalPoint,
    pub physical_size: PhysicalSize,
    pub logical_origin: LogicalPoint,
    pub logical_size: LogicalSize,
    pub scale_factor: f64,
}

impl MonitorGeometry {
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

pub fn select_monitor_at(
    monitors: &[MonitorGeometry],
    point: PhysicalPoint,
) -> Option<&MonitorGeometry> {
    monitors.iter().find(|monitor| monitor.contains(point))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixelFormat {
    Rgba8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedFrameDescriptor {
    pub session_id: String,
    pub monitor: MonitorGeometry,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub pixel_format: PixelFormat,
    pub byte_len: usize,
}

#[derive(Debug)]
pub struct CapturedFrame {
    descriptor: CapturedFrameDescriptor,
    bytes: Vec<u8>,
}

impl CapturedFrame {
    pub fn new(
        session_id: impl Into<String>,
        monitor: MonitorGeometry,
        width: u32,
        height: u32,
        stride: u32,
        bytes: Vec<u8>,
    ) -> Result<Self, CaptureError> {
        monitor.validate()?;
        if width != monitor.physical_size.width || height != monitor.physical_size.height {
            return Err(CaptureError::new(
                CaptureErrorCode::InvalidFrame,
                "captured frame dimensions do not match the selected monitor",
            ));
        }
        Self::validate_layout(width, height, stride, bytes.len())?;
        let session_id = session_id.into();
        if session_id.trim().is_empty() {
            return Err(CaptureError::new(
                CaptureErrorCode::InvalidFrame,
                "captured frame requires a session id",
            ));
        }
        Ok(Self {
            descriptor: CapturedFrameDescriptor {
                session_id,
                monitor,
                width,
                height,
                stride,
                pixel_format: PixelFormat::Rgba8,
                byte_len: bytes.len(),
            },
            bytes,
        })
    }

    pub fn validate_layout(
        width: u32,
        height: u32,
        stride: u32,
        byte_len: usize,
    ) -> Result<(), CaptureError> {
        if width == 0 || height == 0 {
            return Err(CaptureError::new(
                CaptureErrorCode::InvalidFrame,
                "captured frame dimensions must be non-zero",
            ));
        }

        let row_bytes = u64::from(width).checked_mul(4).ok_or_else(|| {
            CaptureError::new(CaptureErrorCode::FrameTooLarge, "RGBA row size overflow")
        })?;
        if row_bytes > u64::from(u32::MAX) {
            return Err(CaptureError::new(
                CaptureErrorCode::FrameTooLarge,
                "RGBA row size cannot be represented by the capture contract",
            ));
        }
        if u64::from(stride) < row_bytes {
            return Err(CaptureError::new(
                CaptureErrorCode::InvalidFrame,
                "captured frame stride is shorter than one RGBA row",
            ));
        }
        let expected = u64::from(stride)
            .checked_mul(u64::from(height))
            .ok_or_else(|| {
                CaptureError::new(CaptureErrorCode::FrameTooLarge, "frame size overflow")
            })?;
        if expected > MAX_RAW_FRAME_BYTES as u64 {
            return Err(CaptureError::new(
                CaptureErrorCode::FrameTooLarge,
                "captured frame exceeds the process memory limit",
            ));
        }
        if expected != byte_len as u64 {
            return Err(CaptureError::new(
                CaptureErrorCode::InvalidFrame,
                "captured frame byte length does not match its layout",
            ));
        }
        Ok(())
    }

    pub fn descriptor(&self) -> &CapturedFrameDescriptor {
        &self.descriptor
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureTriggerKind {
    Composer,
    Global,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureOrigin {
    pub window_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureTarget {
    pub window_label: String,
    pub target_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureRequest {
    pub session_id: String,
    pub trigger: CaptureTriggerKind,
    pub origin: Option<CaptureOrigin>,
    pub target: Option<CaptureTarget>,
}

impl CaptureRequest {
    pub fn validate(&self) -> Result<(), CaptureError> {
        if self.session_id.trim().is_empty() {
            return Err(CaptureError::new(
                CaptureErrorCode::InvalidSession,
                "capture request requires a session id",
            ));
        }
        if self
            .origin
            .as_ref()
            .is_some_and(|origin| origin.window_label.trim().is_empty())
        {
            return Err(CaptureError::new(
                CaptureErrorCode::InvalidSession,
                "capture origin requires a window label",
            ));
        }
        if self.target.as_ref().is_some_and(|target| {
            target.window_label.trim().is_empty() || target.target_token.trim().is_empty()
        }) {
            return Err(CaptureError::new(
                CaptureErrorCode::TargetUnavailable,
                "capture target is incomplete",
            ));
        }
        if self.trigger == CaptureTriggerKind::Composer
            && (self.origin.is_none() || self.target.is_none())
        {
            return Err(CaptureError::new(
                CaptureErrorCode::TargetUnavailable,
                "composer capture requires an origin and delivery target",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeCapturePhase {
    Idle,
    WaitingForOverlay,
    HidingOrigin,
    Capturing,
    FrameAvailable,
    AwaitingPresentation,
    Active,
    ResultAvailable,
    Delivering,
    Restoring,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureResultDescriptor {
    pub session_id: String,
    pub result_id: String,
    pub width: u32,
    pub height: u32,
    pub filename: String,
    pub mime_type: String,
    pub byte_len: usize,
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
