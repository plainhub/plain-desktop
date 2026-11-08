use std::{io::Read, sync::Mutex};

use wl_clipboard_rs::{copy, paste, utils::is_primary_selection_supported};

#[path = "linux/x11.rs"]
mod x11;

type ClipboardResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
const MAX_TEXT_BYTES: usize = 4 * super::MAX_TEXT_UNITS;

enum Backend {
    Wayland,
    X11(x11::Clipboard),
}

enum Content {
    Text(String),
    Png(Vec<u8>),
}

pub(crate) struct LinuxClipboard(Result<Mutex<Option<Backend>>, String>);

impl LinuxClipboard {
    pub(crate) fn new() -> Self {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some()
            && match is_primary_selection_supported() {
                Ok(_) => true,
                Err(error) => {
                    log::debug!("Wayland clipboard data control unavailable: {error}");
                    false
                }
            };
        let backend = if wayland {
            Ok(Backend::Wayland)
        } else {
            x11::Clipboard::new().map(Backend::X11)
        };
        Self(
            backend
                .map(|backend| Mutex::new(Some(backend)))
                .map_err(|error| error.to_string()),
        )
    }

    pub(crate) fn shutdown(&self) {
        if let Ok(backend) = &self.0 {
            let mut backend = backend.lock().unwrap_or_else(|error| error.into_inner());
            backend.take();
        }
    }

    fn with<T>(&self, operation: impl FnOnce(&Backend) -> ClipboardResult<T>) -> Result<T, String> {
        let backend = self
            .0
            .as_ref()
            .map_err(Clone::clone)?
            .lock()
            .map_err(|_| "Desktop clipboard state unavailable")?;
        operation(backend.as_ref().ok_or("Desktop clipboard has shut down")?)
            .map_err(|error| error.to_string())
    }

    pub(super) fn read_text(&self) -> Result<Option<String>, String> {
        self.with(|backend| match backend {
            Backend::X11(clipboard) => clipboard.read_text(),
            Backend::Wayland => {
                match paste::get_contents(
                    paste::ClipboardType::Regular,
                    paste::Seat::Unspecified,
                    paste::MimeType::Text,
                ) {
                    Ok((pipe, _)) => {
                        let mut text = Vec::new();
                        pipe.take((MAX_TEXT_BYTES + 1) as u64)
                            .read_to_end(&mut text)?;
                        if text.len() > MAX_TEXT_BYTES {
                            return Ok(None);
                        }
                        Ok(Some(String::from_utf8(text)?))
                    }
                    Err(paste::Error::ClipboardEmpty | paste::Error::NoMimeType) => Ok(None),
                    Err(error) => Err(error.into()),
                }
            }
        })
    }

    pub(super) fn write_text(&self, text: &str) -> Result<(), String> {
        self.write(Content::Text(text.to_owned()))
    }

    pub(super) fn write_png(&self, png: &[u8]) -> Result<(), String> {
        self.write(Content::Png(png.to_vec()))
    }

    fn write(&self, content: Content) -> Result<(), String> {
        self.with(|backend| match backend {
            Backend::X11(clipboard) => clipboard.write(content),
            Backend::Wayland => {
                let (bytes, mime) = match content {
                    Content::Text(text) => (text.into_bytes(), copy::MimeType::Text),
                    Content::Png(png) => (png, copy::MimeType::Specific("image/png".into())),
                };
                copy::Options::new().copy(copy::Source::Bytes(bytes.into_boxed_slice()), mime)?;
                Ok(())
            }
        })
    }
}
