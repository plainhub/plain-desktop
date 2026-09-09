use std::future::Future;
use std::sync::{Arc, Mutex, TryLockError};
use std::time::Duration;

use serde::Serialize;

#[cfg(test)]
use super::backend::ScreenCaptureBackend;
use super::buffers::validate_result_payload;
use super::contract::{
    CaptureError, CaptureErrorCode, CaptureOrigin, CaptureRequest, CaptureResultDescriptor,
    CaptureResultSubmission, CaptureTarget, CaptureTriggerKind, CapturedFrame,
    CapturedFrameDescriptor, MonitorGeometry, NativeCapturePhase,
};
use super::coordinator::{CaptureCallerRole, CaptureCleanup, CaptureCoordinator, TerminalOutcome};
use super::export::{CaptureExportPort, SaveCaptureOutcome, normalized_png_path};

pub const OVERLAY_WINDOW_LABEL_PREFIX: &str = "screen-capture-overlay";
pub const OVERLAY_WINDOW_LABEL: &str = "screen-capture-overlay-1";
pub const OVERLAY_ROUTE: &str = "/screen-capture";
pub const FRAME_AVAILABLE_EVENT: &str = "screen-capture://frame-available";
pub const RESULT_AVAILABLE_EVENT: &str = "screen-capture://result-available";
pub const DELIVERY_FAILED_EVENT: &str = "screen-capture://delivery-failed";
pub const SESSION_STARTED_EVENT: &str = "screen-capture://session-started";
pub const SESSION_ENDED_EVENT: &str = "screen-capture://session-ended";
pub const OVERLAY_SESSION_ENDED_EVENT: &str = "screen-capture://overlay-session-ended";
pub const TARGET_UNAVAILABLE_EVENT: &str = "screen-capture://target-unavailable";
pub const CAPTURE_PROTOCOL_VERSION: u32 = 1;
pub const MAX_JS_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

pub fn overlay_window_label(overlay_generation: u64) -> String {
    format!("{OVERLAY_WINDOW_LABEL_PREFIX}-{overlay_generation}")
}

pub fn is_overlay_window_label(label: &str) -> bool {
    label
        .strip_prefix(OVERLAY_WINDOW_LABEL_PREFIX)
        .and_then(|suffix| suffix.strip_prefix('-'))
        .and_then(|generation| generation.parse::<u64>().ok())
        .is_some_and(|generation| {
            generation > 0
                && generation <= MAX_JS_SAFE_INTEGER
                && overlay_window_label(generation) == label
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayWindowSpec {
    pub label: String,
    pub route: String,
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameAvailablePayload {
    pub session_id: String,
    pub overlay_generation: u64,
    pub descriptor: CapturedFrameDescriptor,
    pub can_confirm: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultAvailablePayload {
    pub target_token: String,
    pub descriptor: CaptureResultDescriptor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStartedPayload {
    pub session_id: String,
    pub target_token: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEndOutcome {
    Completed,
    Cancelled,
    Saved,
    Copied,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEndedPayload {
    pub session_id: String,
    pub target_token: String,
    pub outcome: SessionEndOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<CaptureErrorCode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlaySessionEndedPayload {
    pub session_id: String,
    pub overlay_generation: u64,
    pub outcome: SessionEndOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetUnavailablePayload {
    pub session_id: String,
    pub overlay_generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryFailedPayload {
    pub session_id: String,
    pub overlay_generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayInit {
    pub overlay_generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureStartResponse {
    pub session_id: String,
    pub overlay_generation: u64,
    pub phase: NativeCapturePhase,
}

/// Native window state frozen immediately before capture hides an origin.
/// Restoration must apply only these recorded properties; cleanup must never
/// opportunistically show, unminimize, or focus a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureWindowState {
    pub visible: bool,
    pub minimized: bool,
    pub focused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayConcealment {
    Hidden,
    /// The platform queued destruction outside the current Tauri command
    /// stack. Runtime authority must rotate immediately, while a new capture
    /// remains blocked until the framework confirms destruction.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    RetirementScheduled,
    /// Hiding failed, so the adapter queued destruction outside the caller's
    /// stack. The error remains observable while retry waits for destruction.
    DestructionDeferred(CaptureError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureTicket {
    session_id: String,
    overlay_generation: u64,
}

impl CaptureTicket {
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn overlay_generation(&self) -> u64 {
        self.overlay_generation
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureReservation {
    pub response: CaptureStartResponse,
    pub ticket: Option<CaptureTicket>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapturePublishOutcome {
    Published,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontendFailureCode {
    OverlayBusy,
    StaleOverlayGeneration,
    FrameDecodeFailed,
    FramePresentationFailed,
    ProtocolMismatch,
}

impl FrontendFailureCode {
    fn parse(value: &str) -> Result<Self, CaptureError> {
        match value {
            "overlay_busy" => Ok(Self::OverlayBusy),
            "stale_overlay_generation" => Ok(Self::StaleOverlayGeneration),
            "frame_decode_failed" => Ok(Self::FrameDecodeFailed),
            "frame_presentation_failed" => Ok(Self::FramePresentationFailed),
            "protocol_mismatch" => Ok(Self::ProtocolMismatch),
            _ => Err(CaptureError::new(
                CaptureErrorCode::UnauthorizedCaller,
                "frontend capture failure code is not allowlisted",
            )),
        }
    }

    fn native_error(self, detail: &str) -> CaptureError {
        let code = match self {
            Self::OverlayBusy => CaptureErrorCode::Busy,
            Self::StaleOverlayGeneration => CaptureErrorCode::InvalidSession,
            Self::FrameDecodeFailed | Self::FramePresentationFailed => {
                CaptureErrorCode::InvalidFrame
            }
            Self::ProtocolMismatch => CaptureErrorCode::OverlayFailed,
        };
        CaptureError::new(code, bounded_frontend_detail(detail))
    }
}

pub trait CaptureWindowPort: Send + Sync {
    fn window_exists(&self, label: &str) -> bool;
    fn create_overlay(&self, spec: &OverlayWindowSpec) -> Result<(), CaptureError>;
    fn defers_overlay_position_until_presented(&self) -> bool {
        false
    }
    fn uses_ephemeral_overlay(&self) -> bool {
        false
    }
    fn conceal_overlay_for_capture(&self, _label: &str) -> Result<(), CaptureError> {
        Ok(())
    }
    fn wake_overlay_for_frame_delivery(
        &self,
        _label: &str,
        _monitor: &MonitorGeometry,
    ) -> Result<(), CaptureError> {
        Ok(())
    }
    fn capture_window_state(&self, label: &str)
    -> Result<Option<CaptureWindowState>, CaptureError>;
    fn hide_window(&self, label: &str) -> Result<(), CaptureError>;
    fn conceal_overlay(&self, label: &str) -> Result<OverlayConcealment, CaptureError>;
    fn restore_window(&self, label: &str, state: CaptureWindowState) -> Result<(), CaptureError>;
    /// Must enqueue the retry without synchronously calling back into runtime.
    fn defer_window_action_retry(&self, delay: Duration);
    fn focus_window(&self, label: &str) -> Result<(), CaptureError>;
    fn position_overlay(&self, label: &str, monitor: &MonitorGeometry) -> Result<(), CaptureError>;
    fn show_overlay(&self, label: &str) -> Result<(), CaptureError>;
    fn emit_frame_available(
        &self,
        label: &str,
        payload: &FrameAvailablePayload,
    ) -> Result<(), CaptureError>;
    fn emit_result_available(
        &self,
        label: &str,
        payload: &ResultAvailablePayload,
    ) -> Result<(), CaptureError>;
    fn emit_delivery_failed(
        &self,
        label: &str,
        payload: &DeliveryFailedPayload,
    ) -> Result<(), CaptureError>;
    fn emit_session_started(
        &self,
        label: &str,
        payload: &SessionStartedPayload,
    ) -> Result<(), CaptureError>;
    fn emit_session_ended(
        &self,
        label: &str,
        payload: &SessionEndedPayload,
    ) -> Result<(), CaptureError>;
    fn emit_target_unavailable(
        &self,
        label: &str,
        payload: &TargetUnavailablePayload,
    ) -> Result<(), CaptureError>;
    fn emit_overlay_session_ended(
        &self,
        label: &str,
        payload: &OverlaySessionEndedPayload,
    ) -> Result<(), CaptureError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeliveryLease {
    session_id: String,
    result_id: String,
    target_window_label: String,
    target_token: String,
    lease_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompletedDelivery {
    session_id: String,
    result_id: String,
    target_window_label: String,
    target_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InvalidatedTarget {
    session_id: String,
    target_window_label: String,
    target_token: String,
    overlay_generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResultExportLease {
    session_id: String,
    result_id: String,
    overlay_generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HiddenOrigin {
    session_id: String,
    window_label: String,
    state: CaptureWindowState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingOverlayAction {
    Hide,
    Rebuild,
}

#[derive(Debug)]
enum OverlayPreparation {
    Existing(OverlayInit),
    Create {
        init: OverlayInit,
        spec: OverlayWindowSpec,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingOriginRestore {
    window_label: String,
    state: CaptureWindowState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingWindowActions {
    overlay: Option<PendingOverlayAction>,
    origin: Option<PendingOriginRestore>,
    next_retry: usize,
}

impl PendingWindowActions {
    fn is_empty(&self) -> bool {
        self.overlay.is_none() && self.origin.is_none()
    }
}

const WINDOW_ACTION_RETRY_DELAYS: [Duration; 3] = [
    Duration::from_millis(50),
    Duration::from_millis(200),
    Duration::from_millis(500),
];
const ORIGIN_RESTORE_RECOVERY_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug)]
struct RuntimeInner {
    coordinator: CaptureCoordinator,
    eligible_targets: Vec<CaptureTarget>,
    current_overlay_generation: Option<u64>,
    loaded_overlay_generation: Option<u64>,
    retiring_overlay_generation: Option<u64>,
    next_overlay_generation: u64,
    hidden_origin: Option<HiddenOrigin>,
    pending_overlay_monitor: Option<MonitorGeometry>,
    pending_window_actions: Option<PendingWindowActions>,
    active_delivery_lease: Option<DeliveryLease>,
    last_completed_delivery: Option<CompletedDelivery>,
    invalidated_target: Option<InvalidatedTarget>,
    active_export: Option<ResultExportLease>,
}

#[derive(Debug)]
pub struct ScreenCaptureRuntime {
    inner: Mutex<RuntimeInner>,
    capture_start: Mutex<()>,
    overlay_creation: Mutex<()>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureTimeoutKind {
    Readiness,
    Presentation,
    Lifetime,
}

impl CaptureTimeoutKind {
    fn applies_to(self, phase: NativeCapturePhase) -> bool {
        match self {
            Self::Readiness => phase == NativeCapturePhase::WaitingForOverlay,
            Self::Presentation => matches!(
                phase,
                NativeCapturePhase::FrameAvailable | NativeCapturePhase::AwaitingPresentation
            ),
            Self::Lifetime => phase != NativeCapturePhase::Restoring,
        }
    }
}

impl ScreenCaptureRuntime {
    pub fn new() -> Result<Self, CaptureError> {
        Ok(Self {
            inner: Mutex::new(RuntimeInner {
                coordinator: CaptureCoordinator::new(OVERLAY_WINDOW_LABEL)?,
                eligible_targets: Vec::new(),
                current_overlay_generation: None,
                loaded_overlay_generation: None,
                retiring_overlay_generation: None,
                next_overlay_generation: 1,
                hidden_origin: None,
                pending_overlay_monitor: None,
                pending_window_actions: None,
                active_delivery_lease: None,
                last_completed_delivery: None,
                invalidated_target: None,
                active_export: None,
            }),
            capture_start: Mutex::new(()),
            overlay_creation: Mutex::new(()),
        })
    }

    #[cfg(test)]
    pub fn init_overlay(
        &self,
        caller_window_label: &str,
        windows: &dyn CaptureWindowPort,
    ) -> Result<OverlayInit, CaptureError> {
        if !is_regular_window_label(caller_window_label) {
            return Err(unauthorized_error(
                "only a regular application window may initialize capture",
            ));
        }
        self.ensure_overlay_native(windows)
    }

    #[cfg(test)]
    pub fn current_overlay_generation(&self) -> Result<Option<u64>, CaptureError> {
        Ok(self.lock()?.current_overlay_generation)
    }

    pub fn active_phase(&self) -> Result<NativeCapturePhase, CaptureError> {
        Ok(self.lock()?.coordinator.phase())
    }

    pub fn authorize_overlay_identity(
        &self,
        caller_window_label: &str,
        overlay_generation: u64,
    ) -> Result<(), CaptureError> {
        self.lock()?
            .require_current_overlay(caller_window_label, overlay_generation)
    }

    #[cfg(test)]
    pub fn has_sensitive_buffers(&self) -> Result<bool, CaptureError> {
        Ok(self.lock()?.coordinator.has_sensitive_buffers())
    }

    #[cfg(test)]
    pub fn has_pending_window_actions(&self) -> Result<bool, CaptureError> {
        Ok(self.lock()?.pending_window_actions.is_some())
    }

    /// Executes one bounded retry attempt. Production adapters schedule this
    /// method after the delay requested through `CaptureWindowPort`; tests may
    /// drive it directly without sleeping.
    pub fn retry_pending_window_actions(
        &self,
        windows: &dyn CaptureWindowPort,
    ) -> Result<(), CaptureError> {
        let needs_overlay = {
            let inner = self.lock()?;
            let overlay_exists = inner
                .current_overlay_generation
                .is_some_and(|generation| windows.window_exists(&overlay_window_label(generation)));
            inner
                .pending_window_actions
                .as_ref()
                .is_some_and(|pending| {
                    matches!(pending.overlay, Some(PendingOverlayAction::Rebuild))
                        && !overlay_exists
                        && pending.next_retry > 0
                })
        };
        if needs_overlay {
            // Creation is performed before retrying the serialized hide/restore
            // actions so WebView lifecycle callbacks can re-enter capture state.
            let _ = self.ensure_overlay(windows, true);
        }
        self.lock()?.retry_pending_window_actions(windows)
    }

    /// Fails closed only when the timer still identifies the exact active
    /// session/generation and the timeout applies to its current phase. Stale
    /// watchdogs are expected after normal completion and are harmless.
    pub fn expire_session(
        &self,
        session_id: &str,
        overlay_generation: u64,
        kind: CaptureTimeoutKind,
        windows: &dyn CaptureWindowPort,
    ) -> Result<bool, CaptureError> {
        let mut inner = self.lock()?;
        let Some(state) = inner.coordinator.active_state() else {
            return Ok(false);
        };
        if state.session_id != session_id
            || state.overlay_generation != overlay_generation
            || !kind.applies_to(state.phase)
        {
            return Ok(false);
        }
        let cleanup = inner.coordinator.fail(
            session_id,
            CaptureError::new(
                CaptureErrorCode::TimedOut,
                "screen capture session timed out",
            ),
        )?;
        inner.complete_cleanup(&cleanup, windows)?;
        Ok(true)
    }

    /// Reserve a composer capture and, when the overlay ready barrier has
    /// already passed, transition it to `Capturing` before returning. Native
    /// acquisition must be awaited only after the caller schedules watchdogs.
    pub fn reserve_composer_capture(
        &self,
        caller_window_label: &str,
        session_id: String,
        target: CaptureTarget,
        windows: &dyn CaptureWindowPort,
    ) -> Result<CaptureReservation, CaptureError> {
        let _start = self.lock_capture_start()?;
        if !is_regular_window_label(caller_window_label) {
            return Err(unauthorized_error(
                "only a regular application window may start composer capture",
            ));
        }
        if !is_regular_window_label(&target.window_label) {
            return Err(CaptureError::new(
                CaptureErrorCode::TargetUnavailable,
                "capture delivery target must be a regular application window",
            ));
        }
        if !windows.window_exists(&target.window_label) {
            return Err(CaptureError::new(
                CaptureErrorCode::TargetUnavailable,
                "capture delivery target window is unavailable",
            ));
        }

        let overlay_generation = self
            .prepare_overlay_for_capture(windows)?
            .overlay_generation;
        let overlay_label = overlay_window_label(overlay_generation);
        if !windows.window_exists(caller_window_label)
            || !windows.window_exists(&target.window_label)
        {
            return Err(CaptureError::new(
                CaptureErrorCode::TargetUnavailable,
                "capture origin or delivery target closed while preparing the overlay",
            ));
        }
        let mut inner = self.lock()?;
        let request = CaptureRequest {
            session_id: session_id.clone(),
            trigger: CaptureTriggerKind::Composer,
            origin: Some(CaptureOrigin {
                window_label: caller_window_label.to_string(),
            }),
            target: Some(target),
        };
        let state = match inner.coordinator.start_from_window(
            caller_window_label,
            request,
            overlay_generation,
        ) {
            Ok(state) => state,
            Err(error) => {
                if error.code == CaptureErrorCode::Busy {
                    let _ = windows.focus_window(&overlay_label);
                }
                return Err(error);
            }
        };
        inner.invalidated_target = None;
        let ticket = if state.phase == NativeCapturePhase::HidingOrigin {
            Some(inner.begin_capture_ticket(&session_id, windows)?)
        } else {
            None
        };
        Ok(CaptureReservation {
            response: CaptureStartResponse {
                session_id,
                overlay_generation,
                phase: inner.coordinator.phase(),
            },
            ticket,
        })
    }

    #[cfg(test)]
    pub fn start_composer(
        &self,
        caller_window_label: &str,
        session_id: String,
        target: CaptureTarget,
        windows: &dyn CaptureWindowPort,
        backend: &dyn ScreenCaptureBackend,
    ) -> Result<CaptureStartResponse, CaptureError> {
        let _start = self.lock_capture_start()?;
        if !is_regular_window_label(caller_window_label) {
            return Err(unauthorized_error(
                "only a regular application window may start composer capture",
            ));
        }
        if !is_regular_window_label(&target.window_label) {
            return Err(CaptureError::new(
                CaptureErrorCode::TargetUnavailable,
                "capture delivery target must be a regular application window",
            ));
        }
        if !windows.window_exists(&target.window_label) {
            return Err(CaptureError::new(
                CaptureErrorCode::TargetUnavailable,
                "capture delivery target window is unavailable",
            ));
        }
        let overlay_generation = self
            .prepare_overlay_for_capture(windows)?
            .overlay_generation;
        let overlay_label = overlay_window_label(overlay_generation);
        if !windows.window_exists(caller_window_label)
            || !windows.window_exists(&target.window_label)
        {
            return Err(CaptureError::new(
                CaptureErrorCode::TargetUnavailable,
                "capture origin or delivery target closed while preparing the overlay",
            ));
        }
        let mut inner = self.lock()?;
        let request = CaptureRequest {
            session_id: session_id.clone(),
            trigger: CaptureTriggerKind::Composer,
            origin: Some(CaptureOrigin {
                window_label: caller_window_label.to_string(),
            }),
            target: Some(target),
        };
        let state = match inner.coordinator.start_from_window(
            caller_window_label,
            request,
            overlay_generation,
        ) {
            Ok(state) => state,
            Err(error) => {
                if error.code == CaptureErrorCode::Busy {
                    // A repeat shortcut/click must signal the existing capture without
                    // replacing its frozen origin, target, or native buffers.
                    let _ = windows.focus_window(&overlay_label);
                }
                return Err(error);
            }
        };
        inner.invalidated_target = None;
        if state.phase == NativeCapturePhase::HidingOrigin {
            inner.capture_and_publish(&session_id, windows, backend)?;
        }
        Ok(CaptureStartResponse {
            session_id,
            overlay_generation,
            phase: inner.coordinator.phase(),
        })
    }

    /// Registers the active chat target owned by a regular application
    /// webview. The token is opaque native authority; recipient identifiers
    /// remain entirely in the registering webview.
    pub fn register_eligible_target(
        &self,
        caller_window_label: &str,
        target_token: &str,
    ) -> Result<(), CaptureError> {
        validate_target_registration(caller_window_label, target_token)?;
        let mut inner = self.lock()?;
        inner
            .eligible_targets
            .retain(|target| target.window_label != caller_window_label);
        inner.eligible_targets.push(CaptureTarget {
            window_label: caller_window_label.to_string(),
            target_token: target_token.to_string(),
        });
        Ok(())
    }

    /// Removes only the exact authority being deactivated. A delayed cleanup
    /// from a cached chat must never erase a newer token for the same webview.
    pub fn unregister_eligible_target(
        &self,
        caller_window_label: &str,
        target_token: &str,
    ) -> Result<(), CaptureError> {
        validate_target_registration(caller_window_label, target_token)?;
        self.lock()?.eligible_targets.retain(|target| {
            target.window_label != caller_window_label || target.target_token != target_token
        });
        Ok(())
    }

    /// Trusted native shortcut entry point. Target pruning, selection, session
    /// reservation, and the transition to `Capturing` are one serialized
    /// operation. The slow backend is deliberately not called under this lock.
    pub fn reserve_global_with_registered_target_capture(
        &self,
        session_id: String,
        origin: Option<CaptureOrigin>,
        windows: &dyn CaptureWindowPort,
    ) -> Result<CaptureReservation, CaptureError> {
        let _start = self.lock_capture_start()?;
        let overlay_generation = self
            .prepare_overlay_for_capture(windows)?
            .overlay_generation;
        let (reservation, session_started_target) = {
            let mut inner = self.lock()?;
            inner.eligible_targets.retain(|target| {
                is_regular_window_label(&target.window_label)
                    && windows.window_exists(&target.window_label)
            });
            let target = origin
                .as_ref()
                .and_then(|origin| {
                    inner
                        .eligible_targets
                        .iter()
                        .rev()
                        .find(|target| target.window_label == origin.window_label)
                })
                .or_else(|| inner.eligible_targets.last())
                .cloned();
            let reservation = inner.reserve_global(
                session_id,
                origin,
                target.clone(),
                overlay_generation,
                windows,
            )?;
            (reservation, target)
        };
        self.publish_global_session_started(
            &reservation,
            session_started_target.as_ref(),
            windows,
        )?;
        Ok(reservation)
    }

    fn publish_global_session_started(
        &self,
        reservation: &CaptureReservation,
        target: Option<&CaptureTarget>,
        windows: &dyn CaptureWindowPort,
    ) -> Result<(), CaptureError> {
        let Some(target) = target else {
            return Ok(());
        };
        let payload = SessionStartedPayload {
            session_id: reservation.response.session_id.clone(),
            target_token: target.target_token.clone(),
        };
        if let Err(error) = windows.emit_session_started(&target.window_label, &payload) {
            let mut inner = self.lock()?;
            if inner.coordinator.active_state().is_some_and(|state| {
                state.session_id == reservation.response.session_id
                    && state.overlay_generation == reservation.response.overlay_generation
            }) {
                inner.abort_session(&reservation.response.session_id, error.clone(), windows)?;
            }
            return Err(error);
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn start_global(
        &self,
        session_id: String,
        origin: Option<CaptureOrigin>,
        target: Option<CaptureTarget>,
        windows: &dyn CaptureWindowPort,
        backend: &dyn ScreenCaptureBackend,
    ) -> Result<CaptureStartResponse, CaptureError> {
        let _start = self.lock_capture_start()?;
        if target.as_ref().is_some_and(|candidate| {
            !is_regular_window_label(&candidate.window_label)
                || !windows.window_exists(&candidate.window_label)
        }) {
            return Err(CaptureError::new(
                CaptureErrorCode::TargetUnavailable,
                "capture delivery target window is unavailable",
            ));
        }
        let overlay_generation = self
            .prepare_overlay_for_capture(windows)?
            .overlay_generation;
        self.lock()?.start_global(
            session_id,
            origin,
            target,
            overlay_generation,
            windows,
            backend,
        )
    }

    /// Trusted native shortcut entry point. Target pruning, selection, and
    /// coordinator start occur under one runtime lock, so deactivation cannot
    /// redirect an in-flight capture between snapshot and start.
    #[cfg(test)]
    pub fn start_global_with_registered_target(
        &self,
        session_id: String,
        origin: Option<CaptureOrigin>,
        windows: &dyn CaptureWindowPort,
        backend: &dyn ScreenCaptureBackend,
    ) -> Result<CaptureStartResponse, CaptureError> {
        let _start = self.lock_capture_start()?;
        let overlay_generation = self
            .prepare_overlay_for_capture(windows)?
            .overlay_generation;
        let mut inner = self.lock()?;
        inner.eligible_targets.retain(|target| {
            is_regular_window_label(&target.window_label)
                && windows.window_exists(&target.window_label)
        });
        let target = origin
            .as_ref()
            .and_then(|origin| {
                inner
                    .eligible_targets
                    .iter()
                    .rev()
                    .find(|target| target.window_label == origin.window_label)
            })
            .or_else(|| inner.eligible_targets.last())
            .cloned();
        inner.start_global(
            session_id,
            origin,
            target,
            overlay_generation,
            windows,
            backend,
        )
    }

    pub fn mark_overlay_ready(
        &self,
        caller_window_label: &str,
        overlay_generation: u64,
        protocol_version: u32,
        windows: &dyn CaptureWindowPort,
    ) -> Result<(NativeCapturePhase, Option<CaptureTicket>), CaptureError> {
        if protocol_version != CAPTURE_PROTOCOL_VERSION {
            return Err(CaptureError::new(
                CaptureErrorCode::OverlayFailed,
                "capture overlay protocol version is unsupported",
            ));
        }
        let mut inner = self.lock()?;
        inner.require_current_overlay(caller_window_label, overlay_generation)?;
        let phase = inner
            .coordinator
            .note_overlay_ready(caller_window_label, overlay_generation)?;
        let ticket = if phase == NativeCapturePhase::HidingOrigin {
            let session_id = inner
                .coordinator
                .active_session_id()
                .ok_or_else(no_active_session_error)?
                .to_string();
            Some(inner.begin_capture_ticket(&session_id, windows)?)
        } else {
            None
        };
        Ok((inner.coordinator.phase(), ticket))
    }

    #[cfg(test)]
    pub fn overlay_ready(
        &self,
        caller_window_label: &str,
        overlay_generation: u64,
        protocol_version: u32,
        windows: &dyn CaptureWindowPort,
        backend: &dyn ScreenCaptureBackend,
    ) -> Result<NativeCapturePhase, CaptureError> {
        if protocol_version != CAPTURE_PROTOCOL_VERSION {
            return Err(CaptureError::new(
                CaptureErrorCode::OverlayFailed,
                "capture overlay protocol version is unsupported",
            ));
        }
        let mut inner = self.lock()?;
        inner.require_current_overlay(caller_window_label, overlay_generation)?;
        let phase = inner
            .coordinator
            .note_overlay_ready(caller_window_label, overlay_generation)?;
        if phase == NativeCapturePhase::HidingOrigin {
            let session_id = inner
                .coordinator
                .active_session_id()
                .ok_or_else(no_active_session_error)?
                .to_string();
            inner.capture_and_publish(&session_id, windows, backend)?;
        }
        Ok(inner.coordinator.phase())
    }

    /// Publish an acquired frame only if the exact reservation is still the
    /// active `Capturing` session. Cancellation, timeout, overlay replacement,
    /// or a newer session makes the result stale; ownership is then dropped
    /// here without ever exposing its pixels.
    pub fn complete_capture(
        &self,
        ticket: &CaptureTicket,
        result: Result<CapturedFrame, CaptureError>,
        windows: &dyn CaptureWindowPort,
    ) -> Result<CapturePublishOutcome, CaptureError> {
        let mut inner = self.lock()?;
        let Some(state) = inner.coordinator.active_state() else {
            return Ok(CapturePublishOutcome::Stale);
        };
        if state.session_id != ticket.session_id
            || state.overlay_generation != ticket.overlay_generation
            || state.phase != NativeCapturePhase::Capturing
        {
            return Ok(CapturePublishOutcome::Stale);
        }

        let frame = match result {
            Ok(frame) => frame,
            Err(error) => {
                inner.abort_session(&ticket.session_id, error.clone(), windows)?;
                return Err(error);
            }
        };
        let descriptor = frame.descriptor().clone();
        let overlay_label = overlay_window_label(ticket.overlay_generation);
        if let Err(error) = inner.coordinator.store_frame(&ticket.session_id, frame) {
            inner.abort_session(&ticket.session_id, error.clone(), windows)?;
            return Err(error);
        }
        if windows.defers_overlay_position_until_presented() {
            inner.pending_overlay_monitor = Some(descriptor.monitor.clone());
        } else if let Err(error) = windows.position_overlay(&overlay_label, &descriptor.monitor) {
            inner.abort_session(&ticket.session_id, error.clone(), windows)?;
            return Err(error);
        }
        if let Err(error) =
            windows.wake_overlay_for_frame_delivery(&overlay_label, &descriptor.monitor)
        {
            inner.abort_session(&ticket.session_id, error.clone(), windows)?;
            return Err(error);
        }
        let payload = FrameAvailablePayload {
            session_id: ticket.session_id.clone(),
            overlay_generation: ticket.overlay_generation,
            descriptor,
            can_confirm: state.target.as_ref().is_some_and(|target| {
                is_regular_window_label(&target.window_label)
                    && windows.window_exists(&target.window_label)
            }),
        };
        if let Err(error) = windows.emit_frame_available(&overlay_label, &payload) {
            inner.abort_session(&ticket.session_id, error.clone(), windows)?;
            return Err(error);
        }
        Ok(CapturePublishOutcome::Published)
    }

    /// Return pending frame metadata without transferring pixel ownership.
    /// Only the current overlay generation can discover the native session.
    pub fn pending_frame(
        &self,
        caller_window_label: &str,
        overlay_generation: u64,
        windows: &dyn CaptureWindowPort,
    ) -> Result<Option<FrameAvailablePayload>, CaptureError> {
        let pending = {
            let inner = self.lock()?;
            inner.require_current_overlay(caller_window_label, overlay_generation)?;
            inner
                .coordinator
                .pending_frame(caller_window_label, overlay_generation)?
        };
        let Some((state, descriptor)) = pending else {
            return Ok(None);
        };
        // Never consult Tauri's window registry while the capture-state mutex
        // is held. On WebView2 this command runs on the IPC/UI path; framework
        // window lookup can re-enter code that needs the runtime and wedge all
        // subsequent IPC once a pending frame exists.
        let can_confirm = state.target.as_ref().is_some_and(|target| {
            is_regular_window_label(&target.window_label)
                && windows.window_exists(&target.window_label)
        });
        Ok(Some(FrameAvailablePayload {
            session_id: state.session_id,
            overlay_generation,
            descriptor,
            can_confirm,
        }))
    }

    fn capture_ticket_is_active(&self, ticket: &CaptureTicket) -> Result<bool, CaptureError> {
        Ok(self
            .lock()?
            .coordinator
            .active_state()
            .is_some_and(|state| {
                state.session_id == ticket.session_id
                    && state.overlay_generation == ticket.overlay_generation
                    && state.phase == NativeCapturePhase::Capturing
            }))
    }

    pub fn take_frame(
        &self,
        caller_window_label: &str,
        session_id: &str,
        overlay_generation: u64,
    ) -> Result<Vec<u8>, CaptureError> {
        let mut inner = self.lock()?;
        inner.require_overlay_session(caller_window_label, session_id, overlay_generation)?;
        let (_, bytes) =
            inner
                .coordinator
                .take_frame(caller_window_label, session_id, overlay_generation)?;
        Ok(bytes)
    }

    pub fn frame_presented(
        &self,
        caller_window_label: &str,
        session_id: &str,
        overlay_generation: u64,
        windows: &dyn CaptureWindowPort,
    ) -> Result<(), CaptureError> {
        let mut inner = self.lock()?;
        inner.require_overlay_session(caller_window_label, session_id, overlay_generation)?;
        inner
            .coordinator
            .frame_presented(caller_window_label, session_id, overlay_generation)?;
        let overlay_label = overlay_window_label(overlay_generation);
        if let Some(monitor) = inner.pending_overlay_monitor.clone() {
            if let Err(error) = windows.position_overlay(&overlay_label, &monitor) {
                inner.abort_session(session_id, error.clone(), windows)?;
                return Err(error);
            }
            inner.pending_overlay_monitor = None;
        }
        if let Err(error) = windows.show_overlay(&overlay_label) {
            inner.abort_session(session_id, error.clone(), windows)?;
            return Err(error);
        }
        Ok(())
    }

    pub fn store_result(
        &self,
        caller_window_label: &str,
        session_id: &str,
        overlay_generation: u64,
        submission: CaptureResultSubmission,
    ) -> Result<CaptureResultDescriptor, CaptureError> {
        let descriptor = CaptureResultDescriptor {
            session_id: session_id.to_string(),
            result_id: submission.result_id,
            width: submission.width,
            height: submission.height,
            filename: submission.filename,
            mime_type: "image/png".to_string(),
            byte_len: submission.bytes.len(),
        };
        // PNG decoding can be expensive for the maximum accepted frame. Keep
        // cancellation, window teardown, and watchdogs responsive while it
        // runs, then revalidate session authority before committing the bytes.
        validate_result_payload(&descriptor, &submission.bytes)?;
        let mut inner = self.lock()?;
        inner.require_overlay_session(caller_window_label, session_id, overlay_generation)?;
        inner.coordinator.store_prevalidated_result(
            caller_window_label,
            session_id,
            overlay_generation,
            descriptor.clone(),
            submission.bytes,
        )?;
        Ok(descriptor)
    }

    pub fn publish_result(
        &self,
        caller_window_label: &str,
        session_id: &str,
        result_id: &str,
        overlay_generation: u64,
        windows: &dyn CaptureWindowPort,
    ) -> Result<(), CaptureError> {
        let mut inner = self.lock()?;
        inner.require_no_active_export()?;
        inner.require_overlay_session(caller_window_label, session_id, overlay_generation)?;
        let (target, descriptor) = inner.coordinator.result_delivery_metadata(
            caller_window_label,
            session_id,
            overlay_generation,
            result_id,
        )?;
        if !is_regular_window_label(&target.window_label)
            || !windows.window_exists(&target.window_label)
        {
            inner.invalidate_frozen_target(
                &target.window_label,
                session_id,
                &target.target_token,
                windows,
                true,
            )?;
            return Err(CaptureError::new(
                CaptureErrorCode::TargetUnavailable,
                "capture delivery target window is unavailable",
            ));
        }
        windows.emit_result_available(
            &target.window_label,
            &ResultAvailablePayload {
                target_token: target.target_token,
                descriptor,
            },
        )
    }

    pub fn take_result(
        &self,
        caller_window_label: &str,
        session_id: &str,
        result_id: &str,
        target_token: &str,
        lease_id: String,
    ) -> Result<Vec<u8>, CaptureError> {
        let mut inner = self.lock()?;
        inner.require_no_active_export()?;
        if inner.active_delivery_lease.is_some() {
            return Err(CaptureError::new(
                CaptureErrorCode::Busy,
                "capture result already has an in-flight delivery",
            ));
        }
        let (_, bytes) = inner.coordinator.lease_result(
            caller_window_label,
            target_token,
            session_id,
            result_id,
            &lease_id,
        )?;
        inner.active_delivery_lease = Some(DeliveryLease {
            session_id: session_id.to_string(),
            result_id: result_id.to_string(),
            target_window_label: caller_window_label.to_string(),
            target_token: target_token.to_string(),
            lease_id,
        });
        Ok(bytes)
    }

    pub fn release_result(
        &self,
        caller_window_label: &str,
        session_id: &str,
        result_id: &str,
        target_token: &str,
        windows: &dyn CaptureWindowPort,
    ) -> Result<(), CaptureError> {
        let mut inner = self.lock()?;
        let overlay_generation = inner
            .coordinator
            .active_state()
            .filter(|state| state.session_id == session_id)
            .map(|state| state.overlay_generation)
            .ok_or_else(no_active_session_error)?;
        if inner.active_delivery_lease.is_some() {
            let lease = inner.require_delivery_lease(
                caller_window_label,
                session_id,
                result_id,
                target_token,
            )?;
            inner.coordinator.release_result_lease(
                caller_window_label,
                target_token,
                session_id,
                result_id,
                &lease.lease_id,
            )?;
            inner.active_delivery_lease = None;
        } else {
            inner.coordinator.validate_pending_result_target(
                caller_window_label,
                target_token,
                session_id,
                result_id,
            )?;
        }
        let payload = DeliveryFailedPayload {
            session_id: session_id.to_string(),
            overlay_generation,
        };
        if let Err(error) =
            windows.emit_delivery_failed(&overlay_window_label(overlay_generation), &payload)
        {
            let cleanup = inner.coordinator.fail(session_id, error.clone())?;
            inner.complete_cleanup(&cleanup, windows)?;
            return Err(error);
        }
        Ok(())
    }

    pub fn ack_result(
        &self,
        caller_window_label: &str,
        session_id: &str,
        result_id: &str,
        target_token: &str,
        windows: &dyn CaptureWindowPort,
    ) -> Result<(), CaptureError> {
        let mut inner = self.lock()?;
        if inner.completed_delivery_matches(
            caller_window_label,
            session_id,
            result_id,
            target_token,
        ) {
            return Ok(());
        }
        let lease = inner.require_delivery_lease(
            caller_window_label,
            session_id,
            result_id,
            target_token,
        )?;
        let completed = CompletedDelivery {
            session_id: lease.session_id.clone(),
            result_id: lease.result_id.clone(),
            target_window_label: lease.target_window_label.clone(),
            target_token: lease.target_token.clone(),
        };
        let cleanup = if inner.coordinator.phase() == NativeCapturePhase::Restoring {
            inner.coordinator.pending_cleanup(session_id)?
        } else {
            inner.coordinator.ack_result(
                caller_window_label,
                target_token,
                session_id,
                result_id,
                &lease.lease_id,
            )?
        };
        let cleanup_result =
            inner.complete_cleanup_with_outcome(&cleanup, SessionEndOutcome::Completed, windows);
        if inner.coordinator.phase() == NativeCapturePhase::Idle {
            // The overlay hide is deliberately the final best-effort window action.
            // If it failed after restoration and state cleanup, retain this bounded
            // marker so a lost/error response can be acknowledged idempotently.
            inner.last_completed_delivery = Some(completed);
        }
        cleanup_result
    }

    pub fn save_result(
        &self,
        caller_window_label: &str,
        session_id: &str,
        result_id: &str,
        overlay_generation: u64,
        exports: &dyn CaptureExportPort,
        windows: &dyn CaptureWindowPort,
    ) -> Result<SaveCaptureOutcome, CaptureError> {
        let (descriptor, bytes) = self.lock()?.begin_result_export(
            caller_window_label,
            session_id,
            result_id,
            overlay_generation,
        )?;
        let selected_path = match exports.choose_save_path(&descriptor.filename) {
            Ok(path) => path,
            Err(error) => {
                self.lock()?
                    .clear_result_export(session_id, result_id, overlay_generation);
                return Err(error);
            }
        };
        let Some(selected_path) = selected_path else {
            self.lock()?
                .clear_result_export(session_id, result_id, overlay_generation);
            return Ok(SaveCaptureOutcome::Cancelled);
        };
        let selected_path = match normalized_png_path(selected_path) {
            Ok(path) => path,
            Err(error) => {
                self.lock()?
                    .clear_result_export(session_id, result_id, overlay_generation);
                return Err(error);
            }
        };
        if let Err(error) = exports.write_png(&selected_path, &bytes) {
            self.lock()?
                .clear_result_export(session_id, result_id, overlay_generation);
            return Err(error);
        }
        let mut inner = self.lock()?;
        inner.require_result_export(session_id, result_id, overlay_generation)?;
        let cleanup = inner.coordinator.complete_result_export(
            caller_window_label,
            session_id,
            overlay_generation,
            result_id,
        )?;
        inner.complete_cleanup_with_outcome(&cleanup, SessionEndOutcome::Saved, windows)?;
        Ok(SaveCaptureOutcome::Saved)
    }

    pub fn copy_result(
        &self,
        caller_window_label: &str,
        session_id: &str,
        result_id: &str,
        overlay_generation: u64,
        exports: &dyn CaptureExportPort,
        windows: &dyn CaptureWindowPort,
    ) -> Result<(), CaptureError> {
        let (descriptor, bytes) = self.lock()?.begin_result_export(
            caller_window_label,
            session_id,
            result_id,
            overlay_generation,
        )?;
        if let Err(error) = exports.copy_png(descriptor.width, descriptor.height, &bytes) {
            self.lock()?
                .clear_result_export(session_id, result_id, overlay_generation);
            return Err(error);
        }
        let mut inner = self.lock()?;
        inner.require_result_export(session_id, result_id, overlay_generation)?;
        let cleanup = inner.coordinator.complete_result_export(
            caller_window_label,
            session_id,
            overlay_generation,
            result_id,
        )?;
        inner.complete_cleanup_with_outcome(&cleanup, SessionEndOutcome::Copied, windows)
    }

    pub fn discard_result(
        &self,
        caller_window_label: &str,
        session_id: &str,
        result_id: &str,
        overlay_generation: u64,
    ) -> Result<NativeCapturePhase, CaptureError> {
        let mut inner = self.lock()?;
        inner.require_no_active_export()?;
        inner.require_overlay_session(caller_window_label, session_id, overlay_generation)?;
        inner.coordinator.discard_result(
            caller_window_label,
            session_id,
            overlay_generation,
            result_id,
        )?;
        Ok(inner.coordinator.phase())
    }

    pub fn invalidate_target_from_window(
        &self,
        caller_window_label: &str,
        session_id: &str,
        target_token: &str,
        windows: &dyn CaptureWindowPort,
    ) -> Result<(), CaptureError> {
        let mut inner = self.lock()?;
        inner.invalidate_frozen_target(
            caller_window_label,
            session_id,
            target_token,
            windows,
            false,
        )
    }

    pub fn fail_from_overlay(
        &self,
        caller_window_label: &str,
        session_id: &str,
        overlay_generation: u64,
        code: &str,
        detail: &str,
        windows: &dyn CaptureWindowPort,
    ) -> Result<(), CaptureError> {
        let failure = FrontendFailureCode::parse(code)?;
        let mut inner = self.lock()?;
        inner.require_overlay_session(caller_window_label, session_id, overlay_generation)?;
        let cleanup = inner
            .coordinator
            .fail(session_id, failure.native_error(detail))?;
        inner.complete_cleanup(&cleanup, windows)
    }

    pub fn cancel_from_window(
        &self,
        caller_window_label: &str,
        session_id: &str,
        overlay_generation: Option<u64>,
        windows: &dyn CaptureWindowPort,
    ) -> Result<(), CaptureError> {
        let mut inner = self.lock()?;
        if is_overlay_window_label(caller_window_label) {
            let overlay_generation = overlay_generation.ok_or_else(|| {
                CaptureError::new(
                    CaptureErrorCode::InvalidSession,
                    "capture overlay generation is required",
                )
            })?;
            inner.require_overlay_session(caller_window_label, session_id, overlay_generation)?;
        }
        let cleanup = inner
            .coordinator
            .cancel_from_window(caller_window_label, session_id)?;
        inner.complete_cleanup(&cleanup, windows)
    }

    pub fn overlay_unavailable(
        &self,
        caller_window_label: &str,
        overlay_generation: u64,
        windows: &dyn CaptureWindowPort,
    ) -> Result<OverlayInit, CaptureError> {
        let mut inner = self.lock()?;
        inner.require_current_overlay(caller_window_label, overlay_generation)?;
        if let Some(cleanup) = inner
            .coordinator
            .note_overlay_unavailable(caller_window_label, overlay_generation)?
        {
            inner.complete_cleanup(&cleanup, windows)?;
        }
        // `pagehide` invokes this command from the overlay itself. Do not destroy
        // or replace an in-flight command's webview: its next page can re-register
        // a listener and rearm this same generation through `overlay_ready`.
        Ok(OverlayInit { overlay_generation })
    }

    /// A document that failed during bootstrap cannot rearm itself. Clear any
    /// active session, retire this exact generation without an eager rebuild,
    /// and let the next explicit capture create a fresh overlay.
    pub fn overlay_bootstrap_failed(
        &self,
        caller_window_label: &str,
        overlay_generation: u64,
        windows: &dyn CaptureWindowPort,
    ) -> Result<OverlayInit, CaptureError> {
        let mut inner = self.lock()?;
        inner.require_current_overlay(caller_window_label, overlay_generation)?;
        if let Some(cleanup) = inner
            .coordinator
            .note_overlay_unavailable(caller_window_label, overlay_generation)?
        {
            inner.complete_cleanup(&cleanup, windows)?;
        }
        inner.current_overlay_generation = None;
        inner.loaded_overlay_generation = None;
        inner.retiring_overlay_generation = Some(overlay_generation);
        Ok(OverlayInit { overlay_generation })
    }

    /// Release the short retirement gate after native code has attempted to
    /// destroy a bootstrap-failed overlay. Generation-qualified labels make a
    /// later replacement safe even if the platform refused that destruction:
    /// the failed WebView can neither impersonate nor receive work for the new
    /// generation.
    pub fn finish_overlay_retirement(&self, window_label: &str) -> Result<(), CaptureError> {
        let mut inner = self.lock()?;
        if inner
            .retiring_overlay_generation
            .is_some_and(|generation| window_label == overlay_window_label(generation))
        {
            inner.retiring_overlay_generation = None;
        }
        Ok(())
    }

    /// Native page-load backstop for reload/navigation. The first load of a
    /// newly created overlay is expected; a later load start invalidates the
    /// old JavaScript heap before it can strand its one-shot frame or result.
    pub fn window_page_load_started(
        &self,
        caller_window_label: &str,
        page_overlay_generation: Option<u64>,
        windows: &dyn CaptureWindowPort,
    ) -> Result<(), CaptureError> {
        if !is_overlay_window_label(caller_window_label) {
            return if is_regular_window_label(caller_window_label) {
                self.prepare_window_close(caller_window_label, windows)
            } else {
                Ok(())
            };
        }
        let mut inner = self.lock()?;
        let Some(overlay_generation) = inner.current_overlay_generation else {
            return Ok(());
        };
        if caller_window_label != overlay_window_label(overlay_generation) {
            return Ok(());
        }
        if page_overlay_generation.is_some() && page_overlay_generation != Some(overlay_generation)
        {
            return Ok(());
        }
        if inner.loaded_overlay_generation != Some(overlay_generation) {
            return Ok(());
        }
        inner.loaded_overlay_generation = None;
        if let Some(cleanup) = inner
            .coordinator
            .note_overlay_unavailable(caller_window_label, overlay_generation)?
        {
            inner.complete_cleanup(&cleanup, windows)?;
        }
        Ok(())
    }

    pub fn overlay_page_load_finished(
        &self,
        caller_window_label: &str,
        page_overlay_generation: Option<u64>,
        windows: &dyn CaptureWindowPort,
    ) -> Result<(), CaptureError> {
        if !is_overlay_window_label(caller_window_label) {
            return Ok(());
        }
        let mut inner = self.lock()?;
        let Some(overlay_generation) = inner.current_overlay_generation else {
            return Ok(());
        };
        if caller_window_label != overlay_window_label(overlay_generation) {
            return Ok(());
        }
        if page_overlay_generation != Some(overlay_generation) {
            return Ok(());
        }
        if windows.window_exists(caller_window_label) {
            inner.loaded_overlay_generation = Some(overlay_generation);
        }
        Ok(())
    }

    /// Native close-request hook. The application must call this synchronously
    /// before allowing the overlay to be destroyed. Generation rotation remains
    /// in `overlay_destroyed`, after Tauri confirms destruction.
    pub fn prepare_window_close(
        &self,
        window_label: &str,
        windows: &dyn CaptureWindowPort,
    ) -> Result<(), CaptureError> {
        let mut inner = self.lock()?;
        inner
            .eligible_targets
            .retain(|target| target.window_label != window_label);
        if is_overlay_window_label(window_label) {
            if let Some(overlay_generation) = inner.current_overlay_generation
                && window_label == overlay_window_label(overlay_generation)
                && let Some(cleanup) = inner
                    .coordinator
                    .note_overlay_unavailable(window_label, overlay_generation)?
            {
                inner.complete_cleanup(&cleanup, windows)?;
            }
            return Ok(());
        }

        let Some(state) = inner.coordinator.active_state() else {
            return Ok(());
        };
        let is_origin = state
            .origin
            .as_ref()
            .is_some_and(|origin| origin.window_label == window_label);
        if is_origin {
            let cleanup = inner
                .coordinator
                .cancel_from_window(window_label, &state.session_id)?;
            inner.complete_cleanup(&cleanup, windows)?;
            return Ok(());
        }

        let is_target = state
            .target
            .as_ref()
            .is_some_and(|target| target.window_label == window_label);
        if is_target {
            let target_token = state
                .target
                .as_ref()
                .map(|target| target.target_token.clone())
                .ok_or_else(no_active_session_error)?;
            // Native destruction cannot retry a failed webview event, but target
            // invalidation and lease release must still complete synchronously.
            let _ = inner.invalidate_frozen_target(
                window_label,
                &state.session_id,
                &target_token,
                windows,
                true,
            );
        }
        Ok(())
    }

    /// Native lifecycle hook used when Tauri reports that the overlay has
    /// already been destroyed. It clears pixels and restores the origin but
    /// leaves overlay recreation to a later event-loop tick.
    pub fn overlay_destroyed(
        &self,
        window_label: &str,
        windows: &dyn CaptureWindowPort,
    ) -> Result<bool, CaptureError> {
        let mut inner = self.lock()?;
        if inner
            .retiring_overlay_generation
            .is_some_and(|generation| window_label == overlay_window_label(generation))
        {
            log::info!("screen capture overlay destruction acknowledged label={window_label}");
            inner.retiring_overlay_generation = None;
            return Ok(false);
        }
        let Some(overlay_generation) = inner.current_overlay_generation else {
            return Ok(false);
        };
        let overlay_label = overlay_window_label(overlay_generation);
        if window_label != overlay_label || windows.window_exists(&overlay_label) {
            // A replacement may already exist by the time the old window's
            // Destroyed event is delivered.
            return Ok(false);
        }
        if let Some(cleanup) = inner
            .coordinator
            .note_overlay_unavailable(&overlay_label, overlay_generation)?
        {
            inner.complete_cleanup(&cleanup, windows)?;
        }
        inner.current_overlay_generation = None;
        inner.loaded_overlay_generation = None;
        Ok(!windows.uses_ephemeral_overlay())
    }

    pub fn ensure_overlay_native(
        &self,
        windows: &dyn CaptureWindowPort,
    ) -> Result<OverlayInit, CaptureError> {
        self.ensure_overlay(windows, false)
    }

    fn prepare_overlay_for_capture(
        &self,
        windows: &dyn CaptureWindowPort,
    ) -> Result<OverlayInit, CaptureError> {
        self.ensure_overlay_native(windows)
    }

    fn ensure_overlay(
        &self,
        windows: &dyn CaptureWindowPort,
        allow_pending_cleanup: bool,
    ) -> Result<OverlayInit, CaptureError> {
        // Webview creation can synchronously run Tauri page-load callbacks on
        // Windows. Those callbacks re-enter this runtime, so the mutable state
        // lock must never cross `create_overlay`. A second, narrow gate keeps
        // concurrent triggers from allocating two overlay generations.
        let _creation = self.overlay_creation.lock().map_err(|_| {
            CaptureError::new(
                CaptureErrorCode::OverlayFailed,
                "capture overlay creation gate is unavailable",
            )
        })?;
        let (init, spec) = match self
            .lock()?
            .prepare_overlay(windows, allow_pending_cleanup)?
        {
            OverlayPreparation::Existing(init) => return Ok(init),
            OverlayPreparation::Create { init, spec } => (init, spec),
        };

        match windows.create_overlay(&spec) {
            Ok(()) => Ok(init),
            Err(error) if windows.window_exists(&spec.label) => {
                // Some platform adapters can report a late initialization error
                // after registering the window. Keep the tracked generation so
                // its lifecycle callback can either ready or invalidate it.
                log::warn!(
                    "screen capture overlay creation reported an error after the window appeared: {error}"
                );
                Ok(init)
            }
            Err(error) => {
                let mut inner = self.lock()?;
                if inner.current_overlay_generation == Some(init.overlay_generation) {
                    inner.current_overlay_generation = None;
                    inner.loaded_overlay_generation = None;
                }
                Err(error)
            }
        }
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, RuntimeInner>, CaptureError> {
        self.inner.lock().map_err(|_| {
            CaptureError::new(
                CaptureErrorCode::CaptureFailed,
                "capture runtime state is unavailable",
            )
        })
    }

    fn lock_capture_start(&self) -> Result<std::sync::MutexGuard<'_, ()>, CaptureError> {
        match self.capture_start.try_lock() {
            Ok(guard) => Ok(guard),
            Err(TryLockError::WouldBlock) => Err(CaptureError::new(
                CaptureErrorCode::Busy,
                "another capture start is already in progress",
            )),
            Err(TryLockError::Poisoned(_)) => Err(CaptureError::new(
                CaptureErrorCode::CaptureFailed,
                "capture start gate is unavailable",
            )),
        }
    }
}

impl Default for ScreenCaptureRuntime {
    fn default() -> Self {
        Self::new().expect("the fixed capture overlay label is valid")
    }
}

/// Run one delayed, bounded acquisition for a previously reserved ticket.
/// The closure form is intentional: if cancellation wins during the compositor
/// settle interval, the backend is never entered. Once entered it is invoked
/// exactly once, and a result that loses a later lifecycle race is discarded by
/// `complete_capture`.
pub async fn acquire_and_publish_once<F, Fut>(
    runtime: &ScreenCaptureRuntime,
    ticket: CaptureTicket,
    windows: &dyn CaptureWindowPort,
    compositor_settle: Duration,
    acquisition_timeout: Duration,
    acquire: F,
) -> Result<CapturePublishOutcome, CaptureError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<CapturedFrame, CaptureError>>,
{
    const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(25);

    tokio::time::sleep(compositor_settle).await;
    if !runtime.capture_ticket_is_active(&ticket)? {
        return Ok(CapturePublishOutcome::Stale);
    }

    let acquisition = acquire();
    tokio::pin!(acquisition);
    let timeout = tokio::time::sleep(acquisition_timeout);
    tokio::pin!(timeout);
    let result = loop {
        tokio::select! {
            result = &mut acquisition => break result,
            _ = &mut timeout => {
                break Err(CaptureError::new(
                    CaptureErrorCode::TimedOut,
                    "screen capture backend timed out",
                ));
            }
            _ = tokio::time::sleep(CANCELLATION_POLL_INTERVAL) => {
                if !runtime.capture_ticket_is_active(&ticket)? {
                    return Ok(CapturePublishOutcome::Stale);
                }
            }
        }
    };
    runtime.complete_capture(&ticket, result, windows)
}

impl RuntimeInner {
    fn reserve_global(
        &mut self,
        session_id: String,
        origin: Option<CaptureOrigin>,
        target: Option<CaptureTarget>,
        overlay_generation: u64,
        windows: &dyn CaptureWindowPort,
    ) -> Result<CaptureReservation, CaptureError> {
        let request = CaptureRequest {
            session_id: session_id.clone(),
            trigger: CaptureTriggerKind::Global,
            origin,
            target,
        };
        let state = match self.coordinator.start_global(request, overlay_generation) {
            Ok(state) => state,
            Err(error) => {
                if error.code == CaptureErrorCode::Busy {
                    let _ = windows.focus_window(&overlay_window_label(overlay_generation));
                }
                return Err(error);
            }
        };
        self.invalidated_target = None;
        let ticket = if state.phase == NativeCapturePhase::HidingOrigin {
            Some(self.begin_capture_ticket(&session_id, windows)?)
        } else {
            None
        };
        Ok(CaptureReservation {
            response: CaptureStartResponse {
                session_id,
                overlay_generation,
                phase: self.coordinator.phase(),
            },
            ticket,
        })
    }

    fn begin_capture_ticket(
        &mut self,
        session_id: &str,
        windows: &dyn CaptureWindowPort,
    ) -> Result<CaptureTicket, CaptureError> {
        let state = self
            .coordinator
            .active_state()
            .ok_or_else(no_active_session_error)?;
        if state.session_id != session_id || state.phase != NativeCapturePhase::HidingOrigin {
            return Err(CaptureError::new(
                CaptureErrorCode::InvalidPhase,
                "capture may begin only after the overlay ready barrier",
            ));
        }

        // Keep this single synchronous hide dispatch serialized with cleanup.
        // Moving it outside the state lock would let cancellation restore the
        // origin before a racing hide completes, leaving the window hidden.
        // This critical section contains no settle sleep, portal interaction,
        // monitor capture, retry loop, or other backend work.
        let overlay_label = overlay_window_label(state.overlay_generation);
        if let Err(error) = windows.conceal_overlay_for_capture(&overlay_label) {
            self.abort_session(session_id, error.clone(), windows)?;
            return Err(error);
        }
        if let Some(origin) = state.origin.as_ref()
            && let Err(error) = self.hide_origin(session_id, &origin.window_label, windows)
        {
            self.abort_session(session_id, error.clone(), windows)?;
            return Err(error);
        }
        if let Err(error) = self.coordinator.begin_capture(session_id) {
            self.abort_session(session_id, error.clone(), windows)?;
            return Err(error);
        }
        Ok(CaptureTicket {
            session_id: session_id.to_string(),
            overlay_generation: state.overlay_generation,
        })
    }

    #[cfg(test)]
    fn start_global(
        &mut self,
        session_id: String,
        origin: Option<CaptureOrigin>,
        target: Option<CaptureTarget>,
        overlay_generation: u64,
        windows: &dyn CaptureWindowPort,
        backend: &dyn ScreenCaptureBackend,
    ) -> Result<CaptureStartResponse, CaptureError> {
        let request = CaptureRequest {
            session_id: session_id.clone(),
            trigger: CaptureTriggerKind::Global,
            origin,
            target,
        };
        let state = match self.coordinator.start_global(request, overlay_generation) {
            Ok(state) => state,
            Err(error) => {
                if error.code == CaptureErrorCode::Busy {
                    let _ = windows.focus_window(&overlay_window_label(overlay_generation));
                }
                return Err(error);
            }
        };
        self.invalidated_target = None;
        if let Some(target) = state.target.as_ref() {
            let payload = SessionStartedPayload {
                session_id: session_id.clone(),
                target_token: target.target_token.clone(),
            };
            if let Err(error) = windows.emit_session_started(&target.window_label, &payload) {
                self.abort_session(&session_id, error.clone(), windows)?;
                return Err(error);
            }
        }
        if state.phase == NativeCapturePhase::HidingOrigin {
            self.capture_and_publish(&session_id, windows, backend)?;
        }
        Ok(CaptureStartResponse {
            session_id,
            overlay_generation,
            phase: self.coordinator.phase(),
        })
    }

    fn require_no_active_export(&self) -> Result<(), CaptureError> {
        if self.active_export.is_some() {
            return Err(CaptureError::new(
                CaptureErrorCode::Busy,
                "a capture result export is already in progress",
            ));
        }
        Ok(())
    }

    fn begin_result_export(
        &mut self,
        caller_window_label: &str,
        session_id: &str,
        result_id: &str,
        overlay_generation: u64,
    ) -> Result<(CaptureResultDescriptor, Arc<[u8]>), CaptureError> {
        self.require_no_active_export()?;
        let snapshot = self.coordinator.snapshot_result(
            caller_window_label,
            session_id,
            overlay_generation,
            result_id,
        )?;
        self.active_export = Some(ResultExportLease {
            session_id: session_id.to_string(),
            result_id: result_id.to_string(),
            overlay_generation,
        });
        Ok(snapshot)
    }

    fn require_result_export(
        &self,
        session_id: &str,
        result_id: &str,
        overlay_generation: u64,
    ) -> Result<(), CaptureError> {
        if self.active_export.as_ref().is_some_and(|export| {
            export.session_id == session_id
                && export.result_id == result_id
                && export.overlay_generation == overlay_generation
        }) {
            Ok(())
        } else {
            Err(CaptureError::new(
                CaptureErrorCode::InvalidSession,
                "capture result export is stale or no longer active",
            ))
        }
    }

    fn clear_result_export(&mut self, session_id: &str, result_id: &str, overlay_generation: u64) {
        if self.active_export.as_ref().is_some_and(|export| {
            export.session_id == session_id
                && export.result_id == result_id
                && export.overlay_generation == overlay_generation
        }) {
            self.active_export = None;
        }
    }

    fn invalidate_frozen_target(
        &mut self,
        caller_window_label: &str,
        session_id: &str,
        target_token: &str,
        windows: &dyn CaptureWindowPort,
        ignore_event_error: bool,
    ) -> Result<(), CaptureError> {
        if let Some(invalidated) = self.invalidated_target.as_ref()
            && invalidated.session_id == session_id
            && invalidated.target_window_label == caller_window_label
            && invalidated.target_token == target_token
        {
            let event = windows.emit_target_unavailable(
                &overlay_window_label(invalidated.overlay_generation),
                &TargetUnavailablePayload {
                    session_id: invalidated.session_id.clone(),
                    overlay_generation: invalidated.overlay_generation,
                },
            );
            return if ignore_event_error { Ok(()) } else { event };
        }

        self.coordinator
            .authorize_target(session_id, caller_window_label, target_token)?;
        let state = self
            .coordinator
            .active_state()
            .ok_or_else(no_active_session_error)?;
        if let Some(lease) = self.active_delivery_lease.clone() {
            self.coordinator.release_result_lease(
                caller_window_label,
                target_token,
                session_id,
                &lease.result_id,
                &lease.lease_id,
            )?;
            self.active_delivery_lease = None;
        }
        self.coordinator
            .invalidate_target(session_id, caller_window_label)?;
        self.invalidated_target = Some(InvalidatedTarget {
            session_id: session_id.to_string(),
            target_window_label: caller_window_label.to_string(),
            target_token: target_token.to_string(),
            overlay_generation: state.overlay_generation,
        });
        let event = windows.emit_target_unavailable(
            &overlay_window_label(state.overlay_generation),
            &TargetUnavailablePayload {
                session_id: session_id.to_string(),
                overlay_generation: state.overlay_generation,
            },
        );
        if ignore_event_error { Ok(()) } else { event }
    }

    fn require_delivery_lease(
        &self,
        caller_window_label: &str,
        session_id: &str,
        result_id: &str,
        target_token: &str,
    ) -> Result<DeliveryLease, CaptureError> {
        let lease = self.active_delivery_lease.as_ref().ok_or_else(|| {
            CaptureError::new(
                CaptureErrorCode::InvalidSession,
                "capture result delivery lease is stale or unknown",
            )
        })?;
        if lease.target_window_label != caller_window_label
            || lease.session_id != session_id
            || lease.result_id != result_id
            || lease.target_token != target_token
        {
            return Err(unauthorized_error(
                "invoking window does not own the capture result delivery lease",
            ));
        }
        Ok(lease.clone())
    }

    fn completed_delivery_matches(
        &self,
        caller_window_label: &str,
        session_id: &str,
        result_id: &str,
        target_token: &str,
    ) -> bool {
        self.last_completed_delivery
            .as_ref()
            .is_some_and(|completed| {
                completed.target_window_label == caller_window_label
                    && completed.session_id == session_id
                    && completed.result_id == result_id
                    && completed.target_token == target_token
            })
    }

    fn prepare_overlay(
        &mut self,
        windows: &dyn CaptureWindowPort,
        allow_pending_cleanup: bool,
    ) -> Result<OverlayPreparation, CaptureError> {
        if self.pending_window_actions.is_some() && !allow_pending_cleanup {
            return Err(CaptureError::new(
                CaptureErrorCode::Busy,
                "capture window cleanup is still pending",
            ));
        }
        if self.retiring_overlay_generation.is_some() {
            return Err(CaptureError::new(
                CaptureErrorCode::Busy,
                "capture overlay destruction is still pending",
            ));
        }
        if let Some(overlay_generation) = self.current_overlay_generation {
            let overlay_label = overlay_window_label(overlay_generation);
            if windows.window_exists(&overlay_label) {
                return Ok(OverlayPreparation::Existing(OverlayInit {
                    overlay_generation,
                }));
            }
            if let Some(cleanup) = self
                .coordinator
                .note_overlay_unavailable(&overlay_label, overlay_generation)?
            {
                self.complete_cleanup(&cleanup, windows)?;
            }
            self.current_overlay_generation = None;
            self.loaded_overlay_generation = None;
        }

        let overlay_generation = self.allocate_generation()?;
        let overlay_label = overlay_window_label(overlay_generation);
        self.coordinator
            .replace_overlay_window_label(overlay_label.clone())?;
        let spec = OverlayWindowSpec {
            label: overlay_label,
            route: format!("{OVERLAY_ROUTE}?overlayGeneration={overlay_generation}"),
            visible: false,
        };
        self.current_overlay_generation = Some(overlay_generation);
        self.loaded_overlay_generation = None;
        Ok(OverlayPreparation::Create {
            init: OverlayInit { overlay_generation },
            spec,
        })
    }

    fn allocate_generation(&mut self) -> Result<u64, CaptureError> {
        let generation = self.next_overlay_generation;
        if generation == 0 || generation > MAX_JS_SAFE_INTEGER {
            return Err(CaptureError::new(
                CaptureErrorCode::OverlayFailed,
                "capture overlay generation space is exhausted",
            ));
        }
        self.next_overlay_generation = generation.checked_add(1).unwrap_or(0);
        Ok(generation)
    }

    fn require_current_overlay(
        &self,
        caller_window_label: &str,
        overlay_generation: u64,
    ) -> Result<(), CaptureError> {
        if overlay_generation == 0 || self.current_overlay_generation != Some(overlay_generation) {
            return Err(stale_generation_error());
        }
        if caller_window_label != overlay_window_label(overlay_generation) {
            return Err(unauthorized_error(
                "only the dedicated capture overlay may call this operation",
            ));
        }
        Ok(())
    }

    fn require_overlay_session(
        &self,
        caller_window_label: &str,
        session_id: &str,
        overlay_generation: u64,
    ) -> Result<(), CaptureError> {
        self.require_current_overlay(caller_window_label, overlay_generation)?;
        self.coordinator.authorize_caller(
            session_id,
            caller_window_label,
            CaptureCallerRole::Overlay,
        )?;
        let state = self
            .coordinator
            .active_state()
            .ok_or_else(no_active_session_error)?;
        if state.overlay_generation != overlay_generation {
            return Err(stale_generation_error());
        }
        Ok(())
    }

    #[cfg(test)]
    fn capture_and_publish(
        &mut self,
        session_id: &str,
        windows: &dyn CaptureWindowPort,
        backend: &dyn ScreenCaptureBackend,
    ) -> Result<(), CaptureError> {
        let state = self
            .coordinator
            .active_state()
            .ok_or_else(no_active_session_error)?;
        if state.session_id != session_id {
            return Err(no_active_session_error());
        }
        if state.phase != NativeCapturePhase::HidingOrigin {
            return Err(CaptureError::new(
                CaptureErrorCode::InvalidPhase,
                "capture may begin only after the overlay ready barrier",
            ));
        }

        let overlay_label = overlay_window_label(state.overlay_generation);
        if let Err(error) = windows.conceal_overlay_for_capture(&overlay_label) {
            self.abort_session(session_id, error.clone(), windows)?;
            return Err(error);
        }
        if let Some(origin) = state.origin.as_ref()
            && let Err(error) = self.hide_origin(session_id, &origin.window_label, windows)
        {
            self.abort_session(session_id, error.clone(), windows)?;
            return Err(error);
        }
        if let Err(error) = self.coordinator.begin_capture(session_id) {
            self.abort_session(session_id, error.clone(), windows)?;
            return Err(error);
        }

        let frame = match super::backend::capture_frame_at_cursor(backend, session_id) {
            Ok(frame) => frame,
            Err(error) => {
                self.abort_session(session_id, error.clone(), windows)?;
                return Err(error);
            }
        };
        let descriptor = frame.descriptor().clone();
        if let Err(error) = self.coordinator.store_frame(session_id, frame) {
            self.abort_session(session_id, error.clone(), windows)?;
            return Err(error);
        }
        if windows.defers_overlay_position_until_presented() {
            self.pending_overlay_monitor = Some(descriptor.monitor.clone());
        } else if let Err(error) = windows.position_overlay(&overlay_label, &descriptor.monitor) {
            self.abort_session(session_id, error.clone(), windows)?;
            return Err(error);
        }
        if let Err(error) =
            windows.wake_overlay_for_frame_delivery(&overlay_label, &descriptor.monitor)
        {
            self.abort_session(session_id, error.clone(), windows)?;
            return Err(error);
        }
        let payload = FrameAvailablePayload {
            session_id: session_id.to_string(),
            overlay_generation: state.overlay_generation,
            descriptor,
            can_confirm: state.target.as_ref().is_some_and(|target| {
                is_regular_window_label(&target.window_label)
                    && windows.window_exists(&target.window_label)
            }),
        };
        if let Err(error) = windows.emit_frame_available(&overlay_label, &payload) {
            self.abort_session(session_id, error.clone(), windows)?;
            return Err(error);
        }
        Ok(())
    }

    fn abort_session(
        &mut self,
        session_id: &str,
        error: CaptureError,
        windows: &dyn CaptureWindowPort,
    ) -> Result<(), CaptureError> {
        let cleanup = self.coordinator.fail(session_id, error)?;
        self.complete_cleanup(&cleanup, windows)
    }

    fn hide_origin(
        &mut self,
        session_id: &str,
        window_label: &str,
        windows: &dyn CaptureWindowPort,
    ) -> Result<(), CaptureError> {
        let state = windows.capture_window_state(window_label)?.ok_or_else(|| {
            CaptureError::new(
                CaptureErrorCode::TargetUnavailable,
                "capture origin disappeared before it could be hidden",
            )
        })?;
        // Record restoration authority before dispatching hide. A platform may
        // complete the hide but still report an error; cleanup must retain the
        // exact prior state in that partial-failure case.
        self.hidden_origin = Some(HiddenOrigin {
            session_id: session_id.to_string(),
            window_label: window_label.to_string(),
            state,
        });
        windows.hide_window(window_label)
    }

    fn try_window_actions(
        &mut self,
        pending: &mut PendingWindowActions,
        windows: &dyn CaptureWindowPort,
    ) -> Result<(), CaptureError> {
        let mut first_error = None;
        let overlay_generation = self.current_overlay_generation;
        let overlay_label = overlay_generation.map(overlay_window_label);
        if let Some(action) = pending.overlay {
            match action {
                PendingOverlayAction::Hide => {
                    if let Some(overlay_label) = overlay_label
                        .as_deref()
                        .filter(|label| windows.window_exists(label))
                    {
                        match windows.conceal_overlay(overlay_label) {
                            Ok(OverlayConcealment::Hidden) => pending.overlay = None,
                            Ok(OverlayConcealment::RetirementScheduled) => {
                                self.current_overlay_generation = None;
                                self.loaded_overlay_generation = None;
                                self.retiring_overlay_generation = overlay_generation;
                                pending.overlay = None;
                            }
                            Ok(OverlayConcealment::DestructionDeferred(error)) => {
                                pending.overlay = Some(PendingOverlayAction::Hide);
                                first_error = Some(error);
                            }
                            Err(error) => {
                                pending.overlay = Some(PendingOverlayAction::Hide);
                                first_error = Some(error);
                            }
                        }
                    } else {
                        if let (Some(overlay_generation), Some(overlay_label)) =
                            (overlay_generation, overlay_label.as_deref())
                        {
                            let _ = self
                                .coordinator
                                .note_overlay_unavailable(overlay_label, overlay_generation)?;
                        }
                        self.current_overlay_generation = None;
                        self.loaded_overlay_generation = None;
                        if pending.next_retry == 0 {
                            // Framework destruction owns its existing delayed
                            // rebuild hook; terminal cleanup only rotates state.
                            pending.overlay = None;
                        } else {
                            pending.overlay = Some(PendingOverlayAction::Rebuild);
                            first_error = Some(CaptureError::new(
                                CaptureErrorCode::OverlayFailed,
                                "capture overlay rebuild is pending",
                            ));
                        }
                    }
                }
                PendingOverlayAction::Rebuild => {
                    if overlay_label
                        .as_deref()
                        .is_some_and(|label| windows.window_exists(label))
                    {
                        pending.overlay = None;
                    } else {
                        first_error = Some(CaptureError::new(
                            CaptureErrorCode::OverlayFailed,
                            "capture overlay rebuild is pending",
                        ));
                    }
                }
            }
        }
        if let Some(origin) = pending.origin.as_ref() {
            match windows.restore_window(&origin.window_label, origin.state) {
                Ok(()) => pending.origin = None,
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn retain_pending_window_actions(
        &mut self,
        mut pending: PendingWindowActions,
        error: CaptureError,
        windows: &dyn CaptureWindowPort,
    ) -> Result<(), CaptureError> {
        if pending.is_empty() {
            self.pending_window_actions = None;
            return Err(error);
        }
        let delay = if let Some(delay) = WINDOW_ACTION_RETRY_DELAYS.get(pending.next_retry).copied()
        {
            pending.next_retry += 1;
            delay
        } else {
            // The overlay never owns restoration authority and may be rebuilt
            // by a later capture. The origin record is different: dropping it
            // can leave a live application window hidden forever. Keep only
            // that small record and move to a low-frequency recovery loop.
            pending.overlay = None;
            if pending
                .origin
                .as_ref()
                .is_some_and(|origin| !windows.window_exists(&origin.window_label))
            {
                pending.origin = None;
            }
            if pending.origin.is_none() {
                self.pending_window_actions = None;
                return Err(CaptureError::new(
                    CaptureErrorCode::OverlayFailed,
                    format!(
                        "capture window cleanup retry budget exhausted: {}",
                        error.detail
                    ),
                ));
            }
            pending.next_retry = WINDOW_ACTION_RETRY_DELAYS.len();
            ORIGIN_RESTORE_RECOVERY_DELAY
        };
        self.pending_window_actions = Some(pending);
        windows.defer_window_action_retry(delay);
        Err(error)
    }

    fn retry_pending_window_actions(
        &mut self,
        windows: &dyn CaptureWindowPort,
    ) -> Result<(), CaptureError> {
        let Some(mut pending) = self.pending_window_actions.take() else {
            return Ok(());
        };
        if pending
            .origin
            .as_ref()
            .is_some_and(|origin| !windows.window_exists(&origin.window_label))
        {
            pending.origin = None;
        }
        if pending.is_empty() {
            return Ok(());
        }
        match self.try_window_actions(&mut pending, windows) {
            Ok(()) if pending.is_empty() => Ok(()),
            Ok(()) => Err(CaptureError::new(
                CaptureErrorCode::OverlayFailed,
                "capture window cleanup made no progress",
            )),
            Err(error) => self.retain_pending_window_actions(pending, error, windows),
        }
    }

    fn complete_cleanup(
        &mut self,
        cleanup: &CaptureCleanup,
        windows: &dyn CaptureWindowPort,
    ) -> Result<(), CaptureError> {
        let (outcome, error_code) = match &cleanup.outcome {
            TerminalOutcome::Completed => (SessionEndOutcome::Completed, None),
            TerminalOutcome::Cancelled => (SessionEndOutcome::Cancelled, None),
            TerminalOutcome::Failed(error) => (SessionEndOutcome::Failed, Some(error.code)),
        };
        self.complete_cleanup_with_terminal(cleanup, outcome, error_code, windows)
    }

    fn complete_cleanup_with_outcome(
        &mut self,
        cleanup: &CaptureCleanup,
        outcome: SessionEndOutcome,
        windows: &dyn CaptureWindowPort,
    ) -> Result<(), CaptureError> {
        self.complete_cleanup_with_terminal(cleanup, outcome, None, windows)
    }

    fn complete_cleanup_with_terminal(
        &mut self,
        cleanup: &CaptureCleanup,
        outcome: SessionEndOutcome,
        error_code: Option<CaptureErrorCode>,
        windows: &dyn CaptureWindowPort,
    ) -> Result<(), CaptureError> {
        let origin = self
            .hidden_origin
            .take()
            .filter(|origin| origin.session_id == cleanup.session_id)
            .map(|origin| PendingOriginRestore {
                window_label: origin.window_label,
                state: origin.state,
            });
        let mut pending = PendingWindowActions {
            overlay: Some(PendingOverlayAction::Hide),
            origin,
            next_retry: 0,
        };
        let window_actions = self.try_window_actions(&mut pending, windows);

        // Terminal coordinator state and every native pixel buffer are cleared
        // even when a platform window action fails. Only the small action record
        // above may survive for a bounded retry.
        self.coordinator.finish_restoration(&cleanup.session_id)?;
        if self.retiring_overlay_generation == Some(cleanup.overlay_generation) {
            let overlay_label = overlay_window_label(cleanup.overlay_generation);
            let follow_up_cleanup = self
                .coordinator
                .note_overlay_unavailable(&overlay_label, cleanup.overlay_generation)?;
            debug_assert!(follow_up_cleanup.is_none());
        }
        self.pending_overlay_monitor = None;
        self.active_delivery_lease = None;
        self.invalidated_target = None;
        self.active_export = None;
        if let (Some(target_window_label), Some(target_token)) = (
            cleanup.target_window_label.as_deref(),
            cleanup.target_token.as_deref(),
        ) && windows.window_exists(target_window_label)
        {
            // Notify only after the coordinator is Idle so an immediate re-trigger
            // from the target cannot race restoration. Delivery is best-effort and
            // never prevents pixels from being cleared or the origin being restored.
            let _ = windows.emit_session_ended(
                target_window_label,
                &SessionEndedPayload {
                    session_id: cleanup.session_id.clone(),
                    target_token: target_token.to_string(),
                    outcome,
                    error_code,
                },
            );
        }
        let overlay_label = overlay_window_label(cleanup.overlay_generation);
        if windows.window_exists(&overlay_label) {
            let _ = windows.emit_overlay_session_ended(
                &overlay_label,
                &OverlaySessionEndedPayload {
                    session_id: cleanup.session_id.clone(),
                    overlay_generation: cleanup.overlay_generation,
                    outcome,
                },
            );
        }
        match window_actions {
            Ok(()) if pending.is_empty() => Ok(()),
            Ok(()) => Err(CaptureError::new(
                CaptureErrorCode::OverlayFailed,
                "capture window cleanup made no progress",
            )),
            Err(error) => self.retain_pending_window_actions(pending, error, windows),
        }
    }
}

fn bounded_frontend_detail(detail: &str) -> String {
    const MAX_DETAIL_CHARS: usize = 512;
    let detail: String = detail.chars().take(MAX_DETAIL_CHARS).collect();
    if detail.trim().is_empty() {
        "capture overlay reported a failure".to_string()
    } else {
        detail
    }
}

fn stale_generation_error() -> CaptureError {
    CaptureError::new(
        CaptureErrorCode::InvalidSession,
        "capture overlay generation is stale or unexpected",
    )
}

fn no_active_session_error() -> CaptureError {
    CaptureError::new(
        CaptureErrorCode::InvalidSession,
        "capture session id is stale or unknown",
    )
}

fn unauthorized_error(detail: &str) -> CaptureError {
    CaptureError::new(CaptureErrorCode::UnauthorizedCaller, detail)
}

fn validate_target_registration(
    caller_window_label: &str,
    target_token: &str,
) -> Result<(), CaptureError> {
    if !is_regular_window_label(caller_window_label) {
        return Err(unauthorized_error(
            "only a regular application window may register a capture target",
        ));
    }
    if target_token.is_empty() || target_token.len() > 256 || target_token.trim() != target_token {
        return Err(CaptureError::new(
            CaptureErrorCode::TargetUnavailable,
            "capture target token is invalid",
        ));
    }
    Ok(())
}

pub(crate) fn is_regular_window_label(label: &str) -> bool {
    label == "main"
        || label
            .strip_prefix("window-")
            .is_some_and(|suffix| !suffix.is_empty())
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;

    use serde_json::Value;

    use super::{
        CAPTURE_PROTOCOL_VERSION, CapturePublishOutcome, CaptureTimeoutKind, CaptureWindowPort,
        CaptureWindowState, DeliveryFailedPayload, FRAME_AVAILABLE_EVENT, FrameAvailablePayload,
        OVERLAY_ROUTE, OVERLAY_WINDOW_LABEL, OverlayConcealment, OverlaySessionEndedPayload,
        OverlayWindowSpec, ResultAvailablePayload, ScreenCaptureRuntime, SessionEndedPayload,
        SessionStartedPayload, TargetUnavailablePayload, acquire_and_publish_once,
        is_overlay_window_label, overlay_window_label,
    };
    use crate::commands::screen_capture::backend::{
        NativeFrame, ScreenCaptureBackend, capture_frame_at_cursor,
    };
    use crate::commands::screen_capture::contract::{
        CaptureError, CaptureErrorCode, CaptureOrigin, CaptureResultSubmission, CaptureTarget,
        CapturedFrame, LogicalPoint, LogicalSize, MonitorGeometry, NativeCapturePhase,
        PhysicalPoint, PhysicalRect, PhysicalSize,
    };
    use crate::commands::screen_capture::export::{CaptureExportPort, SaveCaptureOutcome};

    #[derive(Debug, Clone, PartialEq)]
    enum WindowOperation {
        Create(OverlayWindowSpec),
        Destroy(String),
        Hide(String),
        Restore(String),
        Focus(String),
        Position(String, PhysicalRect),
        WakeForFrame(String),
        Show(String),
        Emit(String, FrameAvailablePayload),
        EmitResult(String, ResultAvailablePayload),
        EmitDeliveryFailed(String, DeliveryFailedPayload),
        EmitStarted(String, SessionStartedPayload),
        EmitEnded(String, SessionEndedPayload),
        EmitTargetUnavailable(String, TargetUnavailablePayload),
        EmitOverlayEnded(String, OverlaySessionEndedPayload),
    }

    #[test]
    fn overlay_window_labels_are_canonical_generation_identities() {
        assert_eq!(overlay_window_label(7), "screen-capture-overlay-7");
        assert!(is_overlay_window_label("screen-capture-overlay-7"));
        for invalid in [
            "screen-capture-overlay",
            "screen-capture-overlay-0",
            "screen-capture-overlay-07",
            "screen-capture-overlay--1",
            "screen-capture-overlay-9007199254740992",
            "screen-capture-overlay-7-extra",
        ] {
            assert!(!is_overlay_window_label(invalid), "accepted {invalid}");
        }
    }

    #[derive(Default)]
    struct FakeWindows {
        windows: Mutex<HashSet<String>>,
        window_states: Mutex<HashMap<String, CaptureWindowState>>,
        operations: Mutex<Vec<WindowOperation>>,
        defer_position_until_presented: Mutex<bool>,
        ephemeral_overlay: Mutex<bool>,
        reentrant_runtime: Mutex<Option<Arc<ScreenCaptureRuntime>>>,
        reentrant_runtime_on_window_lookup: Mutex<Option<Arc<ScreenCaptureRuntime>>>,
        reentrant_runtime_on_session_started: Mutex<Option<Arc<ScreenCaptureRuntime>>>,
        restore_requests: Mutex<Vec<(String, CaptureWindowState)>>,
        hide_failures: Mutex<HashMap<String, usize>>,
        restore_failures: Mutex<HashMap<String, usize>>,
        deferred_destroys: Mutex<Vec<String>>,
        scheduled_retries: Mutex<Vec<Duration>>,
    }

    impl FakeWindows {
        fn with_windows(labels: &[&str]) -> Self {
            Self {
                windows: Mutex::new(labels.iter().map(|label| (*label).to_string()).collect()),
                window_states: Mutex::new(
                    labels
                        .iter()
                        .map(|label| {
                            (
                                (*label).to_string(),
                                CaptureWindowState {
                                    visible: true,
                                    minimized: false,
                                    focused: false,
                                },
                            )
                        })
                        .collect(),
                ),
                operations: Mutex::new(Vec::new()),
                defer_position_until_presented: Mutex::new(false),
                ephemeral_overlay: Mutex::new(false),
                reentrant_runtime: Mutex::new(None),
                reentrant_runtime_on_window_lookup: Mutex::new(None),
                reentrant_runtime_on_session_started: Mutex::new(None),
                restore_requests: Mutex::new(Vec::new()),
                hide_failures: Mutex::new(HashMap::new()),
                restore_failures: Mutex::new(HashMap::new()),
                deferred_destroys: Mutex::new(Vec::new()),
                scheduled_retries: Mutex::new(Vec::new()),
            }
        }

        fn operations(&self) -> Vec<WindowOperation> {
            self.operations.lock().expect("operations lock").clone()
        }

        fn clear_operations(&self) {
            self.operations.lock().expect("operations lock").clear();
        }

        fn defer_overlay_position_until_presented(&self) {
            *self
                .defer_position_until_presented
                .lock()
                .expect("deferred position policy lock") = true;
        }

        fn use_ephemeral_overlay(&self) {
            *self
                .ephemeral_overlay
                .lock()
                .expect("ephemeral overlay policy lock") = true;
        }

        fn reenter_runtime_during_next_window_lookup(&self, runtime: Arc<ScreenCaptureRuntime>) {
            *self
                .reentrant_runtime_on_window_lookup
                .lock()
                .expect("window lookup reentrant runtime lock") = Some(runtime);
        }

        fn reenter_runtime_during_session_started(&self, runtime: Arc<ScreenCaptureRuntime>) {
            *self
                .reentrant_runtime_on_session_started
                .lock()
                .expect("session-started reentrant runtime lock") = Some(runtime);
        }

        fn reenter_runtime_during_overlay_creation(&self, runtime: Arc<ScreenCaptureRuntime>) {
            *self
                .reentrant_runtime
                .lock()
                .expect("reentrant runtime lock") = Some(runtime);
        }

        fn remove_window_without_callback(&self, label: &str) {
            self.windows.lock().expect("windows lock").remove(label);
            self.window_states
                .lock()
                .expect("window states")
                .remove(label);
        }

        fn destroy_window_as_framework(&self, label: &str) {
            self.windows.lock().expect("windows lock").remove(label);
            self.window_states
                .lock()
                .expect("window states")
                .remove(label);
            self.operations
                .lock()
                .expect("operations lock")
                .push(WindowOperation::Destroy(label.to_string()));
        }

        fn fail_next_hide(&self, label: &str) {
            self.fail_hides(label, 1);
        }

        fn fail_next_restore(&self, label: &str) {
            self.fail_restores(label, 1);
        }

        fn fail_hides(&self, label: &str, count: usize) {
            self.hide_failures
                .lock()
                .expect("hide failures")
                .insert(label.to_string(), count);
        }

        fn fail_restores(&self, label: &str, count: usize) {
            self.restore_failures
                .lock()
                .expect("restore failures")
                .insert(label.to_string(), count);
        }

        fn should_fail(failures: &Mutex<HashMap<String, usize>>, label: &str) -> bool {
            let mut failures = failures.lock().expect("failure counters");
            let Some(remaining) = failures.get_mut(label) else {
                return false;
            };
            *remaining -= 1;
            if *remaining == 0 {
                failures.remove(label);
            }
            true
        }

        fn set_window_state(&self, label: &str, state: CaptureWindowState) {
            self.window_states
                .lock()
                .expect("window states")
                .insert(label.to_string(), state);
        }

        fn window_state(&self, label: &str) -> Option<CaptureWindowState> {
            self.window_states
                .lock()
                .expect("window states")
                .get(label)
                .copied()
        }

        fn restore_requests(&self) -> Vec<(String, CaptureWindowState)> {
            self.restore_requests
                .lock()
                .expect("restore requests")
                .clone()
        }

        fn scheduled_retries(&self) -> Vec<Duration> {
            self.scheduled_retries
                .lock()
                .expect("scheduled retries")
                .clone()
        }

        fn run_deferred_destructions(&self) {
            let labels =
                std::mem::take(&mut *self.deferred_destroys.lock().expect("deferred destroys"));
            for label in labels {
                self.destroy_window_as_framework(&label);
            }
        }
    }

    impl CaptureWindowPort for FakeWindows {
        fn window_exists(&self, label: &str) -> bool {
            if let Some(runtime) = self
                .reentrant_runtime_on_window_lookup
                .lock()
                .expect("window lookup reentrant runtime lock")
                .take()
            {
                let (sender, receiver) = mpsc::channel();
                std::thread::spawn(move || {
                    let result = runtime.current_overlay_generation();
                    let _ = sender.send(result);
                });
                if receiver.recv_timeout(Duration::from_millis(250)).is_err() {
                    return false;
                }
            }
            self.windows.lock().expect("windows lock").contains(label)
        }

        fn create_overlay(&self, spec: &OverlayWindowSpec) -> Result<(), CaptureError> {
            let inserted = self
                .windows
                .lock()
                .expect("windows lock")
                .insert(spec.label.clone());
            if !inserted {
                return Err(CaptureError::new(
                    CaptureErrorCode::OverlayFailed,
                    "duplicate overlay",
                ));
            }
            self.window_states.lock().expect("window states").insert(
                spec.label.clone(),
                CaptureWindowState {
                    visible: spec.visible,
                    minimized: false,
                    focused: false,
                },
            );
            self.operations
                .lock()
                .expect("operations lock")
                .push(WindowOperation::Create(spec.clone()));
            if let Some(runtime) = self
                .reentrant_runtime
                .lock()
                .expect("reentrant runtime lock")
                .clone()
            {
                let (sender, receiver) = mpsc::channel();
                std::thread::spawn(move || {
                    let result = runtime.current_overlay_generation();
                    let _ = sender.send(result);
                });
                receiver
                    .recv_timeout(Duration::from_millis(250))
                    .map_err(|_| {
                        CaptureError::new(
                            CaptureErrorCode::OverlayFailed,
                            "capture runtime remained locked during overlay creation",
                        )
                    })??;
            }
            Ok(())
        }

        fn defers_overlay_position_until_presented(&self) -> bool {
            *self
                .defer_position_until_presented
                .lock()
                .expect("deferred position policy lock")
        }

        fn uses_ephemeral_overlay(&self) -> bool {
            *self
                .ephemeral_overlay
                .lock()
                .expect("ephemeral overlay policy lock")
        }

        fn conceal_overlay_for_capture(&self, _label: &str) -> Result<(), CaptureError> {
            Ok(())
        }

        fn wake_overlay_for_frame_delivery(
            &self,
            label: &str,
            _monitor: &MonitorGeometry,
        ) -> Result<(), CaptureError> {
            self.operations
                .lock()
                .expect("operations lock")
                .push(WindowOperation::WakeForFrame(label.to_string()));
            Ok(())
        }

        fn capture_window_state(
            &self,
            label: &str,
        ) -> Result<Option<CaptureWindowState>, CaptureError> {
            Ok(self.window_state(label))
        }

        fn hide_window(&self, label: &str) -> Result<(), CaptureError> {
            self.operations
                .lock()
                .expect("operations lock")
                .push(WindowOperation::Hide(label.to_string()));
            if Self::should_fail(&self.hide_failures, label) {
                return Err(CaptureError::new(
                    CaptureErrorCode::OverlayFailed,
                    "injected hide failure",
                ));
            }
            if let Some(state) = self
                .window_states
                .lock()
                .expect("window states")
                .get_mut(label)
            {
                state.visible = false;
                state.focused = false;
            }
            Ok(())
        }

        fn conceal_overlay(&self, label: &str) -> Result<OverlayConcealment, CaptureError> {
            if self.uses_ephemeral_overlay() {
                self.deferred_destroys
                    .lock()
                    .expect("deferred destroys")
                    .push(label.to_string());
                return Ok(OverlayConcealment::RetirementScheduled);
            }
            match self.hide_window(label) {
                Ok(()) => Ok(OverlayConcealment::Hidden),
                Err(error) => {
                    self.deferred_destroys
                        .lock()
                        .expect("deferred destroys")
                        .push(label.to_string());
                    Ok(OverlayConcealment::DestructionDeferred(error))
                }
            }
        }

        fn restore_window(
            &self,
            label: &str,
            state: CaptureWindowState,
        ) -> Result<(), CaptureError> {
            self.operations
                .lock()
                .expect("operations lock")
                .push(WindowOperation::Restore(label.to_string()));
            self.restore_requests
                .lock()
                .expect("restore requests")
                .push((label.to_string(), state));
            if Self::should_fail(&self.restore_failures, label) {
                return Err(CaptureError::new(
                    CaptureErrorCode::OverlayFailed,
                    "injected restore failure",
                ));
            }
            if self.window_exists(label) {
                self.set_window_state(label, state);
            }
            Ok(())
        }

        fn defer_window_action_retry(&self, delay: Duration) {
            self.scheduled_retries
                .lock()
                .expect("scheduled retries")
                .push(delay);
        }

        fn focus_window(&self, label: &str) -> Result<(), CaptureError> {
            self.operations
                .lock()
                .expect("operations lock")
                .push(WindowOperation::Focus(label.to_string()));
            Ok(())
        }

        fn position_overlay(
            &self,
            label: &str,
            monitor: &MonitorGeometry,
        ) -> Result<(), CaptureError> {
            let bounds = PhysicalRect {
                origin: monitor.physical_origin,
                size: monitor.physical_size,
            };
            self.operations
                .lock()
                .expect("operations lock")
                .push(WindowOperation::Position(label.to_string(), bounds));
            Ok(())
        }

        fn show_overlay(&self, label: &str) -> Result<(), CaptureError> {
            self.operations
                .lock()
                .expect("operations lock")
                .push(WindowOperation::Show(label.to_string()));
            if let Some(state) = self
                .window_states
                .lock()
                .expect("window states")
                .get_mut(label)
            {
                state.visible = true;
                state.minimized = false;
                state.focused = true;
            }
            Ok(())
        }

        fn emit_frame_available(
            &self,
            label: &str,
            payload: &FrameAvailablePayload,
        ) -> Result<(), CaptureError> {
            self.operations
                .lock()
                .expect("operations lock")
                .push(WindowOperation::Emit(label.to_string(), payload.clone()));
            Ok(())
        }

        fn emit_result_available(
            &self,
            label: &str,
            payload: &ResultAvailablePayload,
        ) -> Result<(), CaptureError> {
            self.operations
                .lock()
                .expect("operations lock")
                .push(WindowOperation::EmitResult(
                    label.to_string(),
                    payload.clone(),
                ));
            Ok(())
        }

        fn emit_delivery_failed(
            &self,
            label: &str,
            payload: &DeliveryFailedPayload,
        ) -> Result<(), CaptureError> {
            self.operations.lock().expect("operations lock").push(
                WindowOperation::EmitDeliveryFailed(label.to_string(), payload.clone()),
            );
            Ok(())
        }

        fn emit_session_started(
            &self,
            label: &str,
            payload: &SessionStartedPayload,
        ) -> Result<(), CaptureError> {
            if let Some(runtime) = self
                .reentrant_runtime_on_session_started
                .lock()
                .expect("session-started reentrant runtime lock")
                .clone()
            {
                let (sender, receiver) = mpsc::channel();
                std::thread::spawn(move || {
                    let result = runtime.current_overlay_generation();
                    let _ = sender.send(result);
                });
                receiver
                    .recv_timeout(Duration::from_millis(250))
                    .map_err(|_| {
                        CaptureError::new(
                            CaptureErrorCode::OverlayFailed,
                            "capture runtime remained locked during session-started publication",
                        )
                    })??;
            }
            self.operations
                .lock()
                .expect("operations lock")
                .push(WindowOperation::EmitStarted(
                    label.to_string(),
                    payload.clone(),
                ));
            Ok(())
        }

        fn emit_session_ended(
            &self,
            label: &str,
            payload: &SessionEndedPayload,
        ) -> Result<(), CaptureError> {
            self.operations
                .lock()
                .expect("operations lock")
                .push(WindowOperation::EmitEnded(
                    label.to_string(),
                    payload.clone(),
                ));
            Ok(())
        }

        fn emit_target_unavailable(
            &self,
            label: &str,
            payload: &TargetUnavailablePayload,
        ) -> Result<(), CaptureError> {
            self.operations.lock().expect("operations lock").push(
                WindowOperation::EmitTargetUnavailable(label.to_string(), payload.clone()),
            );
            Ok(())
        }

        fn emit_overlay_session_ended(
            &self,
            label: &str,
            payload: &OverlaySessionEndedPayload,
        ) -> Result<(), CaptureError> {
            self.operations.lock().expect("operations lock").push(
                WindowOperation::EmitOverlayEnded(label.to_string(), payload.clone()),
            );
            Ok(())
        }
    }

    struct FakeBackend {
        monitors: Vec<MonitorGeometry>,
        frames: Mutex<HashMap<String, NativeFrame>>,
    }

    struct FakeExports {
        selected_path: Result<Option<PathBuf>, CaptureError>,
        write_error: Option<CaptureError>,
        copy_error: Option<CaptureError>,
        writes: Mutex<Vec<Vec<u8>>>,
        copies: Mutex<Vec<Vec<u8>>>,
    }

    impl FakeExports {
        fn successful() -> Self {
            Self {
                selected_path: Ok(Some(std::env::temp_dir().join("capture.png"))),
                write_error: None,
                copy_error: None,
                writes: Mutex::new(Vec::new()),
                copies: Mutex::new(Vec::new()),
            }
        }
    }

    impl CaptureExportPort for FakeExports {
        fn choose_save_path(&self, _: &str) -> Result<Option<PathBuf>, CaptureError> {
            self.selected_path.clone()
        }

        fn write_png(&self, _: &Path, png: &[u8]) -> Result<(), CaptureError> {
            if let Some(error) = self.write_error.clone() {
                return Err(error);
            }
            self.writes.lock().expect("writes").push(png.to_vec());
            Ok(())
        }

        fn copy_png(&self, _: u32, _: u32, png: &[u8]) -> Result<(), CaptureError> {
            if let Some(error) = self.copy_error.clone() {
                return Err(error);
            }
            self.copies.lock().expect("copies").push(png.to_vec());
            Ok(())
        }
    }

    struct BlockingCopyExport {
        entered: mpsc::Sender<()>,
        release: Mutex<mpsc::Receiver<()>>,
    }

    impl CaptureExportPort for BlockingCopyExport {
        fn choose_save_path(&self, _: &str) -> Result<Option<PathBuf>, CaptureError> {
            unreachable!("copy-only test export")
        }

        fn write_png(&self, _: &Path, _: &[u8]) -> Result<(), CaptureError> {
            unreachable!("copy-only test export")
        }

        fn copy_png(&self, _: u32, _: u32, _: &[u8]) -> Result<(), CaptureError> {
            self.entered.send(()).expect("announce blocked copy");
            self.release
                .lock()
                .expect("release receiver")
                .recv()
                .expect("release blocked copy");
            Ok(())
        }
    }

    impl FakeBackend {
        fn one_frame() -> Self {
            let monitor = MonitorGeometry {
                id: "monitor-2".to_string(),
                physical_origin: PhysicalPoint { x: -8, y: 12 },
                physical_size: PhysicalSize {
                    width: 2,
                    height: 2,
                },
                logical_origin: LogicalPoint { x: -4.0, y: 6.0 },
                logical_size: LogicalSize {
                    width: 1.0,
                    height: 1.0,
                },
                scale_factor: 2.0,
            };
            Self {
                monitors: vec![monitor],
                frames: Mutex::new(HashMap::from([(
                    "monitor-2".to_string(),
                    NativeFrame {
                        width: 2,
                        height: 2,
                        stride: 8,
                        bytes: vec![0x5a; 16],
                    },
                )])),
            }
        }
    }

    impl ScreenCaptureBackend for FakeBackend {
        fn monitors(&self) -> Result<Vec<MonitorGeometry>, CaptureError> {
            Ok(self.monitors.clone())
        }

        fn monitor_index_at_cursor(
            &self,
            _monitors: &[MonitorGeometry],
        ) -> Result<usize, CaptureError> {
            Ok(0)
        }

        fn capture_monitor(&self, monitor: &MonitorGeometry) -> Result<NativeFrame, CaptureError> {
            self.frames
                .lock()
                .expect("frames lock")
                .remove(&monitor.id)
                .ok_or_else(|| CaptureError::new(CaptureErrorCode::CaptureFailed, "missing frame"))
        }
    }

    fn captured_frame(session_id: &str) -> CapturedFrame {
        capture_frame_at_cursor(&FakeBackend::one_frame(), session_id).expect("fixture frame")
    }

    fn target(label: &str) -> CaptureTarget {
        CaptureTarget {
            window_label: label.to_string(),
            target_token: "target-secret".to_string(),
        }
    }

    fn init_ready(
        runtime: &ScreenCaptureRuntime,
        windows: &FakeWindows,
        backend: &FakeBackend,
    ) -> u64 {
        let init = runtime
            .init_overlay("main", windows)
            .expect("initialize overlay");
        runtime
            .overlay_ready(
                OVERLAY_WINDOW_LABEL,
                init.overlay_generation,
                CAPTURE_PROTOCOL_VERSION,
                windows,
                backend,
            )
            .expect("ready overlay");
        init.overlay_generation
    }

    fn active_composer(
        runtime: &ScreenCaptureRuntime,
        windows: &FakeWindows,
        session_id: &str,
    ) -> u64 {
        let backend = FakeBackend::one_frame();
        let generation = init_ready(runtime, windows, &backend);
        runtime
            .start_composer(
                "main",
                session_id.to_string(),
                target("window-target"),
                windows,
                &backend,
            )
            .expect("start capture");
        runtime
            .take_frame(OVERLAY_WINDOW_LABEL, session_id, generation)
            .expect("take frame");
        runtime
            .frame_presented(OVERLAY_WINDOW_LABEL, session_id, generation, windows)
            .expect("present frame");
        generation
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        crate::commands::screen_capture::png_fixture(width, height)
    }

    fn stored_result(
        runtime: &ScreenCaptureRuntime,
        windows: &FakeWindows,
        session_id: &str,
    ) -> (u64, String, Vec<u8>) {
        let generation = active_composer(runtime, windows, session_id);
        let bytes = png(2, 2);
        let descriptor = runtime
            .store_result(
                OVERLAY_WINDOW_LABEL,
                session_id,
                generation,
                CaptureResultSubmission {
                    result_id: format!("result-{session_id}"),
                    filename: "Plain-capture-1.png".to_string(),
                    width: 2,
                    height: 2,
                    bytes: bytes.clone(),
                },
            )
            .expect("store PNG result");
        (generation, descriptor.result_id, bytes)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancel_acquires_runtime_while_backend_waits_and_drops_the_late_frame() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let backend = FakeBackend::one_frame();
        init_ready(&runtime, &windows, &backend);
        let reservation = runtime
            .reserve_composer_capture(
                "main",
                "session-cancel-in-flight".to_string(),
                target("window-target"),
                &windows,
            )
            .expect("reserve capture");
        let ticket = reservation.ticket.expect("ready capture ticket");
        let late_ticket = ticket.clone();
        let acquisitions = Arc::new(AtomicUsize::new(0));
        let counted = acquisitions.clone();
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let late_frame = captured_frame(ticket.session_id());
        let acquisition = acquire_and_publish_once(
            &runtime,
            ticket,
            &windows,
            Duration::ZERO,
            Duration::from_secs(1),
            move || {
                counted.fetch_add(1, Ordering::SeqCst);
                async move {
                    let _ = entered_tx.send(());
                    release_rx.await.expect("release late backend");
                    Ok(late_frame)
                }
            },
        );
        tokio::pin!(acquisition);
        tokio::select! {
            result = &mut acquisition => panic!("backend completed before cancellation: {result:?}"),
            entered = entered_rx => entered.expect("backend entered"),
        }

        runtime
            .cancel_from_window("main", "session-cancel-in-flight", None, &windows)
            .expect("cancel while backend is in flight");
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), &mut acquisition)
                .await
                .expect("cancellation stops waiting for backend")
                .expect("cancelled acquisition is harmless"),
            CapturePublishOutcome::Stale
        );
        assert!(release_tx.send(()).is_err(), "backend future was dropped");
        assert_eq!(
            runtime
                .complete_capture(
                    &late_ticket,
                    Ok(captured_frame(late_ticket.session_id())),
                    &windows,
                )
                .expect("late detached backend result is harmless"),
            CapturePublishOutcome::Stale
        );
        assert_eq!(acquisitions.load(Ordering::SeqCst), 1);
        assert_eq!(runtime.active_phase().unwrap(), NativeCapturePhase::Idle);
        assert!(!runtime.has_sensitive_buffers().unwrap());
        assert!(
            !windows
                .operations()
                .iter()
                .any(|operation| matches!(operation, WindowOperation::Emit(_, _)))
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn watchdog_timeout_acquires_runtime_while_backend_waits_and_rejects_late_result() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let backend = FakeBackend::one_frame();
        init_ready(&runtime, &windows, &backend);
        let reservation = runtime
            .reserve_composer_capture(
                "main",
                "session-timeout-in-flight".to_string(),
                target("window-target"),
                &windows,
            )
            .expect("reserve capture");
        let generation = reservation.response.overlay_generation;
        let ticket = reservation.ticket.expect("ready capture ticket");
        let late_ticket = ticket.clone();
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let late_frame = captured_frame(ticket.session_id());
        let acquisition = acquire_and_publish_once(
            &runtime,
            ticket,
            &windows,
            Duration::ZERO,
            Duration::from_secs(1),
            move || async move {
                let _ = entered_tx.send(());
                release_rx.await.expect("release late backend");
                Ok(late_frame)
            },
        );
        tokio::pin!(acquisition);
        tokio::select! {
            result = &mut acquisition => panic!("backend completed before timeout: {result:?}"),
            entered = entered_rx => entered.expect("backend entered"),
        }

        assert!(
            runtime
                .expire_session(
                    "session-timeout-in-flight",
                    generation,
                    CaptureTimeoutKind::Lifetime,
                    &windows,
                )
                .expect("watchdog expires active capture")
        );
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), &mut acquisition)
                .await
                .expect("watchdog stops waiting for backend")
                .expect("timed-out acquisition is harmless"),
            CapturePublishOutcome::Stale
        );
        assert!(release_tx.send(()).is_err(), "backend future was dropped");
        assert_eq!(
            runtime
                .complete_capture(
                    &late_ticket,
                    Ok(captured_frame(late_ticket.session_id())),
                    &windows,
                )
                .expect("late detached backend result is harmless"),
            CapturePublishOutcome::Stale
        );
        assert_eq!(runtime.active_phase().unwrap(), NativeCapturePhase::Idle);
        assert!(!runtime.has_sensitive_buffers().unwrap());
        assert!(
            !windows
                .operations()
                .iter()
                .any(|operation| matches!(operation, WindowOperation::Emit(_, _)))
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reserved_capture_enters_backend_and_publishes_exactly_once() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let backend = FakeBackend::one_frame();
        init_ready(&runtime, &windows, &backend);
        let reservation = runtime
            .reserve_composer_capture(
                "main",
                "session-one-acquisition".to_string(),
                target("window-target"),
                &windows,
            )
            .expect("reserve capture");
        let ticket = reservation.ticket.expect("ready capture ticket");
        let acquisitions = Arc::new(AtomicUsize::new(0));
        let counted = acquisitions.clone();
        let frame = captured_frame(ticket.session_id());

        assert_eq!(
            acquire_and_publish_once(
                &runtime,
                ticket,
                &windows,
                Duration::ZERO,
                Duration::from_secs(1),
                move || {
                    counted.fetch_add(1, Ordering::SeqCst);
                    async move { Ok(frame) }
                },
            )
            .await
            .expect("publish capture"),
            CapturePublishOutcome::Published
        );
        assert_eq!(acquisitions.load(Ordering::SeqCst), 1);
        assert_eq!(
            runtime.active_phase().unwrap(),
            NativeCapturePhase::FrameAvailable
        );
        assert_eq!(
            windows
                .operations()
                .iter()
                .filter(|operation| matches!(operation, WindowOperation::Emit(_, _)))
                .count(),
            1
        );
    }

    #[test]
    fn overlay_creation_allows_page_load_reentry_into_runtime_state() {
        let runtime = Arc::new(ScreenCaptureRuntime::new().expect("runtime"));
        let windows = FakeWindows::with_windows(&["main"]);
        windows.reenter_runtime_during_overlay_creation(runtime.clone());

        let init = runtime
            .init_overlay("main", &windows)
            .expect("overlay creation must not hold capture state across the window adapter");

        assert_eq!(
            runtime.current_overlay_generation().unwrap(),
            Some(init.overlay_generation)
        );
    }

    #[test]
    fn capture_creates_a_missing_windows_overlay_only_once() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);

        let reservation = runtime
            .reserve_composer_capture(
                "main",
                "session-new-overlay".to_string(),
                target("window-target"),
                &windows,
            )
            .expect("reserve capture behind new overlay readiness");

        assert_eq!(reservation.response.overlay_generation, 1);
        assert_eq!(
            reservation.response.phase,
            NativeCapturePhase::WaitingForOverlay
        );
        assert!(reservation.ticket.is_none());
        assert!(matches!(
            windows.operations().as_slice(),
            [WindowOperation::Create(OverlayWindowSpec {
                label,
                route,
                visible: false,
            })] if label == OVERLAY_WINDOW_LABEL
                && route == &format!("{OVERLAY_ROUTE}?overlayGeneration=1")
        ));
        assert!(windows.window_state("main").unwrap().visible);
    }

    #[test]
    fn terminal_cleanup_retires_an_ephemeral_overlay_before_the_next_capture() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let generation = active_composer(&runtime, &windows, "session-ephemeral-overlay");
        windows.use_ephemeral_overlay();
        windows.clear_operations();

        runtime
            .cancel_from_window(
                OVERLAY_WINDOW_LABEL,
                "session-ephemeral-overlay",
                Some(generation),
                &windows,
            )
            .expect("terminal cleanup schedules overlay retirement");

        assert_eq!(runtime.active_phase().unwrap(), NativeCapturePhase::Idle);
        assert!(!runtime.has_sensitive_buffers().unwrap());
        assert_eq!(runtime.current_overlay_generation().unwrap(), None);
        assert!(windows.window_exists(OVERLAY_WINDOW_LABEL));

        let error = runtime
            .reserve_composer_capture(
                "main",
                "session-before-retirement-ack".into(),
                target("window-target"),
                &windows,
            )
            .expect_err("a new capture must not race deferred overlay destruction");
        assert_eq!(error.code, CaptureErrorCode::Busy);

        windows.run_deferred_destructions();
        assert!(
            !runtime
                .overlay_destroyed(OVERLAY_WINDOW_LABEL, &windows)
                .expect("terminal retirement callback does not prewarm a replacement")
        );

        let reservation = runtime
            .reserve_composer_capture(
                "main",
                "session-after-retirement-ack".into(),
                target("window-target"),
                &windows,
            )
            .expect("the next capture creates a fresh overlay");
        assert_eq!(reservation.response.overlay_generation, generation + 1);
        assert_eq!(
            reservation.response.phase,
            NativeCapturePhase::WaitingForOverlay
        );
        assert!(reservation.ticket.is_none());
    }

    #[test]
    fn readiness_timeout_clears_a_waiting_session_without_hiding_its_origin() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let backend = FakeBackend::one_frame();
        let generation = runtime
            .init_overlay("main", &windows)
            .expect("initialize overlay")
            .overlay_generation;
        runtime
            .start_composer(
                "main",
                "session-timeout".to_string(),
                target("window-target"),
                &windows,
                &backend,
            )
            .expect("start waiting capture");
        windows.clear_operations();

        assert!(
            runtime
                .expire_session(
                    "session-timeout",
                    generation,
                    CaptureTimeoutKind::Readiness,
                    &windows,
                )
                .expect("expire waiting capture")
        );

        assert_eq!(runtime.active_phase().unwrap(), NativeCapturePhase::Idle);
        assert!(!runtime.has_sensitive_buffers().unwrap());
        assert!(
            !windows
                .operations()
                .contains(&WindowOperation::Restore("main".into()))
        );
        assert!(windows.operations().iter().any(|operation| matches!(
            operation,
            WindowOperation::EmitEnded(
                _,
                SessionEndedPayload {
                    outcome: super::SessionEndOutcome::Failed,
                    ..
                }
            )
        )));
    }

    #[test]
    fn presentation_timeout_restores_the_origin_and_clears_an_unclaimed_frame() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let backend = FakeBackend::one_frame();
        let generation = init_ready(&runtime, &windows, &backend);
        runtime
            .start_composer(
                "main",
                "session-presentation-timeout".to_string(),
                target("window-target"),
                &windows,
                &backend,
            )
            .expect("publish a frame");
        assert_eq!(
            runtime.active_phase().unwrap(),
            NativeCapturePhase::FrameAvailable
        );
        windows.clear_operations();

        assert!(
            runtime
                .expire_session(
                    "session-presentation-timeout",
                    generation,
                    CaptureTimeoutKind::Presentation,
                    &windows,
                )
                .expect("expire unclaimed frame")
        );

        assert_eq!(runtime.active_phase().unwrap(), NativeCapturePhase::Idle);
        assert!(!runtime.has_sensitive_buffers().unwrap());
        assert!(
            windows
                .operations()
                .contains(&WindowOperation::Restore("main".into()))
        );
    }

    #[test]
    fn stale_and_phase_specific_timeouts_cannot_terminate_the_wrong_capture() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let generation = active_composer(&runtime, &windows, "session-live");

        assert!(
            !runtime
                .expire_session("stale", generation, CaptureTimeoutKind::Lifetime, &windows)
                .unwrap()
        );
        assert!(
            !runtime
                .expire_session(
                    "session-live",
                    generation + 1,
                    CaptureTimeoutKind::Lifetime,
                    &windows
                )
                .unwrap()
        );
        assert!(
            !runtime
                .expire_session(
                    "session-live",
                    generation,
                    CaptureTimeoutKind::Presentation,
                    &windows
                )
                .unwrap()
        );
        assert!(
            !runtime
                .expire_session(
                    "session-live",
                    generation,
                    CaptureTimeoutKind::Readiness,
                    &windows
                )
                .unwrap()
        );
        assert_eq!(runtime.active_phase().unwrap(), NativeCapturePhase::Active);

        windows.clear_operations();
        assert!(
            runtime
                .expire_session(
                    "session-live",
                    generation,
                    CaptureTimeoutKind::Lifetime,
                    &windows
                )
                .unwrap()
        );
        assert_eq!(runtime.active_phase().unwrap(), NativeCapturePhase::Idle);
        let operations = windows.operations();
        let restore = operations
            .iter()
            .position(|operation| operation == &WindowOperation::Restore("main".into()))
            .expect("restore origin");
        let target_event = operations
            .iter()
            .position(|operation| matches!(operation, WindowOperation::EmitEnded(_, _)))
            .expect("target terminal");
        assert!(restore < target_event);
    }

    #[test]
    fn restoration_failure_reports_the_error_without_wedging_the_process_session() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let generation = active_composer(&runtime, &windows, "session-restore-failure");
        windows.fail_next_restore("main");

        assert_eq!(
            runtime
                .cancel_from_window(
                    OVERLAY_WINDOW_LABEL,
                    "session-restore-failure",
                    Some(generation),
                    &windows,
                )
                .expect_err("restore failure remains observable")
                .detail,
            "injected restore failure"
        );
        assert_eq!(runtime.active_phase().unwrap(), NativeCapturePhase::Idle);
        assert!(!runtime.has_sensitive_buffers().unwrap());
        runtime
            .retry_pending_window_actions(&windows)
            .expect("transient restore succeeds on retry");

        let next_backend = FakeBackend::one_frame();
        assert!(
            runtime
                .start_composer(
                    "main",
                    "session-after-restore-failure".to_string(),
                    target("window-target"),
                    &windows,
                    &next_backend,
                )
                .is_ok()
        );
    }

    #[test]
    fn listener_ready_barrier_captures_for_distinct_origin_and_target_without_disclosure() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let backend = FakeBackend::one_frame();
        let generation = init_ready(&runtime, &windows, &backend);
        windows.clear_operations();

        let started = runtime
            .start_composer(
                "main",
                "session-1".to_string(),
                target("window-target"),
                &windows,
                &backend,
            )
            .expect("start capture");
        assert_eq!(started.phase, NativeCapturePhase::FrameAvailable);

        let operations = windows.operations();
        assert_eq!(operations[0], WindowOperation::Hide("main".into()));
        let position_index = operations
            .iter()
            .position(|operation| matches!(operation, WindowOperation::Position(_, _)))
            .expect("position overlay");
        let wake_index = operations
            .iter()
            .position(|operation| matches!(operation, WindowOperation::WakeForFrame(_)))
            .expect("wake overlay for frame delivery");
        let publish_index = operations
            .iter()
            .position(|operation| matches!(operation, WindowOperation::Emit(_, _)))
            .expect("publish frame metadata");
        assert!(position_index < wake_index);
        assert!(wake_index < publish_index);
        let WindowOperation::Emit(label, payload) = &operations[publish_index] else {
            unreachable!();
        };
        assert_eq!(label, OVERLAY_WINDOW_LABEL);
        assert_eq!(payload.session_id, "session-1");
        assert_eq!(payload.overlay_generation, generation);
        assert_eq!(payload.descriptor.byte_len, 16);
        assert!(payload.can_confirm);
        assert!(
            !operations
                .iter()
                .any(|operation| matches!(operation, WindowOperation::Show(_)))
        );

        let json = serde_json::to_value(payload).expect("serialize frame event");
        let object = json.as_object().expect("event object");
        assert_eq!(object.len(), 4);
        assert_eq!(
            object.get("sessionId"),
            Some(&Value::String("session-1".into()))
        );
        let encoded = serde_json::to_string(payload).expect("event JSON");
        assert!(!encoded.contains("window-target"));
        assert!(!encoded.contains("target-secret"));
        assert!(!encoded.contains("bytes"));
        assert_eq!(FRAME_AVAILABLE_EVENT, "screen-capture://frame-available");
    }

    #[test]
    fn global_capture_without_a_target_marks_confirm_unavailable() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main"]);
        let backend = FakeBackend::one_frame();
        let generation = init_ready(&runtime, &windows, &backend);
        windows.clear_operations();

        runtime
            .start_global("global-1".to_string(), None, None, &windows, &backend)
            .expect("global capture");
        let operation = windows
            .operations()
            .into_iter()
            .find(|operation| matches!(operation, WindowOperation::Emit(_, _)))
            .expect("frame event");
        let WindowOperation::Emit(_, payload) = operation else {
            unreachable!()
        };
        assert!(!payload.can_confirm);
        runtime
            .cancel_from_window(OVERLAY_WINDOW_LABEL, "global-1", Some(generation), &windows)
            .expect("cancel global capture");
    }

    #[test]
    fn registered_global_target_prefers_focused_window_then_latest_eligible_window() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-second"]);
        let backend = FakeBackend::one_frame();
        init_ready(&runtime, &windows, &backend);
        runtime
            .register_eligible_target("main", "token-main")
            .expect("register main target");
        runtime
            .register_eligible_target("window-second", "token-second")
            .expect("register second target");

        runtime
            .start_global_with_registered_target(
                "global-focused".to_string(),
                Some(CaptureOrigin {
                    window_label: "main".to_string(),
                }),
                &windows,
                &backend,
            )
            .expect("start focused global capture");
        assert_eq!(
            runtime
                .lock()
                .expect("runtime lock")
                .coordinator
                .active_state()
                .and_then(|state| state.target.clone()),
            Some(CaptureTarget {
                window_label: "main".to_string(),
                target_token: "token-main".to_string(),
            })
        );
        runtime
            .cancel_from_window("main", "global-focused", None, &windows)
            .expect("cancel focused capture");

        let next_backend = FakeBackend::one_frame();
        runtime
            .start_global_with_registered_target(
                "global-background".to_string(),
                None,
                &windows,
                &next_backend,
            )
            .expect("start background global capture");
        assert_eq!(
            runtime
                .lock()
                .expect("runtime lock")
                .coordinator
                .active_state()
                .and_then(|state| state.target.clone()),
            Some(CaptureTarget {
                window_label: "window-second".to_string(),
                target_token: "token-second".to_string(),
            })
        );
    }

    #[test]
    fn global_target_receives_authenticated_session_metadata_before_capture_begins() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main"]);
        let backend = FakeBackend::one_frame();
        init_ready(&runtime, &windows, &backend);
        runtime
            .register_eligible_target("main", "token-main")
            .expect("register target");
        windows.clear_operations();

        runtime
            .start_global_with_registered_target(
                "global-1".to_string(),
                Some(CaptureOrigin {
                    window_label: "main".to_string(),
                }),
                &windows,
                &backend,
            )
            .expect("start global capture");

        let operations = windows.operations();
        assert_eq!(
            operations.first(),
            Some(&WindowOperation::EmitStarted(
                "main".to_string(),
                SessionStartedPayload {
                    session_id: "global-1".to_string(),
                    target_token: "token-main".to_string(),
                },
            ))
        );
        assert!(matches!(operations.get(1), Some(WindowOperation::Hide(label)) if label == "main"));
        let WindowOperation::EmitStarted(_, payload) = &operations[0] else {
            unreachable!()
        };
        let encoded = serde_json::to_string(payload).expect("serialize session start");
        assert_eq!(
            serde_json::from_str::<Value>(&encoded)
                .unwrap()
                .as_object()
                .unwrap()
                .len(),
            2
        );
        assert!(!encoded.contains("chat"));
    }

    #[test]
    fn exact_deactivation_and_window_close_cannot_clear_another_or_newer_target() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-second"]);
        let backend = FakeBackend::one_frame();
        init_ready(&runtime, &windows, &backend);
        runtime
            .register_eligible_target("main", "token-old")
            .expect("register old target");
        runtime
            .register_eligible_target("main", "token-new")
            .expect("replace target");
        runtime
            .register_eligible_target("window-second", "token-second")
            .expect("register second target");

        runtime
            .unregister_eligible_target("main", "token-old")
            .expect("stale unregister is harmless");
        runtime
            .prepare_window_close("window-second", &windows)
            .expect("window close clears its registration");
        runtime
            .start_global_with_registered_target("global-1".to_string(), None, &windows, &backend)
            .expect("start with surviving target");

        assert_eq!(
            runtime
                .lock()
                .expect("runtime lock")
                .coordinator
                .active_state()
                .and_then(|state| state.target.clone()),
            Some(CaptureTarget {
                window_label: "main".to_string(),
                target_token: "token-new".to_string(),
            })
        );
    }

    #[test]
    fn target_registry_rejects_utility_callers_and_prunes_destroyed_windows() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-stale"]);
        let backend = FakeBackend::one_frame();
        init_ready(&runtime, &windows, &backend);

        assert_eq!(
            runtime
                .register_eligible_target(OVERLAY_WINDOW_LABEL, "secret")
                .expect_err("overlay cannot register a chat target")
                .code,
            CaptureErrorCode::UnauthorizedCaller
        );
        runtime
            .register_eligible_target("window-stale", "token-stale")
            .expect("register stale target");
        windows.remove_window_without_callback("window-stale");

        runtime
            .start_global_with_registered_target("global-1".to_string(), None, &windows, &backend)
            .expect("global capture remains available without send target");
        assert!(
            runtime
                .lock()
                .expect("runtime lock")
                .coordinator
                .active_state()
                .is_some_and(|state| state.target.is_none())
        );
    }

    #[test]
    fn stale_wrong_caller_session_and_generation_cannot_consume_or_present_frame() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target", "attacker"]);
        let backend = FakeBackend::one_frame();
        let generation = init_ready(&runtime, &windows, &backend);
        runtime
            .start_composer(
                "main",
                "session-1".to_string(),
                target("window-target"),
                &windows,
                &backend,
            )
            .expect("start capture");

        for (caller, session, candidate_generation, expected) in [
            (
                "attacker",
                "session-1",
                generation,
                CaptureErrorCode::UnauthorizedCaller,
            ),
            (
                OVERLAY_WINDOW_LABEL,
                "stale",
                generation,
                CaptureErrorCode::InvalidSession,
            ),
            (
                OVERLAY_WINDOW_LABEL,
                "session-1",
                generation + 1,
                CaptureErrorCode::InvalidSession,
            ),
        ] {
            assert_eq!(
                runtime
                    .take_frame(caller, session, candidate_generation)
                    .expect_err("invalid frame consumer")
                    .code,
                expected
            );
        }
        assert!(runtime.has_sensitive_buffers().expect("buffer state"));

        let bytes = runtime
            .take_frame(OVERLAY_WINDOW_LABEL, "session-1", generation)
            .expect("valid frame read");
        assert_eq!(bytes, vec![0x5a; 16]);
        assert_eq!(
            runtime
                .frame_presented(OVERLAY_WINDOW_LABEL, "session-1", generation + 1, &windows,)
                .expect_err("stale presentation")
                .code,
            CaptureErrorCode::InvalidSession
        );
        runtime
            .frame_presented(OVERLAY_WINDOW_LABEL, "session-1", generation, &windows)
            .expect("valid presentation");
        assert!(
            matches!(windows.operations().last(), Some(WindowOperation::Show(label)) if label == OVERLAY_WINDOW_LABEL)
        );
    }

    #[test]
    fn start_before_ready_waits_and_busy_retrigger_focuses_without_replacing_session() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let backend = FakeBackend::one_frame();
        let generation = runtime
            .init_overlay("main", &windows)
            .expect("initialize overlay")
            .overlay_generation;
        windows.clear_operations();

        let waiting = runtime
            .start_composer(
                "main",
                "session-1".to_string(),
                target("window-target"),
                &windows,
                &backend,
            )
            .expect("start before ready");
        assert_eq!(waiting.phase, NativeCapturePhase::WaitingForOverlay);
        assert!(windows.operations().is_empty());

        let error = runtime
            .start_composer(
                "main",
                "session-2".to_string(),
                target("window-target"),
                &windows,
                &backend,
            )
            .expect_err("second capture is busy");
        assert_eq!(error.code, CaptureErrorCode::Busy);
        assert_eq!(
            windows.operations(),
            vec![WindowOperation::Focus(OVERLAY_WINDOW_LABEL.into())]
        );

        windows.clear_operations();
        assert_eq!(
            runtime
                .overlay_ready(
                    OVERLAY_WINDOW_LABEL,
                    generation,
                    CAPTURE_PROTOCOL_VERSION,
                    &windows,
                    &backend,
                )
                .expect("ready triggers capture"),
            NativeCapturePhase::FrameAvailable
        );
        assert!(
            matches!(windows.operations().first(), Some(WindowOperation::Hide(label)) if label == "main")
        );
    }

    #[test]
    fn native_capture_failure_restores_origin_and_releases_the_session() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let backend = FakeBackend::one_frame();
        backend.frames.lock().expect("frames lock").clear();
        init_ready(&runtime, &windows, &backend);
        windows.clear_operations();

        let error = runtime
            .start_composer(
                "main",
                "session-1".to_string(),
                target("window-target"),
                &windows,
                &backend,
            )
            .expect_err("capture backend failure");
        assert_eq!(error.code, CaptureErrorCode::CaptureFailed);
        assert_eq!(
            runtime.active_phase().expect("phase"),
            NativeCapturePhase::Idle
        );
        assert!(!runtime.has_sensitive_buffers().expect("buffer state"));
        assert_eq!(
            windows.operations(),
            vec![
                WindowOperation::Hide("main".into()),
                WindowOperation::Hide(OVERLAY_WINDOW_LABEL.into()),
                WindowOperation::Restore("main".into()),
                WindowOperation::EmitEnded(
                    "window-target".into(),
                    SessionEndedPayload {
                        session_id: "session-1".into(),
                        target_token: "target-secret".into(),
                        outcome: super::SessionEndOutcome::Failed,
                        error_code: Some(CaptureErrorCode::CaptureFailed),
                    },
                ),
                WindowOperation::EmitOverlayEnded(
                    OVERLAY_WINDOW_LABEL.into(),
                    OverlaySessionEndedPayload {
                        session_id: "session-1".into(),
                        overlay_generation: 1,
                        outcome: super::SessionEndOutcome::Failed,
                    },
                ),
            ]
        );
    }

    #[test]
    fn pending_frame_replays_metadata_without_consuming_pixels() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let backend = FakeBackend::one_frame();
        let generation = init_ready(&runtime, &windows, &backend);
        runtime
            .start_composer(
                "main",
                "session-1".to_string(),
                target("window-target"),
                &windows,
                &backend,
            )
            .expect("start capture");
        let pending = runtime
            .pending_frame(OVERLAY_WINDOW_LABEL, generation, &windows)
            .expect("query pending frame")
            .expect("pending frame");
        assert_eq!(pending.session_id, "session-1");
        assert_eq!(pending.descriptor.session_id, "session-1");
        assert!(windows.operations().iter().any(
            |operation| matches!(operation, WindowOperation::Emit(_, payload) if payload.session_id == "session-1")
        ));
        assert!(runtime.has_sensitive_buffers().expect("buffer state"));
        runtime
            .take_frame(OVERLAY_WINDOW_LABEL, "session-1", generation)
            .expect("consume frame after metadata replay");
        assert!(
            runtime
                .pending_frame(OVERLAY_WINDOW_LABEL, generation, &windows)
                .expect("query consumed frame")
                .is_none()
        );
        runtime
            .frame_presented(OVERLAY_WINDOW_LABEL, "session-1", generation, &windows)
            .expect("acknowledge presentation");
    }

    #[test]
    fn pending_frame_releases_runtime_before_checking_target_window_availability() {
        let runtime = Arc::new(ScreenCaptureRuntime::new().expect("runtime"));
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        windows.defer_overlay_position_until_presented();
        let backend = FakeBackend::one_frame();
        let generation = init_ready(&runtime, &windows, &backend);
        runtime
            .start_composer(
                "main",
                "session-pending-window-lookup".to_string(),
                target("window-target"),
                &windows,
                &backend,
            )
            .expect("capture frame");
        windows.reenter_runtime_during_next_window_lookup(runtime.clone());

        let pending = runtime
            .pending_frame(OVERLAY_WINDOW_LABEL, generation, &windows)
            .expect("query pending frame")
            .expect("pending frame");

        assert!(pending.can_confirm);
    }

    #[test]
    fn global_session_started_publication_does_not_hold_the_runtime_lock() {
        let runtime = Arc::new(ScreenCaptureRuntime::new().expect("runtime"));
        let windows = FakeWindows::with_windows(&["main"]);
        let backend = FakeBackend::one_frame();
        init_ready(&runtime, &windows, &backend);
        runtime
            .register_eligible_target("main", "target-secret")
            .expect("register target");
        windows.reenter_runtime_during_session_started(runtime.clone());

        let reservation = runtime
            .reserve_global_with_registered_target_capture(
                "session-global-event".to_string(),
                None,
                &windows,
            )
            .expect("session-started publication must not reenter under the runtime lock");

        assert!(reservation.ticket.is_some());
        assert!(windows.operations().iter().any(|operation| {
            matches!(operation, WindowOperation::EmitStarted(label, payload) if label == "main" && payload.session_id == "session-global-event")
        }));
    }

    #[test]
    fn deferred_overlay_position_occurs_only_after_the_frame_is_presented() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        windows.defer_overlay_position_until_presented();
        let backend = FakeBackend::one_frame();
        let generation = init_ready(&runtime, &windows, &backend);
        windows.clear_operations();

        runtime
            .start_composer(
                "main",
                "session-deferred-position".to_string(),
                target("window-target"),
                &windows,
                &backend,
            )
            .expect("capture while overlay remains offscreen");
        assert!(
            !windows
                .operations()
                .iter()
                .any(|operation| matches!(operation, WindowOperation::Position(_, _)))
        );

        runtime
            .take_frame(
                OVERLAY_WINDOW_LABEL,
                "session-deferred-position",
                generation,
            )
            .expect("take captured frame");
        runtime
            .frame_presented(
                OVERLAY_WINDOW_LABEL,
                "session-deferred-position",
                generation,
                &windows,
            )
            .expect("present captured frame");

        let operations = windows.operations();
        let position = operations
            .iter()
            .position(|operation| matches!(operation, WindowOperation::Position(_, _)))
            .expect("position overlay");
        let show = operations
            .iter()
            .position(|operation| matches!(operation, WindowOperation::Show(_)))
            .expect("show overlay");
        assert!(position < show);
    }

    #[test]
    fn page_unavailable_restores_and_clears_without_destroying_its_in_flight_webview() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let backend = FakeBackend::one_frame();
        let generation = init_ready(&runtime, &windows, &backend);
        runtime
            .start_composer(
                "main",
                "session-1".to_string(),
                target("window-target"),
                &windows,
                &backend,
            )
            .expect("start capture");
        windows.clear_operations();

        assert_eq!(
            runtime
                .overlay_unavailable(OVERLAY_WINDOW_LABEL, generation + 1, &windows)
                .expect_err("stale lifecycle message")
                .code,
            CaptureErrorCode::InvalidSession
        );
        assert!(windows.operations().is_empty());
        assert!(runtime.has_sensitive_buffers().expect("buffer state"));

        let retained = runtime
            .overlay_unavailable(OVERLAY_WINDOW_LABEL, generation, &windows)
            .expect("mark page unavailable");
        assert_eq!(retained.overlay_generation, generation);
        assert_eq!(
            runtime.current_overlay_generation().expect("generation"),
            Some(generation)
        );
        assert_eq!(
            runtime.active_phase().expect("phase"),
            NativeCapturePhase::Idle
        );
        assert!(!runtime.has_sensitive_buffers().expect("buffer state"));

        let operations = windows.operations();
        assert!(operations.contains(&WindowOperation::Restore("main".into())));
        assert!(!operations.iter().any(|operation| matches!(
            operation,
            WindowOperation::Destroy(_) | WindowOperation::Create(_)
        )));

        runtime
            .overlay_ready(
                OVERLAY_WINDOW_LABEL,
                generation,
                CAPTURE_PROTOCOL_VERSION,
                &windows,
                &backend,
            )
            .expect("reloaded page rearms retained generation");
    }

    #[test]
    fn bootstrap_failure_retires_the_generation_until_framework_destruction() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main"]);
        let generation = runtime
            .init_overlay("main", &windows)
            .expect("initialize overlay")
            .overlay_generation;

        runtime
            .overlay_bootstrap_failed(OVERLAY_WINDOW_LABEL, generation, &windows)
            .expect("retire failed bootstrap");
        assert_eq!(runtime.current_overlay_generation().unwrap(), None);
        assert_eq!(
            runtime.ensure_overlay_native(&windows)
                .expect_err("destruction must finish before replacement")
                .code,
            CaptureErrorCode::Busy
        );

        windows.destroy_window_as_framework(OVERLAY_WINDOW_LABEL);
        assert!(!runtime
            .overlay_destroyed(OVERLAY_WINDOW_LABEL, &windows)
            .expect("acknowledge retired overlay"));
        let replacement = runtime
            .ensure_overlay_native(&windows)
            .expect("create fresh overlay after destruction");
        assert_eq!(replacement.overlay_generation, generation + 1);
        assert!(windows.window_exists(&overlay_window_label(generation + 1)));
    }

    #[test]
    fn bootstrap_retirement_can_finish_when_platform_destruction_fails() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main"]);
        let generation = runtime
            .init_overlay("main", &windows)
            .expect("initialize overlay")
            .overlay_generation;

        runtime
            .overlay_bootstrap_failed(OVERLAY_WINDOW_LABEL, generation, &windows)
            .expect("retire failed bootstrap");
        runtime
            .finish_overlay_retirement(OVERLAY_WINDOW_LABEL)
            .expect("finish native retirement attempt");

        let replacement = runtime
            .ensure_overlay_native(&windows)
            .expect("create replacement despite stale failed platform window");
        assert_eq!(replacement.overlay_generation, generation + 1);
        assert!(windows.window_exists(&overlay_window_label(generation + 1)));
    }

    #[test]
    fn native_page_load_hook_cleans_an_active_session_before_overlay_reload() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let generation = active_composer(&runtime, &windows, "session-reload");
        runtime
            .overlay_page_load_finished(OVERLAY_WINDOW_LABEL, Some(generation), &windows)
            .expect("record initial page load");
        windows.clear_operations();

        runtime
            .window_page_load_started(OVERLAY_WINDOW_LABEL, Some(generation), &windows)
            .expect("reload starts native cleanup");

        assert_eq!(runtime.active_phase().unwrap(), NativeCapturePhase::Idle);
        assert!(!runtime.has_sensitive_buffers().unwrap());
        assert_eq!(
            runtime.current_overlay_generation().unwrap(),
            Some(generation)
        );
        assert!(
            windows
                .operations()
                .contains(&WindowOperation::Restore("main".into()))
        );
    }

    #[test]
    fn queryless_navigation_cleans_the_current_loaded_overlay_session() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let generation = active_composer(&runtime, &windows, "session-queryless-navigation");
        runtime
            .overlay_page_load_finished(OVERLAY_WINDOW_LABEL, Some(generation), &windows)
            .expect("record initial page load");
        windows.clear_operations();

        runtime
            .window_page_load_started(OVERLAY_WINDOW_LABEL, None, &windows)
            .expect("queryless navigation invalidates the current JavaScript heap");

        assert_eq!(runtime.active_phase().unwrap(), NativeCapturePhase::Idle);
        assert!(!runtime.has_sensitive_buffers().unwrap());
        assert!(
            windows
                .operations()
                .contains(&WindowOperation::Restore("main".into()))
        );
    }

    #[test]
    fn first_overlay_page_load_does_not_cancel_a_waiting_session() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let generation = runtime
            .init_overlay("main", &windows)
            .unwrap()
            .overlay_generation;
        runtime
            .reserve_composer_capture(
                "main",
                "session-initial-load".into(),
                target("window-target"),
                &windows,
            )
            .expect("reserve before initial page finishes");

        runtime
            .window_page_load_started(OVERLAY_WINDOW_LABEL, Some(generation), &windows)
            .expect("first load is not a reload");

        assert_eq!(
            runtime.active_phase().unwrap(),
            NativeCapturePhase::WaitingForOverlay
        );
        assert_eq!(
            runtime.current_overlay_generation().unwrap(),
            Some(generation)
        );
        assert!(!runtime.has_sensitive_buffers().unwrap());
    }

    #[test]
    fn regular_target_reload_prunes_registration_and_releases_delivery_authority() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        runtime
            .register_eligible_target("window-target", "target-secret")
            .expect("register target");
        let (generation, result_id, _) = stored_result(&runtime, &windows, "session-target-reload");
        runtime
            .take_result(
                "window-target",
                "session-target-reload",
                &result_id,
                "target-secret",
                "native-lease".into(),
            )
            .expect("lease result");
        windows.clear_operations();

        runtime
            .window_page_load_started("window-target", None, &windows)
            .expect("target reload cleanup");

        let inner = runtime.lock().expect("runtime lock");
        assert!(inner.eligible_targets.is_empty());
        assert!(inner.active_delivery_lease.is_none());
        drop(inner);
        assert_eq!(
            runtime.active_phase().unwrap(),
            NativeCapturePhase::ResultAvailable
        );
        assert!(runtime.has_sensitive_buffers().unwrap());
        assert!(matches!(
            windows.operations().as_slice(),
            [WindowOperation::EmitTargetUnavailable(label, TargetUnavailablePayload { session_id, overlay_generation })]
                if label == OVERLAY_WINDOW_LABEL
                    && session_id == "session-target-reload"
                    && *overlay_generation == generation
        ));
    }

    #[test]
    fn regular_origin_reload_cancels_and_restores_before_the_old_heap_is_lost() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        active_composer(&runtime, &windows, "session-origin-reload");
        windows.clear_operations();

        runtime
            .window_page_load_started("main", None, &windows)
            .expect("origin reload cleanup");

        assert_eq!(runtime.active_phase().unwrap(), NativeCapturePhase::Idle);
        assert!(!runtime.has_sensitive_buffers().unwrap());
        assert!(
            windows
                .operations()
                .contains(&WindowOperation::Restore("main".into()))
        );
    }

    #[test]
    fn native_destruction_rotates_generation_after_restoration_before_rebuild() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let backend = FakeBackend::one_frame();
        let generation = init_ready(&runtime, &windows, &backend);
        runtime
            .start_composer(
                "main",
                "session-1".to_string(),
                target("window-target"),
                &windows,
                &backend,
            )
            .expect("start capture");
        windows.clear_operations();
        windows.remove_window_without_callback(OVERLAY_WINDOW_LABEL);

        runtime
            .overlay_destroyed(OVERLAY_WINDOW_LABEL, &windows)
            .expect("destroyed hook restores synchronously");
        assert_eq!(
            runtime.active_phase().expect("phase"),
            NativeCapturePhase::Idle
        );
        assert_eq!(
            runtime.current_overlay_generation().expect("generation"),
            None
        );
        assert_eq!(
            windows.operations(),
            vec![
                WindowOperation::Restore("main".into()),
                WindowOperation::EmitEnded(
                    "window-target".into(),
                    SessionEndedPayload {
                        session_id: "session-1".into(),
                        target_token: "target-secret".into(),
                        outcome: super::SessionEndOutcome::Failed,
                        error_code: Some(CaptureErrorCode::OverlayFailed),
                    },
                ),
            ]
        );

        let replacement = runtime
            .ensure_overlay_native(&windows)
            .expect("rebuild overlay on later tick");
        assert_eq!(replacement.overlay_generation, generation + 1);
        let operations = windows.operations();
        let restore = operations
            .iter()
            .position(|operation| operation == &WindowOperation::Restore("main".into()))
            .expect("origin restored");
        let create = operations
            .iter()
            .position(|operation| matches!(operation, WindowOperation::Create(_)))
            .expect("overlay recreated");
        assert!(restore < create);
        let WindowOperation::Create(spec) = &operations[create] else {
            unreachable!();
        };
        assert_eq!(
            spec.label,
            overlay_window_label(replacement.overlay_generation)
        );
        assert!(!spec.visible);
        assert_eq!(
            spec.route,
            format!(
                "{OVERLAY_ROUTE}?overlayGeneration={}",
                replacement.overlay_generation
            )
        );
    }

    #[test]
    fn unexpected_ephemeral_overlay_destruction_stays_lazy() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let backend = FakeBackend::one_frame();
        let generation = init_ready(&runtime, &windows, &backend);
        windows.use_ephemeral_overlay();
        windows.remove_window_without_callback(OVERLAY_WINDOW_LABEL);

        assert!(
            !runtime
                .overlay_destroyed(OVERLAY_WINDOW_LABEL, &windows)
                .expect("ephemeral destruction must not request automatic prewarming")
        );
        assert_eq!(runtime.current_overlay_generation().unwrap(), None);

        let reservation = runtime
            .reserve_composer_capture(
                "main",
                "session-after-unexpected-destruction".into(),
                target("window-target"),
                &windows,
            )
            .expect("the next trigger creates the replacement lazily");
        assert_eq!(reservation.response.overlay_generation, generation + 1);
        assert_eq!(
            reservation.response.phase,
            NativeCapturePhase::WaitingForOverlay
        );
    }

    #[test]
    fn native_close_hook_restores_origin_before_framework_destruction() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let backend = FakeBackend::one_frame();
        let generation = init_ready(&runtime, &windows, &backend);
        runtime
            .start_composer(
                "main",
                "session-1".to_string(),
                target("window-target"),
                &windows,
                &backend,
            )
            .expect("start capture");
        windows.clear_operations();

        runtime
            .prepare_window_close(OVERLAY_WINDOW_LABEL, &windows)
            .expect("close hook cleanup");
        windows.destroy_window_as_framework(OVERLAY_WINDOW_LABEL);
        runtime
            .overlay_destroyed(OVERLAY_WINDOW_LABEL, &windows)
            .expect("destroyed hook rotates generation");
        let replacement = runtime
            .ensure_overlay_native(&windows)
            .expect("later rebuild");
        assert_eq!(replacement.overlay_generation, generation + 1);

        let operations = windows.operations();
        let restore = operations
            .iter()
            .position(|operation| operation == &WindowOperation::Restore("main".into()))
            .expect("origin restored");
        let destroy = operations
            .iter()
            .position(|operation| {
                operation == &WindowOperation::Destroy(OVERLAY_WINDOW_LABEL.into())
            })
            .expect("overlay destroyed");
        let create = operations
            .iter()
            .position(|operation| matches!(operation, WindowOperation::Create(_)))
            .expect("overlay rebuilt");
        assert!(restore < destroy && destroy < create);
    }

    #[test]
    fn closing_a_distinct_target_keeps_capture_and_notifies_overlay_without_secrets() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let (generation, _, _) = stored_result(&runtime, &windows, "session-1");
        windows.clear_operations();

        runtime
            .prepare_window_close("window-target", &windows)
            .expect("target close invalidation");
        windows.destroy_window_as_framework("window-target");

        assert_eq!(
            runtime.active_phase().expect("phase"),
            NativeCapturePhase::ResultAvailable
        );
        assert!(runtime.has_sensitive_buffers().expect("buffer state"));
        let operations = windows.operations();
        assert_eq!(
            operations,
            vec![
                WindowOperation::EmitTargetUnavailable(
                    OVERLAY_WINDOW_LABEL.into(),
                    TargetUnavailablePayload {
                        session_id: "session-1".into(),
                        overlay_generation: generation,
                    },
                ),
                WindowOperation::Destroy("window-target".into()),
            ]
        );
        let WindowOperation::EmitTargetUnavailable(_, payload) = &operations[0] else {
            unreachable!()
        };
        let encoded = serde_json::to_string(payload).expect("target event JSON");
        assert!(!encoded.contains("target-secret"));
    }

    #[test]
    fn overlay_failures_are_allowlisted_and_cancel_requires_generation_for_overlay_callers() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let backend = FakeBackend::one_frame();
        let generation = init_ready(&runtime, &windows, &backend);
        runtime
            .start_composer(
                "main",
                "session-1".to_string(),
                target("window-target"),
                &windows,
                &backend,
            )
            .expect("start capture");

        assert_eq!(
            runtime
                .fail_from_overlay(
                    OVERLAY_WINDOW_LABEL,
                    "session-1",
                    generation,
                    "permission_denied",
                    "untrusted",
                    &windows,
                )
                .expect_err("frontend cannot choose native error codes")
                .code,
            CaptureErrorCode::UnauthorizedCaller
        );
        assert_eq!(
            runtime.active_phase().expect("phase"),
            NativeCapturePhase::FrameAvailable
        );
        assert_eq!(
            runtime
                .cancel_from_window(OVERLAY_WINDOW_LABEL, "session-1", None, &windows)
                .expect_err("overlay cancellation requires generation")
                .code,
            CaptureErrorCode::InvalidSession
        );
        runtime
            .fail_from_overlay(
                OVERLAY_WINDOW_LABEL,
                "session-1",
                generation,
                "frame_decode_failed",
                &"x".repeat(1024),
                &windows,
            )
            .expect("allowlisted overlay failure");
        assert_eq!(
            runtime.active_phase().expect("phase"),
            NativeCapturePhase::Idle
        );
        assert!(!runtime.has_sensitive_buffers().expect("buffer state"));
    }

    #[test]
    fn composer_authority_is_the_injected_regular_window_label_not_the_loaded_route() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&[
            "main",
            "window-target",
            "about",
            "media-preview-warm",
            "media-preview-1",
        ]);
        let backend = FakeBackend::one_frame();
        init_ready(&runtime, &windows, &backend);

        for label in [
            "about",
            "media-preview-warm",
            "media-preview-1",
            OVERLAY_WINDOW_LABEL,
        ] {
            assert_eq!(
                runtime
                    .start_composer(
                        label,
                        format!("session-{label}"),
                        target("window-target"),
                        &windows,
                        &backend,
                    )
                    .expect_err("non-regular label must not gain authority by navigation")
                    .code,
                CaptureErrorCode::UnauthorizedCaller
            );
        }
        assert_eq!(
            runtime
                .start_composer(
                    "main",
                    "session-preview-target".to_string(),
                    target("media-preview-1"),
                    &windows,
                    &backend,
                )
                .expect_err("non-regular delivery target is rejected")
                .code,
            CaptureErrorCode::TargetUnavailable
        );
        assert_eq!(
            runtime.active_phase().expect("phase"),
            NativeCapturePhase::Idle
        );
    }

    #[test]
    fn capture_init_rejects_every_non_regular_window_label() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows =
            FakeWindows::with_windows(&["main", "window-chat", "about", "media-preview-1"]);
        for label in [
            "about",
            "media-preview-1",
            "arbitrary",
            OVERLAY_WINDOW_LABEL,
        ] {
            assert_eq!(
                runtime
                    .init_overlay(label, &windows)
                    .expect_err("untrusted label cannot initialize native overlay")
                    .code,
                CaptureErrorCode::UnauthorizedCaller
            );
        }
        runtime
            .init_overlay("window-chat", &windows)
            .expect("regular auxiliary window can initialize capture");
    }

    #[test]
    fn result_publication_is_metadata_only_and_delivery_is_single_lease_retryable() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target", "attacker"]);
        let (generation, result_id, bytes) = stored_result(&runtime, &windows, "session-result");
        windows.clear_operations();

        runtime
            .publish_result(
                OVERLAY_WINDOW_LABEL,
                "session-result",
                &result_id,
                generation,
                &windows,
            )
            .expect("publish metadata");
        let operations = windows.operations();
        let WindowOperation::EmitResult(label, payload) = &operations[0] else {
            panic!("result event expected")
        };
        assert_eq!(label, "window-target");
        assert_eq!(payload.target_token, "target-secret");
        assert_eq!(payload.descriptor.result_id, result_id);
        let object = serde_json::to_value(payload).expect("result JSON");
        assert_eq!(object.as_object().expect("result object").len(), 2);
        assert!(
            !serde_json::to_string(payload)
                .unwrap()
                .contains("overlayGeneration")
        );

        assert_eq!(
            runtime
                .take_result(
                    "attacker",
                    "session-result",
                    &result_id,
                    "target-secret",
                    "native-lease-a".into(),
                )
                .expect_err("wrong target label")
                .code,
            CaptureErrorCode::UnauthorizedCaller
        );
        assert_eq!(
            runtime
                .take_result(
                    "window-target",
                    "session-result",
                    &result_id,
                    "wrong-token",
                    "native-lease-a".into(),
                )
                .expect_err("wrong target token")
                .code,
            CaptureErrorCode::UnauthorizedCaller
        );
        assert_eq!(
            runtime
                .take_result(
                    "window-target",
                    "session-result",
                    &result_id,
                    "target-secret",
                    "native-lease-a".into(),
                )
                .expect("lease result"),
            bytes
        );
        assert_eq!(
            runtime
                .take_result(
                    "window-target",
                    "session-result",
                    &result_id,
                    "target-secret",
                    "native-lease-b".into(),
                )
                .expect_err("concurrent read")
                .code,
            CaptureErrorCode::Busy
        );
        runtime
            .release_result(
                "window-target",
                "session-result",
                &result_id,
                "target-secret",
                &windows,
            )
            .expect("release failed delivery");
        assert!(windows.operations().iter().any(|operation| {
            matches!(
                operation,
                WindowOperation::EmitDeliveryFailed(label, payload)
                    if label == OVERLAY_WINDOW_LABEL
                        && payload.session_id == "session-result"
                        && payload.overlay_generation == generation
            )
        }));
        assert_eq!(
            runtime.active_phase().unwrap(),
            NativeCapturePhase::ResultAvailable
        );
        runtime
            .take_result(
                "window-target",
                "session-result",
                &result_id,
                "target-secret",
                "native-lease-b".into(),
            )
            .expect("retry result");
        windows.clear_operations();
        runtime
            .ack_result(
                "window-target",
                "session-result",
                &result_id,
                "target-secret",
                &windows,
            )
            .expect("ack result");
        assert_eq!(runtime.active_phase().unwrap(), NativeCapturePhase::Idle);
        assert!(!runtime.has_sensitive_buffers().unwrap());
        let operations = windows.operations();
        let restore = operations
            .iter()
            .position(|operation| operation == &WindowOperation::Restore("main".into()))
            .expect("origin restored");
        let ended = operations
            .iter()
            .position(|operation| matches!(operation, WindowOperation::EmitEnded(_, _)))
            .expect("target notified");
        assert!(restore < ended);

        runtime
            .ack_result(
                "window-target",
                "session-result",
                &result_id,
                "target-secret",
                &windows,
            )
            .expect("duplicate ACK is idempotent");
    }

    #[test]
    fn ack_remains_idempotent_when_final_overlay_hide_failed_after_cleanup() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let (_, result_id, _) = stored_result(&runtime, &windows, "session-hide-error");
        runtime
            .take_result(
                "window-target",
                "session-hide-error",
                &result_id,
                "target-secret",
                "native-lease".into(),
            )
            .expect("lease result");
        windows.fail_next_hide(OVERLAY_WINDOW_LABEL);
        assert_eq!(
            runtime
                .ack_result(
                    "window-target",
                    "session-hide-error",
                    &result_id,
                    "target-secret",
                    &windows,
                )
                .expect_err("hide error is reported")
                .code,
            CaptureErrorCode::OverlayFailed
        );
        assert_eq!(runtime.active_phase().unwrap(), NativeCapturePhase::Idle);
        runtime
            .ack_result(
                "window-target",
                "session-hide-error",
                &result_id,
                "target-secret",
                &windows,
            )
            .expect("retry matches completed delivery tombstone");
    }

    #[test]
    fn save_and_copy_failures_or_cancel_preserve_result_until_success() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let (generation, result_id, bytes) = stored_result(&runtime, &windows, "session-save");

        let cancelled = FakeExports {
            selected_path: Ok(None),
            ..FakeExports::successful()
        };
        assert_eq!(
            runtime
                .save_result(
                    OVERLAY_WINDOW_LABEL,
                    "session-save",
                    &result_id,
                    generation,
                    &cancelled,
                    &windows,
                )
                .expect("dialog cancel"),
            SaveCaptureOutcome::Cancelled
        );
        assert_eq!(
            runtime.active_phase().unwrap(),
            NativeCapturePhase::ResultAvailable
        );
        assert!(runtime.has_sensitive_buffers().unwrap());

        let write_failure = FakeExports {
            write_error: Some(CaptureError::new(CaptureErrorCode::SaveFailed, "denied")),
            ..FakeExports::successful()
        };
        assert_eq!(
            runtime
                .save_result(
                    OVERLAY_WINDOW_LABEL,
                    "session-save",
                    &result_id,
                    generation,
                    &write_failure,
                    &windows,
                )
                .expect_err("save failure")
                .code,
            CaptureErrorCode::SaveFailed
        );
        assert_eq!(
            runtime.active_phase().unwrap(),
            NativeCapturePhase::ResultAvailable
        );

        let saved = FakeExports::successful();
        windows.clear_operations();
        assert_eq!(
            runtime
                .save_result(
                    OVERLAY_WINDOW_LABEL,
                    "session-save",
                    &result_id,
                    generation,
                    &saved,
                    &windows,
                )
                .expect("save success"),
            SaveCaptureOutcome::Saved
        );
        assert_eq!(saved.writes.lock().unwrap()[0], bytes);
        assert_eq!(runtime.active_phase().unwrap(), NativeCapturePhase::Idle);
        assert!(windows.operations().iter().any(|operation| matches!(
            operation,
            WindowOperation::EmitEnded(
                _,
                SessionEndedPayload {
                    outcome: super::SessionEndOutcome::Saved,
                    ..
                }
            )
        )));
        let operations = windows.operations();
        let WindowOperation::EmitOverlayEnded(_, overlay_end) = operations.last().unwrap() else {
            panic!("overlay terminal event must follow cleanup")
        };
        let encoded = serde_json::to_string(overlay_end).expect("overlay terminal event JSON");
        assert!(!encoded.contains("target-secret"));
        assert!(!encoded.contains("bytes"));

        let (generation, result_id, bytes) = stored_result(&runtime, &windows, "session-copy");
        let copy_failure = FakeExports {
            copy_error: Some(CaptureError::new(
                CaptureErrorCode::ClipboardFailed,
                "unavailable",
            )),
            ..FakeExports::successful()
        };
        assert_eq!(
            runtime
                .copy_result(
                    OVERLAY_WINDOW_LABEL,
                    "session-copy",
                    &result_id,
                    generation,
                    &copy_failure,
                    &windows,
                )
                .expect_err("copy failure")
                .code,
            CaptureErrorCode::ClipboardFailed
        );
        assert_eq!(
            runtime.active_phase().unwrap(),
            NativeCapturePhase::ResultAvailable
        );

        let copied = FakeExports::successful();
        windows.clear_operations();
        runtime
            .copy_result(
                OVERLAY_WINDOW_LABEL,
                "session-copy",
                &result_id,
                generation,
                &copied,
                &windows,
            )
            .expect("copy success");
        assert_eq!(copied.copies.lock().unwrap()[0], bytes);
        assert_eq!(runtime.active_phase().unwrap(), NativeCapturePhase::Idle);
        assert!(windows.operations().iter().any(|operation| matches!(
            operation,
            WindowOperation::EmitEnded(
                _,
                SessionEndedPayload {
                    outcome: super::SessionEndOutcome::Copied,
                    ..
                }
            )
        )));
    }

    #[test]
    fn slow_clipboard_export_does_not_hold_the_runtime_mutex() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let (generation, result_id, _) =
            stored_result(&runtime, &windows, "session-nonblocking-copy");
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let exports = BlockingCopyExport {
            entered: entered_tx,
            release: Mutex::new(release_rx),
        };

        std::thread::scope(|scope| {
            let copy = scope.spawn(|| {
                runtime.copy_result(
                    OVERLAY_WINDOW_LABEL,
                    "session-nonblocking-copy",
                    &result_id,
                    generation,
                    &exports,
                    &windows,
                )
            });
            entered_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("copy entered platform adapter");

            let (probe_tx, probe_rx) = mpsc::channel();
            let runtime_ref = &runtime;
            scope.spawn(move || {
                probe_tx
                    .send(runtime_ref.active_phase())
                    .expect("send runtime probe");
            });
            let probe = probe_rx.recv_timeout(Duration::from_millis(100));
            release_tx.send(()).expect("release platform copy");

            assert_eq!(
                probe
                    .expect("runtime mutex must remain available during platform copy")
                    .expect("runtime probe"),
                NativeCapturePhase::ResultAvailable
            );
            copy.join()
                .expect("copy thread")
                .expect("copy completes after release");
        });
    }

    #[test]
    fn overlay_can_discard_a_stale_render_and_submit_a_replacement() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let (generation, result_id, _) = stored_result(&runtime, &windows, "session-replace");
        assert_eq!(
            runtime
                .discard_result("attacker", "session-replace", &result_id, generation)
                .expect_err("only overlay can discard")
                .code,
            CaptureErrorCode::UnauthorizedCaller
        );
        assert_eq!(
            runtime
                .discard_result(
                    OVERLAY_WINDOW_LABEL,
                    "session-replace",
                    &result_id,
                    generation,
                )
                .expect("discard old render"),
            NativeCapturePhase::Active
        );
        let replacement = runtime
            .store_result(
                OVERLAY_WINDOW_LABEL,
                "session-replace",
                generation,
                CaptureResultSubmission {
                    result_id: "result-new".into(),
                    filename: "Plain-capture-2.png".into(),
                    width: 2,
                    height: 2,
                    bytes: png(2, 2),
                },
            )
            .expect("replacement render");
        assert_eq!(replacement.result_id, "result-new");
    }

    #[test]
    fn target_webview_invalidation_is_authorized_idempotent_and_releases_delivery() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target", "attacker"]);
        let (generation, result_id, _) = stored_result(&runtime, &windows, "session-deactivate");
        runtime
            .take_result(
                "window-target",
                "session-deactivate",
                &result_id,
                "target-secret",
                "native-lease".into(),
            )
            .expect("lease result");
        assert_eq!(
            runtime
                .invalidate_target_from_window(
                    "attacker",
                    "session-deactivate",
                    "target-secret",
                    &windows,
                )
                .expect_err("wrong window")
                .code,
            CaptureErrorCode::UnauthorizedCaller
        );
        assert_eq!(
            runtime
                .invalidate_target_from_window(
                    "window-target",
                    "session-deactivate",
                    "wrong-token",
                    &windows,
                )
                .expect_err("wrong token")
                .code,
            CaptureErrorCode::UnauthorizedCaller
        );
        windows.clear_operations();
        runtime
            .invalidate_target_from_window(
                "window-target",
                "session-deactivate",
                "target-secret",
                &windows,
            )
            .expect("invalidate frozen target");
        assert_eq!(
            runtime.active_phase().unwrap(),
            NativeCapturePhase::ResultAvailable
        );
        assert!(runtime.has_sensitive_buffers().unwrap());
        assert!(matches!(
          windows.operations().as_slice(),
          [WindowOperation::EmitTargetUnavailable(label, TargetUnavailablePayload { session_id, overlay_generation })]
            if label == OVERLAY_WINDOW_LABEL
              && session_id == "session-deactivate"
              && *overlay_generation == generation
        ));

        windows.clear_operations();
        runtime
            .invalidate_target_from_window(
                "window-target",
                "session-deactivate",
                "target-secret",
                &windows,
            )
            .expect("duplicate invalidation rearms metadata notification");
        assert_eq!(windows.operations().len(), 1);
        assert_eq!(
            runtime
                .publish_result(
                    OVERLAY_WINDOW_LABEL,
                    "session-deactivate",
                    &result_id,
                    generation,
                    &windows,
                )
                .expect_err("invalidated result cannot be delivered")
                .code,
            CaptureErrorCode::TargetUnavailable
        );
    }

    #[test]
    fn delivery_abort_is_idempotent_before_or_after_an_ambiguous_result_take() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let (generation, result_id, _) =
            stored_result(&runtime, &windows, "session-abort-delivery");
        windows.clear_operations();

        runtime
            .release_result(
                "window-target",
                "session-abort-delivery",
                &result_id,
                "target-secret",
                &windows,
            )
            .expect("abort before lease is safe");
        runtime
            .take_result(
                "window-target",
                "session-abort-delivery",
                &result_id,
                "target-secret",
                "native-lease".into(),
            )
            .expect("lease after retry notification");
        runtime
            .release_result(
                "window-target",
                "session-abort-delivery",
                &result_id,
                "target-secret",
                &windows,
            )
            .expect("abort an acquired lease");
        runtime
            .release_result(
                "window-target",
                "session-abort-delivery",
                &result_id,
                "target-secret",
                &windows,
            )
            .expect("duplicate abort is safe");

        assert_eq!(
            runtime.active_phase().unwrap(),
            NativeCapturePhase::ResultAvailable
        );
        assert!(runtime.has_sensitive_buffers().unwrap());
        assert_eq!(
            windows
                .operations()
                .iter()
                .filter(|operation| matches!(
                    operation,
                    WindowOperation::EmitDeliveryFailed(label, payload)
                        if label == OVERLAY_WINDOW_LABEL
                            && payload.session_id == "session-abort-delivery"
                            && payload.overlay_generation == generation
                ))
                .count(),
            3
        );
    }

    #[test]
    fn cleanup_restores_the_exact_minimized_and_unfocused_origin_state() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let prior = CaptureWindowState {
            visible: true,
            minimized: true,
            focused: false,
        };
        windows.set_window_state("main", prior);
        let generation = active_composer(&runtime, &windows, "session-exact-state");

        runtime
            .cancel_from_window(
                OVERLAY_WINDOW_LABEL,
                "session-exact-state",
                Some(generation),
                &windows,
            )
            .expect("cancel and restore");

        assert_eq!(windows.window_state("main"), Some(prior));
        assert_eq!(windows.restore_requests(), vec![("main".into(), prior)]);
        assert!(
            !windows
                .operations()
                .contains(&WindowOperation::Focus("main".into()))
        );
    }

    #[test]
    fn transient_restore_failure_retries_with_bounded_backoff_after_state_cleanup() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let generation = active_composer(&runtime, &windows, "session-retry-restore");
        windows.fail_restores("main", 2);

        runtime
            .cancel_from_window(
                OVERLAY_WINDOW_LABEL,
                "session-retry-restore",
                Some(generation),
                &windows,
            )
            .expect_err("first restore failure is observable");
        assert_eq!(runtime.active_phase().unwrap(), NativeCapturePhase::Idle);
        assert!(!runtime.has_sensitive_buffers().unwrap());
        assert!(runtime.has_pending_window_actions().unwrap());
        assert_eq!(windows.scheduled_retries(), vec![Duration::from_millis(50)]);

        runtime
            .retry_pending_window_actions(&windows)
            .expect_err("second restore failure");
        assert_eq!(
            windows.scheduled_retries(),
            vec![Duration::from_millis(50), Duration::from_millis(200)]
        );
        runtime
            .retry_pending_window_actions(&windows)
            .expect("third restore attempt succeeds");
        assert!(!runtime.has_pending_window_actions().unwrap());
        assert!(windows.window_state("main").unwrap().visible);
    }

    #[test]
    fn restore_retry_exhaustion_transitions_to_slow_recovery_until_origin_is_gone() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let generation = active_composer(&runtime, &windows, "session-exhaust-restore");
        windows.fail_restores("main", 4);

        runtime
            .cancel_from_window(
                OVERLAY_WINDOW_LABEL,
                "session-exhaust-restore",
                Some(generation),
                &windows,
            )
            .expect_err("initial restore failure");
        runtime
            .retry_pending_window_actions(&windows)
            .expect_err("retry one");
        runtime
            .retry_pending_window_actions(&windows)
            .expect_err("retry two");
        let exhausted = runtime
            .retry_pending_window_actions(&windows)
            .expect_err("fast retry budget transitions to recovery");

        assert_eq!(exhausted.code, CaptureErrorCode::OverlayFailed);
        assert!(runtime.has_pending_window_actions().unwrap());
        assert_eq!(
            windows.scheduled_retries(),
            vec![
                Duration::from_millis(50),
                Duration::from_millis(200),
                Duration::from_millis(500),
                Duration::from_secs(5),
            ]
        );
        assert_eq!(runtime.active_phase().unwrap(), NativeCapturePhase::Idle);
        assert!(!runtime.has_sensitive_buffers().unwrap());

        windows.remove_window_without_callback("main");
        runtime
            .retry_pending_window_actions(&windows)
            .expect("a definitively gone origin releases restoration metadata");
        assert!(!runtime.has_pending_window_actions().unwrap());
    }

    #[test]
    fn overlay_hide_failure_destroys_visible_pixels_then_rebuilds_hidden() {
        let runtime = ScreenCaptureRuntime::new().expect("runtime");
        let windows = FakeWindows::with_windows(&["main", "window-target"]);
        let (generation, _, _) = stored_result(&runtime, &windows, "session-overlay-hide");
        windows.fail_next_hide(OVERLAY_WINDOW_LABEL);

        runtime
            .cancel_from_window(
                OVERLAY_WINDOW_LABEL,
                "session-overlay-hide",
                Some(generation),
                &windows,
            )
            .expect_err("hide failure is observable after fail-closed destruction");

        assert!(windows.window_exists(OVERLAY_WINDOW_LABEL));
        assert!(
            !windows
                .operations()
                .contains(&WindowOperation::Destroy(OVERLAY_WINDOW_LABEL.into()))
        );
        assert!(!runtime.has_sensitive_buffers().unwrap());
        assert_eq!(runtime.active_phase().unwrap(), NativeCapturePhase::Idle);
        assert!(runtime.has_pending_window_actions().unwrap());
        assert_eq!(windows.scheduled_retries(), vec![Duration::from_millis(50)]);

        windows.run_deferred_destructions();
        assert!(!windows.window_exists(OVERLAY_WINDOW_LABEL));
        runtime
            .retry_pending_window_actions(&windows)
            .expect_err("first retry safely promotes the missing overlay to rebuild");
        runtime
            .retry_pending_window_actions(&windows)
            .expect("rebuild hidden overlay");
        let replacement_label = overlay_window_label(generation + 1);
        assert!(windows.window_exists(&replacement_label));
        assert!(
            !windows
                .operations()
                .contains(&WindowOperation::Hide(replacement_label))
        );
        assert!(!runtime.has_pending_window_actions().unwrap());
        assert_eq!(
            runtime.current_overlay_generation().unwrap(),
            Some(generation + 1)
        );
    }
}
