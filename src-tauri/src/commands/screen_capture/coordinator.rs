use std::sync::Arc;

use super::buffers::SessionBuffers;
use super::contract::{
    CaptureError, CaptureErrorCode, CaptureOrigin, CaptureRequest, CaptureResultDescriptor,
    CaptureTarget, CaptureTriggerKind, CapturedFrame, CapturedFrameDescriptor, NativeCapturePhase,
};
use super::session::CaptureSessionGuard;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureCallerRole {
    Target,
    Overlay,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "error", rename_all = "snake_case")]
pub enum TerminalOutcome {
    Completed,
    Cancelled,
    Failed(CaptureError),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureCleanup {
    pub session_id: String,
    pub overlay_generation: u64,
    pub origin_window_label: Option<String>,
    pub target_window_label: Option<String>,
    pub target_token: Option<String>,
    pub outcome: TerminalOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureState {
    pub session_id: String,
    pub phase: NativeCapturePhase,
    pub overlay_generation: u64,
    pub origin: Option<CaptureOrigin>,
    pub target: Option<CaptureTarget>,
}

#[derive(Debug)]
struct ActiveCaptureSession {
    request: CaptureRequest,
    overlay_generation: u64,
    phase: NativeCapturePhase,
    buffers: SessionBuffers,
    terminal_outcome: Option<TerminalOutcome>,
}

impl ActiveCaptureSession {
    fn state(&self) -> CaptureState {
        CaptureState {
            session_id: self.request.session_id.clone(),
            phase: self.phase,
            overlay_generation: self.overlay_generation,
            origin: self.request.origin.clone(),
            target: self.request.target.clone(),
        }
    }

    fn cleanup(&self) -> Result<CaptureCleanup, CaptureError> {
        let outcome = self.terminal_outcome.clone().ok_or_else(|| {
            CaptureError::new(
                CaptureErrorCode::InvalidPhase,
                "capture session does not have a terminal outcome",
            )
        })?;
        Ok(CaptureCleanup {
            session_id: self.request.session_id.clone(),
            overlay_generation: self.overlay_generation,
            origin_window_label: self
                .request
                .origin
                .as_ref()
                .map(|origin| origin.window_label.clone()),
            target_window_label: self
                .request
                .target
                .as_ref()
                .map(|target| target.window_label.clone()),
            target_token: self
                .request
                .target
                .as_ref()
                .map(|target| target.target_token.clone()),
            outcome,
        })
    }
}

/// Pure native capture state. Command handlers must pass labels obtained from
/// Tauri's invoking window rather than accepting a label supplied in JSON.
///
/// The type deliberately requires `&mut self`; the eventual managed state must
/// put it behind one mutex so transitions and buffer leases remain serialized.
#[derive(Debug)]
pub struct CaptureCoordinator {
    overlay_window_label: String,
    ready_overlay_generation: Option<u64>,
    session_guard: CaptureSessionGuard,
    active: Option<ActiveCaptureSession>,
}

impl CaptureCoordinator {
    pub fn new(overlay_window_label: impl Into<String>) -> Result<Self, CaptureError> {
        let overlay_window_label = overlay_window_label.into();
        if overlay_window_label.trim().is_empty() {
            return Err(CaptureError::new(
                CaptureErrorCode::OverlayFailed,
                "capture overlay requires a window label",
            ));
        }
        Ok(Self {
            overlay_window_label,
            ready_overlay_generation: None,
            session_guard: CaptureSessionGuard::default(),
            active: None,
        })
    }

    pub fn phase(&self) -> NativeCapturePhase {
        self.active
            .as_ref()
            .map(|session| session.phase)
            .unwrap_or(NativeCapturePhase::Idle)
    }

    pub fn replace_overlay_window_label(
        &mut self,
        overlay_window_label: impl Into<String>,
    ) -> Result<(), CaptureError> {
        let overlay_window_label = overlay_window_label.into();
        if overlay_window_label.trim().is_empty() {
            return Err(CaptureError::new(
                CaptureErrorCode::OverlayFailed,
                "capture overlay requires a window label",
            ));
        }
        if self.active.is_some() || self.ready_overlay_generation.is_some() {
            return Err(CaptureError::new(
                CaptureErrorCode::Busy,
                "capture overlay identity cannot change while it is active",
            ));
        }
        self.overlay_window_label = overlay_window_label;
        Ok(())
    }

    pub fn active_session_id(&self) -> Option<&str> {
        self.session_guard.active_session_id()
    }

    pub fn active_state(&self) -> Option<CaptureState> {
        self.active.as_ref().map(ActiveCaptureSession::state)
    }

    #[cfg(test)]
    pub fn has_sensitive_buffers(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|session| session.buffers.has_frame() || session.buffers.has_result())
    }

    /// Records a prewarmed overlay readiness generation. If a capture already
    /// waits for this exact generation, readiness opens the hiding/capture gate.
    pub fn note_overlay_ready(
        &mut self,
        caller_window_label: &str,
        generation: u64,
    ) -> Result<NativeCapturePhase, CaptureError> {
        self.require_overlay_label(caller_window_label)?;
        validate_generation(generation)?;

        if let Some(active) = self.active.as_mut() {
            if active.overlay_generation != generation {
                return Err(stale_generation_error());
            }
            self.ready_overlay_generation = Some(generation);
            if active.phase == NativeCapturePhase::WaitingForOverlay {
                active.phase = NativeCapturePhase::HidingOrigin;
            }
            return Ok(active.phase);
        }

        if self
            .ready_overlay_generation
            .is_some_and(|ready| ready > generation)
        {
            return Err(stale_generation_error());
        }
        self.ready_overlay_generation = Some(generation);
        Ok(NativeCapturePhase::Idle)
    }

    /// Invalidates a destroyed/reloaded overlay generation. An active session
    /// using it immediately enters restoring with all native pixels cleared.
    pub fn note_overlay_unavailable(
        &mut self,
        caller_window_label: &str,
        generation: u64,
    ) -> Result<Option<CaptureCleanup>, CaptureError> {
        self.require_overlay_label(caller_window_label)?;
        validate_generation(generation)?;
        if self.ready_overlay_generation == Some(generation) {
            self.ready_overlay_generation = None;
        }
        let active_generation = self.active.as_ref().map(|active| active.overlay_generation);
        if active_generation != Some(generation) {
            return Ok(None);
        }
        if self.phase() == NativeCapturePhase::Restoring {
            return self
                .active
                .as_ref()
                .map(ActiveCaptureSession::cleanup)
                .transpose();
        }
        self.enter_terminal(TerminalOutcome::Failed(CaptureError::new(
            CaptureErrorCode::OverlayFailed,
            "capture overlay became unavailable",
        )))
        .map(Some)
    }

    pub fn start_from_window(
        &mut self,
        caller_window_label: &str,
        request: CaptureRequest,
        overlay_generation: u64,
    ) -> Result<CaptureState, CaptureError> {
        request.validate()?;
        if request.trigger != CaptureTriggerKind::Composer {
            return Err(unauthorized_error(
                "only native shortcut handling may start a global capture",
            ));
        }
        let origin_matches = request
            .origin
            .as_ref()
            .is_some_and(|origin| origin.window_label == caller_window_label);
        if caller_window_label == self.overlay_window_label || !origin_matches {
            return Err(unauthorized_error(
                "invoking window does not own the capture origin",
            ));
        }
        self.start(request, overlay_generation)
    }

    pub fn start_global(
        &mut self,
        request: CaptureRequest,
        overlay_generation: u64,
    ) -> Result<CaptureState, CaptureError> {
        request.validate()?;
        if request.trigger != CaptureTriggerKind::Global {
            return Err(unauthorized_error(
                "native shortcut handling requires a global capture request",
            ));
        }
        self.start(request, overlay_generation)
    }

    fn start(
        &mut self,
        request: CaptureRequest,
        overlay_generation: u64,
    ) -> Result<CaptureState, CaptureError> {
        validate_generation(overlay_generation)?;
        if request
            .origin
            .as_ref()
            .is_some_and(|origin| origin.window_label == self.overlay_window_label)
            || request
                .target
                .as_ref()
                .is_some_and(|target| target.window_label == self.overlay_window_label)
        {
            return Err(unauthorized_error(
                "capture overlay cannot be an origin or delivery target",
            ));
        }
        if self
            .ready_overlay_generation
            .is_some_and(|ready| ready > overlay_generation)
        {
            return Err(stale_generation_error());
        }

        let buffers = SessionBuffers::new(request.session_id.clone())?;
        self.session_guard.start(&request.session_id)?;
        let phase = if self.ready_overlay_generation == Some(overlay_generation) {
            NativeCapturePhase::HidingOrigin
        } else {
            NativeCapturePhase::WaitingForOverlay
        };
        let active = ActiveCaptureSession {
            request,
            overlay_generation,
            phase,
            buffers,
            terminal_outcome: None,
        };
        let state = active.state();
        self.active = Some(active);
        Ok(state)
    }

    pub fn authorize_caller(
        &self,
        session_id: &str,
        caller_window_label: &str,
        role: CaptureCallerRole,
    ) -> Result<(), CaptureError> {
        let active = self.require_session(session_id)?;
        let authorized = match role {
            CaptureCallerRole::Overlay => caller_window_label == self.overlay_window_label,
            CaptureCallerRole::Target => active
                .request
                .target
                .as_ref()
                .is_some_and(|target| target.window_label == caller_window_label),
        };
        if !authorized {
            return Err(unauthorized_error(
                "invoking window does not have the required capture role",
            ));
        }
        Ok(())
    }

    pub fn begin_capture(&mut self, session_id: &str) -> Result<(), CaptureError> {
        self.require_phase(session_id, NativeCapturePhase::HidingOrigin)?;
        self.require_session_mut(session_id)?.phase = NativeCapturePhase::Capturing;
        Ok(())
    }

    pub fn store_frame(
        &mut self,
        session_id: &str,
        frame: CapturedFrame,
    ) -> Result<(), CaptureError> {
        self.require_phase(session_id, NativeCapturePhase::Capturing)?;
        let active = self.require_session_mut(session_id)?;
        active.buffers.store_frame(frame)?;
        active.phase = NativeCapturePhase::FrameAvailable;
        Ok(())
    }

    pub fn take_frame(
        &mut self,
        caller_window_label: &str,
        session_id: &str,
        overlay_generation: u64,
    ) -> Result<(CapturedFrameDescriptor, Vec<u8>), CaptureError> {
        self.require_overlay_session(caller_window_label, session_id, overlay_generation)?;
        self.require_phase(session_id, NativeCapturePhase::FrameAvailable)?;
        let active = self.require_session_mut(session_id)?;
        let frame = active.buffers.take_frame(session_id)?;
        active.phase = NativeCapturePhase::AwaitingPresentation;
        Ok(frame)
    }

    /// Returns metadata for a frame that remains owned by native memory. This
    /// lets a freshly initialized overlay recover publication without consuming
    /// the one-shot pixel buffer or trusting session identifiers from JS.
    pub fn pending_frame(
        &self,
        caller_window_label: &str,
        overlay_generation: u64,
    ) -> Result<Option<(CaptureState, CapturedFrameDescriptor)>, CaptureError> {
        self.require_overlay_label(caller_window_label)?;
        validate_generation(overlay_generation)?;
        let Some(active) = self.active.as_ref() else {
            return Ok(None);
        };
        if active.overlay_generation != overlay_generation {
            return Err(stale_generation_error());
        }
        if active.phase != NativeCapturePhase::FrameAvailable {
            return Ok(None);
        }
        Ok(Some((
            active.state(),
            active
                .buffers
                .frame_descriptor(&active.request.session_id)?,
        )))
    }

    pub fn frame_presented(
        &mut self,
        caller_window_label: &str,
        session_id: &str,
        overlay_generation: u64,
    ) -> Result<(), CaptureError> {
        self.require_overlay_session(caller_window_label, session_id, overlay_generation)?;
        self.require_phase(session_id, NativeCapturePhase::AwaitingPresentation)?;
        self.require_session_mut(session_id)?.phase = NativeCapturePhase::Active;
        Ok(())
    }

    /// Stores a validated raw PNG. The command layer must construct the
    /// descriptor with a fresh native-generated result ID rather than trusting
    /// a result ID supplied by the overlay.
    #[cfg(test)]
    pub fn store_result(
        &mut self,
        caller_window_label: &str,
        session_id: &str,
        overlay_generation: u64,
        descriptor: CaptureResultDescriptor,
        bytes: Vec<u8>,
    ) -> Result<(), CaptureError> {
        self.require_overlay_session(caller_window_label, session_id, overlay_generation)?;
        self.require_phase(session_id, NativeCapturePhase::Active)?;
        let active = self.require_session_mut(session_id)?;
        active.buffers.store_result(descriptor, bytes)?;
        active.phase = NativeCapturePhase::ResultAvailable;
        Ok(())
    }

    /// Commit a result whose PNG payload was decoded and validated before the
    /// caller acquired the runtime mutex.
    pub fn store_prevalidated_result(
        &mut self,
        caller_window_label: &str,
        session_id: &str,
        overlay_generation: u64,
        descriptor: CaptureResultDescriptor,
        bytes: Vec<u8>,
    ) -> Result<(), CaptureError> {
        self.require_overlay_session(caller_window_label, session_id, overlay_generation)?;
        self.require_phase(session_id, NativeCapturePhase::Active)?;
        let active = self.require_session_mut(session_id)?;
        active
            .buffers
            .store_prevalidated_result(descriptor, bytes)?;
        active.phase = NativeCapturePhase::ResultAvailable;
        Ok(())
    }

    pub fn result_delivery_metadata(
        &self,
        caller_window_label: &str,
        session_id: &str,
        overlay_generation: u64,
        result_id: &str,
    ) -> Result<(CaptureTarget, CaptureResultDescriptor), CaptureError> {
        self.require_overlay_session(caller_window_label, session_id, overlay_generation)?;
        self.require_phase(session_id, NativeCapturePhase::ResultAvailable)?;
        let active = self.require_session(session_id)?;
        let target = active.request.target.clone().ok_or_else(|| {
            CaptureError::new(
                CaptureErrorCode::TargetUnavailable,
                "capture delivery target is no longer available",
            )
        })?;
        active
            .buffers
            .inspect_result(session_id, result_id, |descriptor, _| {
                Ok((target, descriptor.clone()))
            })
    }

    pub fn snapshot_result(
        &self,
        caller_window_label: &str,
        session_id: &str,
        overlay_generation: u64,
        result_id: &str,
    ) -> Result<(CaptureResultDescriptor, Arc<[u8]>), CaptureError> {
        self.require_overlay_session(caller_window_label, session_id, overlay_generation)?;
        self.require_phase(session_id, NativeCapturePhase::ResultAvailable)?;
        self.require_session(session_id)?
            .buffers
            .snapshot_result(session_id, result_id)
    }

    pub fn complete_result_export(
        &mut self,
        caller_window_label: &str,
        session_id: &str,
        overlay_generation: u64,
        result_id: &str,
    ) -> Result<CaptureCleanup, CaptureError> {
        self.require_overlay_session(caller_window_label, session_id, overlay_generation)?;
        self.require_phase(session_id, NativeCapturePhase::ResultAvailable)?;
        self.require_session(session_id)?.buffers.inspect_result(
            session_id,
            result_id,
            |_, _| Ok(()),
        )?;
        self.enter_terminal(TerminalOutcome::Completed)
    }

    pub fn discard_result(
        &mut self,
        caller_window_label: &str,
        session_id: &str,
        overlay_generation: u64,
        result_id: &str,
    ) -> Result<(), CaptureError> {
        self.require_overlay_session(caller_window_label, session_id, overlay_generation)?;
        self.require_phase(session_id, NativeCapturePhase::ResultAvailable)?;
        let active = self.require_session_mut(session_id)?;
        active.buffers.discard_result(session_id, result_id)?;
        active.phase = NativeCapturePhase::Active;
        Ok(())
    }

    /// Drops a frozen delivery target without terminating the capture. This is
    /// used when a distinct chat window closes: the overlay remains useful for
    /// copy, save, or explicit cancellation, but result delivery cannot be
    /// redirected to another window.
    pub fn invalidate_target(
        &mut self,
        session_id: &str,
        target_window_label: &str,
    ) -> Result<bool, CaptureError> {
        let active = self.require_session_mut(session_id)?;
        let matches = active
            .request
            .target
            .as_ref()
            .is_some_and(|target| target.window_label == target_window_label);
        if matches {
            active.request.target = None;
        }
        Ok(matches)
    }

    pub fn pending_cleanup(&self, session_id: &str) -> Result<CaptureCleanup, CaptureError> {
        self.require_phase(session_id, NativeCapturePhase::Restoring)?;
        self.require_session(session_id)?.cleanup()
    }

    /// Acquires the sole delivery read. The command layer must generate a fresh
    /// unpredictable lease ID; it must not accept `lease_id` from webview JSON.
    pub fn lease_result(
        &mut self,
        caller_window_label: &str,
        target_token: &str,
        session_id: &str,
        result_id: &str,
        lease_id: &str,
    ) -> Result<(CaptureResultDescriptor, Vec<u8>), CaptureError> {
        self.authorize_target(session_id, caller_window_label, target_token)?;
        if self.phase() == NativeCapturePhase::Delivering {
            return Err(CaptureError::new(
                CaptureErrorCode::Busy,
                "capture result already has an in-flight delivery",
            ));
        }
        self.require_phase(session_id, NativeCapturePhase::ResultAvailable)?;
        let active = self.require_session_mut(session_id)?;
        let result = active
            .buffers
            .read_result(session_id, result_id, lease_id)?;
        active.phase = NativeCapturePhase::Delivering;
        Ok(result)
    }

    pub fn release_result_lease(
        &mut self,
        caller_window_label: &str,
        target_token: &str,
        session_id: &str,
        result_id: &str,
        lease_id: &str,
    ) -> Result<(), CaptureError> {
        self.authorize_target(session_id, caller_window_label, target_token)?;
        self.require_phase(session_id, NativeCapturePhase::Delivering)?;
        let active = self.require_session_mut(session_id)?;
        active
            .buffers
            .release_result_read(session_id, result_id, lease_id)?;
        active.phase = NativeCapturePhase::ResultAvailable;
        Ok(())
    }

    /// Authorizes an idempotent delivery abort before a result lease exists.
    /// The exact result must still be retained for this frozen target; callers
    /// cannot use the retry signal to probe another session or result ID.
    pub fn validate_pending_result_target(
        &self,
        caller_window_label: &str,
        target_token: &str,
        session_id: &str,
        result_id: &str,
    ) -> Result<(), CaptureError> {
        self.authorize_target(session_id, caller_window_label, target_token)?;
        self.require_phase(session_id, NativeCapturePhase::ResultAvailable)?;
        self.require_session(session_id)?
            .buffers
            .inspect_result(session_id, result_id, |_, _| Ok(()))
    }

    pub fn ack_result(
        &mut self,
        caller_window_label: &str,
        target_token: &str,
        session_id: &str,
        result_id: &str,
        lease_id: &str,
    ) -> Result<CaptureCleanup, CaptureError> {
        self.authorize_target(session_id, caller_window_label, target_token)?;
        self.require_phase(session_id, NativeCapturePhase::Delivering)?;
        self.require_session_mut(session_id)?
            .buffers
            .ack_result(session_id, result_id, lease_id)?;
        self.enter_terminal(TerminalOutcome::Completed)
    }

    pub fn cancel_from_window(
        &mut self,
        caller_window_label: &str,
        session_id: &str,
    ) -> Result<CaptureCleanup, CaptureError> {
        let active = self.require_session(session_id)?;
        let authorized = caller_window_label == self.overlay_window_label
            || active
                .request
                .origin
                .as_ref()
                .is_some_and(|origin| origin.window_label == caller_window_label)
            || active
                .request
                .target
                .as_ref()
                .is_some_and(|target| target.window_label == caller_window_label);
        if !authorized {
            return Err(unauthorized_error(
                "invoking window does not belong to the capture session",
            ));
        }
        if active.phase == NativeCapturePhase::Restoring {
            return active.cleanup();
        }
        self.enter_terminal(TerminalOutcome::Cancelled)
    }

    /// Internal failure path for the native coordinator; command handlers must
    /// not expose this without deriving and validating the invoking window.
    pub fn fail(
        &mut self,
        session_id: &str,
        error: CaptureError,
    ) -> Result<CaptureCleanup, CaptureError> {
        let active = self.require_session(session_id)?;
        if active.phase == NativeCapturePhase::Restoring {
            return active.cleanup();
        }
        self.enter_terminal(TerminalOutcome::Failed(error))
    }

    /// Called only after the window adapter has restored the origin. Keeping the
    /// session guard active until here prevents a new capture from racing window
    /// restoration.
    pub fn finish_restoration(&mut self, session_id: &str) -> Result<(), CaptureError> {
        self.require_phase(session_id, NativeCapturePhase::Restoring)?;
        self.require_session_mut(session_id)?.buffers.clear();
        self.session_guard.finish(session_id)?;
        self.active = None;
        Ok(())
    }

    fn enter_terminal(&mut self, outcome: TerminalOutcome) -> Result<CaptureCleanup, CaptureError> {
        let active = self.active.as_mut().ok_or_else(no_active_session_error)?;
        active.buffers.clear();
        active.phase = NativeCapturePhase::Restoring;
        active.terminal_outcome = Some(outcome);
        active.cleanup()
    }

    fn require_overlay_session(
        &self,
        caller_window_label: &str,
        session_id: &str,
        overlay_generation: u64,
    ) -> Result<(), CaptureError> {
        self.authorize_caller(session_id, caller_window_label, CaptureCallerRole::Overlay)?;
        validate_generation(overlay_generation)?;
        if self.require_session(session_id)?.overlay_generation != overlay_generation {
            return Err(stale_generation_error());
        }
        Ok(())
    }

    pub(crate) fn authorize_target(
        &self,
        session_id: &str,
        caller_window_label: &str,
        target_token: &str,
    ) -> Result<(), CaptureError> {
        self.authorize_caller(session_id, caller_window_label, CaptureCallerRole::Target)?;
        let target_matches = self
            .require_session(session_id)?
            .request
            .target
            .as_ref()
            .is_some_and(|target| target.target_token == target_token);
        if !target_matches {
            return Err(unauthorized_error(
                "capture delivery target token is stale or unknown",
            ));
        }
        Ok(())
    }

    fn require_overlay_label(&self, caller_window_label: &str) -> Result<(), CaptureError> {
        if caller_window_label != self.overlay_window_label {
            return Err(unauthorized_error(
                "only the dedicated capture overlay may report its lifecycle",
            ));
        }
        Ok(())
    }

    fn require_session(&self, session_id: &str) -> Result<&ActiveCaptureSession, CaptureError> {
        let active = self.active.as_ref().ok_or_else(no_active_session_error)?;
        if active.request.session_id != session_id
            || self.session_guard.active_session_id() != Some(session_id)
        {
            return Err(no_active_session_error());
        }
        Ok(active)
    }

    fn require_session_mut(
        &mut self,
        session_id: &str,
    ) -> Result<&mut ActiveCaptureSession, CaptureError> {
        if self.session_guard.active_session_id() != Some(session_id) {
            return Err(no_active_session_error());
        }
        let active = self.active.as_mut().ok_or_else(no_active_session_error)?;
        if active.request.session_id != session_id {
            return Err(no_active_session_error());
        }
        Ok(active)
    }

    fn require_phase(
        &self,
        session_id: &str,
        expected: NativeCapturePhase,
    ) -> Result<(), CaptureError> {
        let active = self.require_session(session_id)?;
        if active.phase != expected {
            return Err(CaptureError::new(
                CaptureErrorCode::InvalidPhase,
                format!(
                    "capture operation requires phase {expected:?}, current phase is {:?}",
                    active.phase
                ),
            ));
        }
        Ok(())
    }
}

fn validate_generation(generation: u64) -> Result<(), CaptureError> {
    if generation == 0 {
        return Err(CaptureError::new(
            CaptureErrorCode::InvalidSession,
            "capture overlay generation must be non-zero",
        ));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::{CaptureCallerRole, CaptureCoordinator, TerminalOutcome};
    use crate::commands::screen_capture::contract::{
        CaptureError, CaptureErrorCode, CaptureOrigin, CaptureRequest, CaptureResultDescriptor,
        CaptureTarget, CaptureTriggerKind, CapturedFrame, LogicalPoint, LogicalSize,
        MonitorGeometry, NativeCapturePhase, PhysicalPoint, PhysicalSize,
    };

    const OVERLAY: &str = "screen-capture-overlay";
    const GENERATION: u64 = 7;
    const TARGET_TOKEN: &str = "chat-42";

    fn composer_request(session_id: &str) -> CaptureRequest {
        CaptureRequest {
            session_id: session_id.to_string(),
            trigger: CaptureTriggerKind::Composer,
            origin: Some(CaptureOrigin {
                window_label: "main".to_string(),
            }),
            target: Some(CaptureTarget {
                window_label: "main".to_string(),
                target_token: TARGET_TOKEN.to_string(),
            }),
        }
    }

    fn global_request(session_id: &str) -> CaptureRequest {
        CaptureRequest {
            session_id: session_id.to_string(),
            trigger: CaptureTriggerKind::Global,
            origin: None,
            target: None,
        }
    }

    fn frame(session_id: &str) -> CapturedFrame {
        CapturedFrame::new(
            session_id,
            MonitorGeometry {
                id: "main-monitor".to_string(),
                physical_origin: PhysicalPoint { x: 0, y: 0 },
                physical_size: PhysicalSize {
                    width: 2,
                    height: 2,
                },
                logical_origin: LogicalPoint { x: 0.0, y: 0.0 },
                logical_size: LogicalSize {
                    width: 2.0,
                    height: 2.0,
                },
                scale_factor: 1.0,
            },
            2,
            2,
            8,
            vec![0x5a; 16],
        )
        .expect("frame fixture")
    }

    fn png(session_id: &str, result_id: &str) -> (CaptureResultDescriptor, Vec<u8>) {
        let bytes = crate::commands::screen_capture::png_fixture(1, 1);
        (
            CaptureResultDescriptor {
                session_id: session_id.to_string(),
                result_id: result_id.to_string(),
                width: 1,
                height: 1,
                filename: "Plain-capture.png".to_string(),
                mime_type: "image/png".to_string(),
                byte_len: bytes.len(),
            },
            bytes,
        )
    }

    fn advance_to_active(coordinator: &mut CaptureCoordinator, session_id: &str) {
        coordinator
            .note_overlay_ready(OVERLAY, GENERATION)
            .expect("overlay ready");
        coordinator
            .start_from_window("main", composer_request(session_id), GENERATION)
            .expect("start capture");
        coordinator
            .begin_capture(session_id)
            .expect("begin capture");
        coordinator
            .store_frame(session_id, frame(session_id))
            .expect("store frame");
        coordinator
            .take_frame(OVERLAY, session_id, GENERATION)
            .expect("take frame");
        coordinator
            .frame_presented(OVERLAY, session_id, GENERATION)
            .expect("present frame");
    }

    #[test]
    fn overlay_identity_can_rotate_only_while_idle_and_unready() {
        let mut coordinator = CaptureCoordinator::new(OVERLAY).expect("coordinator");
        assert_eq!(
            coordinator
                .replace_overlay_window_label(" ")
                .expect_err("blank overlay label")
                .code,
            CaptureErrorCode::OverlayFailed
        );

        coordinator
            .replace_overlay_window_label("screen-capture-overlay-8")
            .expect("idle overlay identity rotates");
        assert_eq!(
            coordinator
                .note_overlay_ready(OVERLAY, GENERATION)
                .expect_err("retired label is unauthorized")
                .code,
            CaptureErrorCode::UnauthorizedCaller
        );
        coordinator
            .note_overlay_ready("screen-capture-overlay-8", GENERATION + 1)
            .expect("replacement overlay becomes ready");
        assert_eq!(
            coordinator
                .replace_overlay_window_label("screen-capture-overlay-9")
                .expect_err("ready overlay identity cannot rotate")
                .code,
            CaptureErrorCode::Busy
        );
    }

    #[test]
    fn ready_before_start_runs_the_full_acknowledged_delivery_state_machine() {
        let mut coordinator = CaptureCoordinator::new(OVERLAY).expect("coordinator");
        coordinator
            .note_overlay_ready(OVERLAY, GENERATION)
            .expect("prewarmed overlay");

        assert_eq!(
            coordinator
                .start_from_window("main", composer_request("session-a"), GENERATION)
                .expect("start")
                .phase,
            NativeCapturePhase::HidingOrigin
        );
        coordinator
            .begin_capture("session-a")
            .expect("begin capture");
        coordinator
            .store_frame("session-a", frame("session-a"))
            .expect("store frame");
        assert!(coordinator.has_sensitive_buffers());

        let (frame_descriptor, frame_bytes) = coordinator
            .take_frame(OVERLAY, "session-a", GENERATION)
            .expect("one-shot frame");
        assert_eq!(frame_descriptor.byte_len, 16);
        assert_eq!(frame_bytes, vec![0x5a; 16]);
        assert_eq!(
            coordinator.phase(),
            NativeCapturePhase::AwaitingPresentation
        );
        assert_eq!(
            coordinator
                .take_frame(OVERLAY, "session-a", GENERATION)
                .expect_err("frame is one-shot")
                .code,
            CaptureErrorCode::InvalidPhase
        );

        coordinator
            .frame_presented(OVERLAY, "session-a", GENERATION)
            .expect("presentation barrier");
        let (descriptor, bytes) = png("session-a", "result-a");
        coordinator
            .store_result(
                OVERLAY,
                "session-a",
                GENERATION,
                descriptor.clone(),
                bytes.clone(),
            )
            .expect("store result");
        assert_eq!(coordinator.phase(), NativeCapturePhase::ResultAvailable);

        assert_eq!(
            coordinator
                .lease_result("other", TARGET_TOKEN, "session-a", "result-a", "lease-a",)
                .expect_err("wrong target")
                .code,
            CaptureErrorCode::UnauthorizedCaller
        );
        assert_eq!(
            coordinator
                .lease_result("main", TARGET_TOKEN, "session-a", "result-a", "lease-a",)
                .expect("delivery lease"),
            (descriptor.clone(), bytes.clone())
        );
        assert_eq!(
            coordinator
                .release_result_lease("main", "stale-chat", "session-a", "result-a", "lease-a",)
                .expect_err("stale target token")
                .code,
            CaptureErrorCode::UnauthorizedCaller
        );
        assert_eq!(coordinator.phase(), NativeCapturePhase::Delivering);
        assert_eq!(
            coordinator
                .lease_result("main", TARGET_TOKEN, "session-a", "result-a", "lease-b",)
                .expect_err("one delivery at a time")
                .code,
            CaptureErrorCode::Busy
        );
        coordinator
            .release_result_lease("main", TARGET_TOKEN, "session-a", "result-a", "lease-a")
            .expect("failed delivery is retryable");
        assert_eq!(coordinator.phase(), NativeCapturePhase::ResultAvailable);
        coordinator
            .lease_result("main", TARGET_TOKEN, "session-a", "result-a", "lease-b")
            .expect("retry delivery");

        let cleanup = coordinator
            .ack_result("main", TARGET_TOKEN, "session-a", "result-a", "lease-b")
            .expect("acknowledge delivery");
        assert_eq!(cleanup.outcome, TerminalOutcome::Completed);
        assert_eq!(cleanup.origin_window_label.as_deref(), Some("main"));
        assert_eq!(coordinator.phase(), NativeCapturePhase::Restoring);
        assert!(!coordinator.has_sensitive_buffers());
        assert_eq!(
            coordinator
                .start_from_window("main", composer_request("session-b"), GENERATION)
                .expect_err("restoration keeps the guard active")
                .code,
            CaptureErrorCode::Busy
        );

        coordinator
            .finish_restoration("session-a")
            .expect("restoration finished");
        assert_eq!(coordinator.phase(), NativeCapturePhase::Idle);
        assert_eq!(coordinator.active_session_id(), None);
    }

    #[test]
    fn start_before_ready_waits_for_the_exact_overlay_generation() {
        let mut coordinator = CaptureCoordinator::new(OVERLAY).expect("coordinator");
        coordinator
            .start_from_window("main", composer_request("session-a"), GENERATION)
            .expect("start before ready");
        assert_eq!(coordinator.phase(), NativeCapturePhase::WaitingForOverlay);
        assert_eq!(
            coordinator
                .begin_capture("session-a")
                .expect_err("capture cannot outrun readiness")
                .code,
            CaptureErrorCode::InvalidPhase
        );
        assert_eq!(
            coordinator
                .note_overlay_ready("main", GENERATION)
                .expect_err("ordinary windows cannot claim overlay readiness")
                .code,
            CaptureErrorCode::UnauthorizedCaller
        );
        assert_eq!(
            coordinator
                .note_overlay_ready(OVERLAY, GENERATION - 1)
                .expect_err("stale generation")
                .code,
            CaptureErrorCode::InvalidSession
        );
        coordinator
            .note_overlay_ready(OVERLAY, GENERATION)
            .expect("matching generation");
        assert_eq!(coordinator.phase(), NativeCapturePhase::HidingOrigin);
    }

    #[test]
    fn caller_roles_are_derived_from_frozen_session_labels() {
        let mut coordinator = CaptureCoordinator::new(OVERLAY).expect("coordinator");
        assert_eq!(
            coordinator
                .start_from_window("other", composer_request("session-a"), GENERATION)
                .expect_err("caller cannot forge origin")
                .code,
            CaptureErrorCode::UnauthorizedCaller
        );
        assert_eq!(
            coordinator
                .start_from_window(OVERLAY, composer_request("session-a"), GENERATION)
                .expect_err("overlay cannot start composer capture")
                .code,
            CaptureErrorCode::UnauthorizedCaller
        );

        coordinator
            .start_global(global_request("global-a"), GENERATION)
            .expect("native global capture");
        assert_eq!(
            coordinator
                .authorize_caller("global-a", "main", CaptureCallerRole::Target)
                .expect_err("global capture has no delivery target")
                .code,
            CaptureErrorCode::UnauthorizedCaller
        );
        coordinator
            .authorize_caller("global-a", OVERLAY, CaptureCallerRole::Overlay)
            .expect("dedicated overlay");
    }

    #[test]
    fn cancel_and_fail_clear_buffers_before_window_restoration() {
        let mut coordinator = CaptureCoordinator::new(OVERLAY).expect("coordinator");
        coordinator
            .note_overlay_ready(OVERLAY, GENERATION)
            .expect("ready");
        coordinator
            .start_from_window("main", composer_request("cancelled"), GENERATION)
            .expect("start");
        coordinator.begin_capture("cancelled").expect("capture");
        coordinator
            .store_frame("cancelled", frame("cancelled"))
            .expect("sensitive frame");
        let cleanup = coordinator
            .cancel_from_window(OVERLAY, "cancelled")
            .expect("overlay cancel");
        assert_eq!(cleanup.outcome, TerminalOutcome::Cancelled);
        assert_eq!(coordinator.phase(), NativeCapturePhase::Restoring);
        assert!(!coordinator.has_sensitive_buffers());
        coordinator
            .finish_restoration("cancelled")
            .expect("finish cancel restoration");

        advance_to_active(&mut coordinator, "failed");
        let (descriptor, bytes) = png("failed", "result-failed");
        coordinator
            .store_result(OVERLAY, "failed", GENERATION, descriptor, bytes)
            .expect("result");
        coordinator
            .lease_result(
                "main",
                TARGET_TOKEN,
                "failed",
                "result-failed",
                "lease-failed",
            )
            .expect("in-flight result");
        let failure = CaptureError::new(CaptureErrorCode::EncodeFailed, "renderer crashed");
        let cleanup = coordinator.fail("failed", failure.clone()).expect("fail");
        assert_eq!(cleanup.outcome, TerminalOutcome::Failed(failure));
        assert!(!coordinator.has_sensitive_buffers());
    }

    #[test]
    fn stale_session_and_lease_operations_leave_the_active_session_unchanged() {
        let mut coordinator = CaptureCoordinator::new(OVERLAY).expect("coordinator");
        advance_to_active(&mut coordinator, "session-a");
        let (descriptor, bytes) = png("session-a", "result-a");
        coordinator
            .store_result(OVERLAY, "session-a", GENERATION, descriptor, bytes)
            .expect("result");
        coordinator
            .lease_result("main", TARGET_TOKEN, "session-a", "result-a", "lease-a")
            .expect("lease");

        assert_eq!(
            coordinator
                .release_result_lease("main", TARGET_TOKEN, "stale", "result-a", "lease-a",)
                .expect_err("stale session")
                .code,
            CaptureErrorCode::InvalidSession
        );
        assert_eq!(
            coordinator
                .ack_result("main", TARGET_TOKEN, "session-a", "result-a", "wrong-lease",)
                .expect_err("stale lease")
                .code,
            CaptureErrorCode::InvalidSession
        );
        assert_eq!(coordinator.phase(), NativeCapturePhase::Delivering);
        assert!(coordinator.has_sensitive_buffers());
    }

    #[test]
    fn losing_the_expected_overlay_generation_clears_native_pixels() {
        let mut coordinator = CaptureCoordinator::new(OVERLAY).expect("coordinator");
        coordinator
            .note_overlay_ready(OVERLAY, GENERATION)
            .expect("ready");
        coordinator
            .start_from_window("main", composer_request("session-a"), GENERATION)
            .expect("start");
        coordinator.begin_capture("session-a").expect("capture");
        coordinator
            .store_frame("session-a", frame("session-a"))
            .expect("frame");

        let cleanup = coordinator
            .note_overlay_unavailable(OVERLAY, GENERATION)
            .expect("overlay lifecycle")
            .expect("active session cleanup");
        assert_eq!(
            cleanup.outcome,
            TerminalOutcome::Failed(CaptureError::new(
                CaptureErrorCode::OverlayFailed,
                "capture overlay became unavailable",
            ))
        );
        assert_eq!(coordinator.phase(), NativeCapturePhase::Restoring);
        assert!(!coordinator.has_sensitive_buffers());
    }

    #[test]
    fn command_facing_state_serializes_metadata_without_pixel_fields() {
        let mut coordinator = CaptureCoordinator::new(OVERLAY).expect("coordinator");
        let state = coordinator
            .start_from_window("main", composer_request("session-a"), GENERATION)
            .expect("start");
        let json = serde_json::to_value(state).expect("serialize command state");

        assert_eq!(json["sessionId"], "session-a");
        assert_eq!(json["phase"], "waiting_for_overlay");
        assert_eq!(json["overlayGeneration"], GENERATION);
        assert!(json.get("bytes").is_none());
    }
}
