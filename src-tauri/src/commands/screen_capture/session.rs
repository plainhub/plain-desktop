use super::contract::{CaptureError, CaptureErrorCode};

#[derive(Debug, Default)]
pub struct CaptureSessionGuard {
    active_session_id: Option<String>,
}

impl CaptureSessionGuard {
    pub fn start(&mut self, session_id: &str) -> Result<(), CaptureError> {
        if session_id.trim().is_empty() {
            return Err(CaptureError::new(
                CaptureErrorCode::InvalidSession,
                "capture session id cannot be empty",
            ));
        }
        if self.active_session_id.is_some() {
            return Err(CaptureError::new(
                CaptureErrorCode::Busy,
                "another capture session is already active",
            ));
        }
        self.active_session_id = Some(session_id.to_string());
        Ok(())
    }

    pub fn finish(&mut self, session_id: &str) -> Result<(), CaptureError> {
        match self.active_session_id.as_deref() {
            Some(active) if active == session_id => {
                self.active_session_id = None;
                Ok(())
            }
            _ => Err(CaptureError::new(
                CaptureErrorCode::InvalidSession,
                "capture session id is stale or unknown",
            )),
        }
    }

    pub fn active_session_id(&self) -> Option<&str> {
        self.active_session_id.as_deref()
    }
}
