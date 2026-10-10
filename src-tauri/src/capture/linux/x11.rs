//! X11 plain-capture backend: RandR enumeration and root-window GetImage
//! screenshots through the pure-Rust X11 protocol implementation.
//!
//! X11 has no streaming screenshot API; one capture is one synchronous
//! `GetImage` for the monitor's region of the root window. The pointer is
//! never part of a GetImage result.

use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as RandrConnectionExt;
use x11rb::protocol::xproto::{ConnectionExt, ImageFormat, Window};
use x11rb::rust_connection::RustConnection;

use crate::capture::Capture;
use crate::capture::CaptureError;
use crate::capture::CaptureErrorCode;
use crate::capture::DisplayInfo;
use crate::capture::Frame;
use crate::capture::PhysicalPoint;
use crate::capture::checked_rounded_i32;
use crate::capture::select_display_at;

pub struct X11Capture {
    conn: RustConnection,
    root: Window,
}

impl X11Capture {
    pub fn connect() -> Result<Self, CaptureError> {
        let (conn, screen_num) = x11rb::connect(None)
            .map_err(|error| native_failure("x11 connect (is DISPLAY set?)", error))?;
        let root = conn.setup().roots[screen_num].root;
        Ok(Self { conn, root })
    }

    fn randr_monitors(&self) -> Result<Vec<(String, i16, i16, u16, u16)>, CaptureError> {
        let reply = self
            .conn
            .randr_get_monitors(self.root, true)
            .map_err(|error| native_failure("randr_get_monitors", error))?
            .reply()
            .map_err(|error| native_failure("randr_get_monitors reply", error))?;

        let mut monitors = Vec::new();
        for monitor in reply.monitors {
            let name = self
                .conn
                .get_atom_name(monitor.name)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
                .map(|reply| String::from_utf8_lossy(&reply.name).into_owned())
                .unwrap_or_else(|| format!("monitor-{}", monitor.name));
            monitors.push((name, monitor.x, monitor.y, monitor.width, monitor.height));
        }
        if monitors.is_empty() {
            return Err(CaptureError::new(
                CaptureErrorCode::NoMonitor,
                "RandR reports no active monitors",
            ));
        }
        Ok(monitors)
    }

    fn display_entry(
        &self,
        display: &DisplayInfo,
    ) -> Result<(String, i16, i16, u16, u16), CaptureError> {
        self.randr_monitors()?
            .into_iter()
            .enumerate()
            .find(|(index, (name, x, y, width, height))| {
                display.id == x11_monitor_id(*index, name, *x, *y, *width, *height)
            })
            .map(|(_, entry)| entry)
            .ok_or_else(|| {
                CaptureError::new(
                    CaptureErrorCode::NoMonitor,
                    "the selected monitor disappeared before capture",
                )
            })
    }
}

fn native_failure(stage: &str, error: impl std::fmt::Display) -> CaptureError {
    CaptureError::new(
        CaptureErrorCode::CaptureFailed,
        format!("failed to {stage}: {error}"),
    )
}

fn x11_monitor_id(index: usize, name: &str, x: i16, y: i16, width: u16, height: u16) -> String {
    format!("{index}:{name}:{x}:{y}:{width}:{height}")
}

impl Capture for X11Capture {
    fn displays(&self) -> Result<Vec<DisplayInfo>, CaptureError> {
        self.randr_monitors()?
            .into_iter()
            .enumerate()
            .map(
                |(index, (name, x, y, width, height))| -> Result<DisplayInfo, CaptureError> {
                    let scale_factor = 1.0;
                    let geometry = DisplayInfo {
                        id: x11_monitor_id(index, &name, x, y, width, height),
                        physical_origin: PhysicalPoint {
                            x: i32::from(x),
                            y: i32::from(y),
                        },
                        physical_size: crate::capture::linux::super::PhysicalSize {
                            width: u32::from(width),
                            height: u32::from(height),
                        },
                        logical_origin: crate::capture::linux::super::LogicalPoint {
                            x: f64::from(x),
                            y: f64::from(y),
                        },
                        logical_size: crate::capture::linux::super::LogicalSize {
                            width: f64::from(width),
                            height: f64::from(height),
                        },
                        scale_factor,
                    };
                    geometry.validate()?;
                    Ok(geometry)
                },
            )
            .collect()
    }

    fn display_index_at_cursor(&self, displays: &[DisplayInfo]) -> Result<usize, CaptureError> {
        let reply = self
            .conn
            .query_pointer(self.root)
            .map_err(|error| native_failure("query_pointer", error))?
            .reply()
            .map_err(|error| native_failure("query_pointer reply", error))?;
        let physical = PhysicalPoint {
            x: checked_rounded_i32(f64::from(reply.root_x))?,
            y: checked_rounded_i32(f64::from(reply.root_y))?,
        };
        select_display_at(displays, physical)
            .and_then(|selected| {
                displays
                    .iter()
                    .position(|display| display.id == selected.id)
            })
            .ok_or_else(|| {
                CaptureError::new(
                    CaptureErrorCode::NoMonitor,
                    "the pointer is not inside an active monitor",
                )
            })
    }

    fn capture_display(
        &self,
        display: &DisplayInfo,
        exclude_window_ids: &[u64],
    ) -> Result<Frame, CaptureError> {
        let _ = exclude_window_ids;
        crate::capture::linux::super::preflight_capture_permission()?;
        let (_, x, y, width, height) = self.display_entry(display)?;

        let image = self
            .conn
            .get_image(
                ImageFormat::Z_PIXMAP,
                self.root,
                x,
                y,
                width,
                height,
                u32::MAX,
            )
            .map_err(|error| native_failure("get_image", error))?
            .reply()
            .map_err(|error| native_failure("get_image reply", error))?;

        if image.depth != 24 && image.depth != 32 {
            return Err(CaptureError::new(
                CaptureErrorCode::CaptureFailed,
                format!(
                    "x11 visual depth {} is not a supported truecolor layout",
                    image.depth
                ),
            ));
        }
        let stride = u32::from(width).checked_mul(4).ok_or_else(|| {
            CaptureError::new(CaptureErrorCode::FrameTooLarge, "RGBA stride overflow")
        })?;
        let expected = usize::try_from(u64::from(stride) * u64::from(height)).map_err(|_| {
            CaptureError::new(CaptureErrorCode::FrameTooLarge, "frame size overflow")
        })?;
        let mut data = image.data;
        if data.len() < expected {
            return Err(CaptureError::new(
                CaptureErrorCode::FrameTooLarge,
                format!(
                    "get_image returned {} bytes, expected {expected}",
                    data.len()
                ),
            ));
        }
        data.truncate(expected);

        for pixel in data.chunks_exact_mut(4) {
            if image.depth == 24 {
                pixel[3] = 255;
            }
            pixel.swap(0, 2);
        }

        Ok(Frame {
            width: u32::from(width),
            height: u32::from(height),
            stride,
            bytes: data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::x11_monitor_id;

    #[test]
    fn monitor_ids_encode_index_name_and_root_geometry() {
        assert_eq!(
            x11_monitor_id(2, "DP-1", -1920, 0, 1920, 1080),
            "2:DP-1:-1920:0:1920:1080"
        );
    }
}
