use std::sync::Mutex;

pub(crate) struct LinuxClipboard(Result<Mutex<Option<arboard::Clipboard>>, arboard::Error>);

impl LinuxClipboard {
    pub(crate) fn new() -> Self {
        Self(arboard::Clipboard::new().map(|clipboard| Mutex::new(Some(clipboard))))
    }

    pub(crate) fn shutdown(&self) {
        if let Ok(clipboard) = &self.0 {
            let mut clipboard = clipboard.lock().unwrap_or_else(|error| error.into_inner());
            clipboard.take();
        }
    }

    fn with<T>(
        &self,
        operation: impl FnOnce(&mut arboard::Clipboard) -> Result<T, arboard::Error>,
    ) -> Result<T, String> {
        let mut clipboard = self
            .0
            .as_ref()
            .map_err(ToString::to_string)?
            .lock()
            .map_err(|_| "Desktop clipboard state unavailable")?;
        operation(
            clipboard
                .as_mut()
                .ok_or("Desktop clipboard has shut down")?,
        )
        .map_err(|error| error.to_string())
    }

    pub(super) fn read_text(&self) -> Result<Option<String>, String> {
        self.with(|clipboard| match clipboard.get_text() {
            Ok(text) => Ok(Some(text)),
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(error) => Err(error),
        })
    }

    pub(super) fn write_text(&self, text: &str) -> Result<(), String> {
        self.with(|clipboard| clipboard.set_text(text))
    }

    pub(super) fn write_image(&self, rgba: image::RgbaImage) -> Result<(), String> {
        let (width, height) = rgba.dimensions();
        self.with(|clipboard| {
            clipboard.set_image(arboard::ImageData {
                width: width as usize,
                height: height as usize,
                bytes: rgba.into_raw().into(),
            })
        })
    }
}
