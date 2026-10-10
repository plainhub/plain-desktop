//! Linux plain-capture dispatch: X11 via RandR/GetImage, Wayland through the
//! XDG ScreenCast portal (which the app layer drives directly because it
//! needs async session management and user interaction).

use crate::capture::Capture;
use crate::capture::CaptureError;
use crate::capture::CaptureErrorCode;
use crate::capture::wayland_cursor_is_unavailable;

pub(crate) mod wayland;
pub(crate) mod x11;

pub fn platform_capture() -> Result<Box<dyn Capture>, CaptureError> {
    if wayland_cursor_is_unavailable(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
    ) {
        return Err(CaptureError::new(
            CaptureErrorCode::MonitorSelectionUnavailable,
            "Wayland capture goes through the desktop portal path",
        ));
    }
    Ok(Box::new(x11::X11Capture::connect()?))
}
