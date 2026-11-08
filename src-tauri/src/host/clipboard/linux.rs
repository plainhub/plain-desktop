use std::sync::Mutex;

#[path = "linux/wayland.rs"]
mod wayland;
#[path = "linux/x11.rs"]
mod x11;

type ClipboardResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
const MAX_TEXT_BYTES: usize = 4 * super::MAX_TEXT_UNITS;

enum Backend {
    Wayland(wayland::Clipboard),
    X11(x11::Clipboard),
}

enum Content {
    Text(String),
    Png(Vec<u8>),
}

pub(crate) struct LinuxClipboard(Result<Mutex<Option<Backend>>, String>);

impl LinuxClipboard {
    pub(crate) fn new() -> Self {
        let wayland =
            std::env::var_os("WAYLAND_DISPLAY").and_then(|_| match wayland::Clipboard::new() {
                Ok(clipboard) => Some(clipboard),
                Err(error) => {
                    log::debug!("Wayland clipboard data control unavailable: {error}");
                    None
                }
            });
        let backend = if let Some(clipboard) = wayland {
            Ok(Backend::Wayland(clipboard))
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

    fn with<T>(
        &self,
        operation: impl FnOnce(&mut Backend) -> ClipboardResult<T>,
    ) -> Result<T, String> {
        let mut backend = self
            .0
            .as_ref()
            .map_err(Clone::clone)?
            .lock()
            .map_err(|_| "Desktop clipboard state unavailable")?;
        operation(backend.as_mut().ok_or("Desktop clipboard has shut down")?)
            .map_err(|error| error.to_string())
    }

    pub(super) fn read_text(&self) -> Result<Option<String>, String> {
        self.with(|backend| match backend {
            Backend::X11(clipboard) => clipboard.read_text(),
            Backend::Wayland(clipboard) => clipboard.read_text(),
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
            Backend::Wayland(clipboard) => clipboard.write(content),
        })
    }
}
