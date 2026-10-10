use tauri::ipc::{InvokeBody, Response};

use crate::commands::screen_capture::contract::CaptureError;
use crate::commands::screen_capture::contract::CaptureErrorCode;

pub fn raw_response(bytes: Vec<u8>) -> Response {
    Response::new(bytes)
}

pub fn require_raw_body(body: &InvokeBody) -> Result<&[u8], CaptureError> {
    match body {
        InvokeBody::Raw(bytes) => Ok(bytes),
        InvokeBody::Json(_) => Err(CaptureError::new(
            CaptureErrorCode::InvalidFrame,
            "capture pixels must use binary IPC",
        )),
    }
}
