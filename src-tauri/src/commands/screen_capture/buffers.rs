use std::sync::Arc;

use super::contract::{
    CaptureError, CaptureErrorCode, CaptureResultDescriptor, CapturedFrame,
    CapturedFrameDescriptor, MAX_PNG_RESULT_BYTES, MAX_RAW_FRAME_BYTES,
};

#[derive(Debug)]
struct CaptureResult {
    descriptor: CaptureResultDescriptor,
    bytes: Arc<[u8]>,
    read_lease_id: Option<String>,
}

#[derive(Debug)]
pub struct SessionBuffers {
    session_id: String,
    frame: Option<CapturedFrame>,
    result: Option<CaptureResult>,
}

impl SessionBuffers {
    pub fn new(session_id: impl Into<String>) -> Result<Self, CaptureError> {
        let session_id = session_id.into();
        if session_id.trim().is_empty() {
            return Err(CaptureError::new(
                CaptureErrorCode::InvalidSession,
                "capture session id cannot be empty",
            ));
        }
        Ok(Self {
            session_id,
            frame: None,
            result: None,
        })
    }

    pub fn store_frame(&mut self, frame: CapturedFrame) -> Result<(), CaptureError> {
        self.require_session(&frame.descriptor().session_id)?;
        if self.frame.is_some() {
            return Err(CaptureError::new(
                CaptureErrorCode::Busy,
                "the capture frame is already stored",
            ));
        }
        self.frame = Some(frame);
        Ok(())
    }

    pub fn take_frame(
        &mut self,
        session_id: &str,
    ) -> Result<(CapturedFrameDescriptor, Vec<u8>), CaptureError> {
        self.require_session(session_id)?;
        let frame = self.frame.take().ok_or_else(|| {
            CaptureError::new(
                CaptureErrorCode::InvalidFrame,
                "capture frame is unavailable",
            )
        })?;
        let descriptor = frame.descriptor().clone();
        Ok((descriptor, frame.into_bytes()))
    }

    pub fn frame_descriptor(
        &self,
        session_id: &str,
    ) -> Result<CapturedFrameDescriptor, CaptureError> {
        self.require_session(session_id)?;
        self.frame
            .as_ref()
            .map(|frame| frame.descriptor().clone())
            .ok_or_else(|| {
                CaptureError::new(
                    CaptureErrorCode::InvalidFrame,
                    "capture frame is unavailable",
                )
            })
    }

    pub fn store_result(
        &mut self,
        descriptor: CaptureResultDescriptor,
        bytes: Vec<u8>,
    ) -> Result<(), CaptureError> {
        validate_result_payload(&descriptor, &bytes)?;
        self.store_prevalidated_result(descriptor, bytes)
    }

    /// Store bytes already validated by `validate_result_payload`. Runtime IPC
    /// uses this only after decoding outside its global state mutex.
    pub(crate) fn store_prevalidated_result(
        &mut self,
        descriptor: CaptureResultDescriptor,
        bytes: Vec<u8>,
    ) -> Result<(), CaptureError> {
        self.require_session(&descriptor.session_id)?;
        if self.result.is_some() {
            return Err(CaptureError::new(
                CaptureErrorCode::Busy,
                "a capture result is already awaiting acknowledgement",
            ));
        }
        self.result = Some(CaptureResult {
            descriptor,
            bytes: Arc::from(bytes),
            read_lease_id: None,
        });
        Ok(())
    }

    pub fn read_result(
        &mut self,
        session_id: &str,
        result_id: &str,
        lease_id: &str,
    ) -> Result<(CaptureResultDescriptor, Vec<u8>), CaptureError> {
        self.require_session(session_id)?;
        if lease_id.trim().is_empty() {
            return Err(CaptureError::new(
                CaptureErrorCode::InvalidSession,
                "capture result read requires a delivery lease id",
            ));
        }
        let result = self.result.as_mut().ok_or_else(|| {
            CaptureError::new(
                CaptureErrorCode::InvalidFrame,
                "capture result is unavailable",
            )
        })?;
        if result.descriptor.result_id != result_id {
            return Err(CaptureError::new(
                CaptureErrorCode::InvalidSession,
                "capture result id does not belong to this session",
            ));
        }
        if result.read_lease_id.is_some() {
            return Err(CaptureError::new(
                CaptureErrorCode::Busy,
                "capture result already has an in-flight delivery read",
            ));
        }
        result.read_lease_id = Some(lease_id.to_string());
        Ok((result.descriptor.clone(), result.bytes.as_ref().to_vec()))
    }

    pub fn release_result_read(
        &mut self,
        session_id: &str,
        result_id: &str,
        lease_id: &str,
    ) -> Result<(), CaptureError> {
        self.require_session(session_id)?;
        let result = self.require_result_lease(result_id, lease_id)?;
        result.read_lease_id = None;
        Ok(())
    }

    pub fn ack_result(
        &mut self,
        session_id: &str,
        result_id: &str,
        lease_id: &str,
    ) -> Result<(), CaptureError> {
        self.require_session(session_id)?;
        self.require_result_lease(result_id, lease_id)?;
        self.result = None;
        Ok(())
    }

    pub fn inspect_result<T>(
        &self,
        session_id: &str,
        result_id: &str,
        inspect: impl FnOnce(&CaptureResultDescriptor, &[u8]) -> Result<T, CaptureError>,
    ) -> Result<T, CaptureError> {
        self.require_session(session_id)?;
        let result = self.result.as_ref().ok_or_else(|| {
            CaptureError::new(
                CaptureErrorCode::InvalidFrame,
                "capture result is unavailable",
            )
        })?;
        if result.descriptor.result_id != result_id {
            return Err(CaptureError::new(
                CaptureErrorCode::InvalidSession,
                "capture result id does not belong to this session",
            ));
        }
        if result.read_lease_id.is_some() {
            return Err(CaptureError::new(
                CaptureErrorCode::Busy,
                "capture result already has an in-flight delivery read",
            ));
        }
        inspect(&result.descriptor, &result.bytes)
    }

    pub fn snapshot_result(
        &self,
        session_id: &str,
        result_id: &str,
    ) -> Result<(CaptureResultDescriptor, Arc<[u8]>), CaptureError> {
        self.inspect_result(session_id, result_id, |descriptor, _| {
            let result = self.result.as_ref().ok_or_else(|| {
                CaptureError::new(
                    CaptureErrorCode::InvalidFrame,
                    "capture result is unavailable",
                )
            })?;
            Ok((descriptor.clone(), result.bytes.clone()))
        })
    }

    pub fn discard_result(
        &mut self,
        session_id: &str,
        result_id: &str,
    ) -> Result<(), CaptureError> {
        self.require_session(session_id)?;
        self.inspect_result(session_id, result_id, |_, _| Ok(()))?;
        self.result = None;
        Ok(())
    }

    fn require_result_lease(
        &mut self,
        result_id: &str,
        lease_id: &str,
    ) -> Result<&mut CaptureResult, CaptureError> {
        let result = self.result.as_mut().ok_or_else(|| {
            CaptureError::new(
                CaptureErrorCode::InvalidFrame,
                "capture result is unavailable",
            )
        })?;
        if result.descriptor.result_id != result_id {
            return Err(CaptureError::new(
                CaptureErrorCode::InvalidSession,
                "capture result id does not belong to this session",
            ));
        }
        if result.read_lease_id.as_deref() != Some(lease_id) {
            return Err(CaptureError::new(
                CaptureErrorCode::InvalidSession,
                "capture result delivery lease is stale or unknown",
            ));
        }
        Ok(result)
    }

    pub fn has_frame(&self) -> bool {
        self.frame.is_some()
    }

    pub fn has_result(&self) -> bool {
        self.result.is_some()
    }

    pub fn clear(&mut self) {
        self.frame = None;
        self.result = None;
    }

    fn require_session(&self, session_id: &str) -> Result<(), CaptureError> {
        if self.session_id != session_id {
            return Err(CaptureError::new(
                CaptureErrorCode::InvalidSession,
                "capture session id is stale or unknown",
            ));
        }
        Ok(())
    }
}

pub(crate) fn validate_result_payload(
    descriptor: &CaptureResultDescriptor,
    bytes: &[u8],
) -> Result<(), CaptureError> {
    if descriptor.result_id.trim().is_empty()
        || descriptor.width == 0
        || descriptor.height == 0
        || descriptor.mime_type != "image/png"
        || !descriptor.filename.to_ascii_lowercase().ends_with(".png")
        || descriptor.byte_len != bytes.len()
    {
        return Err(CaptureError::new(
            CaptureErrorCode::InvalidFrame,
            "capture result metadata does not match a PNG payload",
        ));
    }
    if bytes.len() > MAX_PNG_RESULT_BYTES {
        return Err(CaptureError::new(
            CaptureErrorCode::FrameTooLarge,
            "capture result exceeds the process memory limit",
        ));
    }
    let decoded_len = u64::from(descriptor.width)
        .checked_mul(u64::from(descriptor.height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| {
            CaptureError::new(
                CaptureErrorCode::FrameTooLarge,
                "decoded capture result size overflow",
            )
        })?;
    if decoded_len > MAX_RAW_FRAME_BYTES as u64 {
        return Err(CaptureError::new(
            CaptureErrorCode::FrameTooLarge,
            "decoded capture result exceeds the process memory limit",
        ));
    }
    if !png_payload_matches(bytes, descriptor.width, descriptor.height) {
        return Err(CaptureError::new(
            CaptureErrorCode::InvalidFrame,
            "capture result metadata does not match a decodable PNG payload",
        ));
    }
    Ok(())
}

fn png_payload_matches(bytes: &[u8], width: u32, height: u32) -> bool {
    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    let header_matches = bytes.len() >= 24
        && &bytes[..8] == PNG_SIGNATURE
        && bytes[8..12] == 13_u32.to_be_bytes()
        && &bytes[12..16] == b"IHDR"
        && bytes[16..20] == width.to_be_bytes()
        && bytes[20..24] == height.to_be_bytes();
    header_matches
        && xcap::image::load_from_memory(bytes)
            .is_ok_and(|decoded| decoded.width() == width && decoded.height() == height)
}
