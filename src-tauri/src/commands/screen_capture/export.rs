use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Runtime, image::Image};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;

use super::contract::{CaptureError, CaptureErrorCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SaveCaptureOutcome {
    Saved,
    Cancelled,
}

/// Native export operations are isolated behind this interface so dialog,
/// filesystem, and clipboard failures can be proven without platform UI.
pub trait CaptureExportPort {
    fn choose_save_path(&self, suggested_filename: &str) -> Result<Option<PathBuf>, CaptureError>;
    fn write_png(&self, path: &Path, png: &[u8]) -> Result<(), CaptureError>;
    fn copy_png(&self, width: u32, height: u32, png: &[u8]) -> Result<(), CaptureError>;
}

pub fn save_capture_png(
    exports: &dyn CaptureExportPort,
    suggested_filename: &str,
    png: &[u8],
) -> Result<SaveCaptureOutcome, CaptureError> {
    let Some(path) = exports.choose_save_path(suggested_filename)? else {
        return Ok(SaveCaptureOutcome::Cancelled);
    };
    let path = normalized_png_path(path)?;
    exports.write_png(&path, png)?;
    Ok(SaveCaptureOutcome::Saved)
}

pub fn stable_png_filename(unix_millis: u128) -> String {
    format!("Plain-capture-{unix_millis}.png")
}

pub(crate) fn normalized_png_path(mut path: PathBuf) -> Result<PathBuf, CaptureError> {
    if !path.is_absolute() || path.file_name().is_none() {
        return Err(CaptureError::new(
            CaptureErrorCode::SaveFailed,
            "the selected capture destination is invalid",
        ));
    }
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
    {
        path.set_extension("png");
    }
    Ok(path)
}

pub struct TauriCaptureExportPort<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> TauriCaptureExportPort<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

impl<R: Runtime> CaptureExportPort for TauriCaptureExportPort<R> {
    fn choose_save_path(&self, suggested_filename: &str) -> Result<Option<PathBuf>, CaptureError> {
        self.app
            .dialog()
            .file()
            .add_filter("PNG image", &["png"])
            .set_file_name(suggested_filename)
            .blocking_save_file()
            .map(|path| {
                path.into_path().map_err(|_| {
                    CaptureError::new(
                        CaptureErrorCode::SaveFailed,
                        "the selected capture destination is invalid",
                    )
                })
            })
            .transpose()
    }

    fn write_png(&self, path: &Path, png: &[u8]) -> Result<(), CaptureError> {
        std::fs::write(path, png).map_err(|_| {
            CaptureError::new(
                CaptureErrorCode::SaveFailed,
                "could not write the selected capture destination",
            )
        })
    }

    fn copy_png(&self, width: u32, height: u32, png: &[u8]) -> Result<(), CaptureError> {
        let decoded = xcap::image::load_from_memory(png).map_err(|_| {
            CaptureError::new(
                CaptureErrorCode::ClipboardFailed,
                "could not decode the capture for the system clipboard",
            )
        })?;
        if decoded.width() != width || decoded.height() != height {
            return Err(CaptureError::new(
                CaptureErrorCode::InvalidFrame,
                "decoded clipboard image dimensions do not match the capture result",
            ));
        }
        let rgba = decoded.into_rgba8();
        let image = Image::new_owned(rgba.into_raw(), width, height);
        self.app.clipboard().write_image(&image).map_err(|_| {
            CaptureError::new(
                CaptureErrorCode::ClipboardFailed,
                "could not write the capture to the system clipboard",
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    struct FakeExport {
        selected: Result<Option<PathBuf>, CaptureError>,
        write_error: Option<CaptureError>,
        writes: Mutex<Vec<(PathBuf, Vec<u8>)>>,
    }

    impl CaptureExportPort for FakeExport {
        fn choose_save_path(&self, _: &str) -> Result<Option<PathBuf>, CaptureError> {
            self.selected.clone()
        }

        fn write_png(&self, path: &Path, png: &[u8]) -> Result<(), CaptureError> {
            if let Some(error) = self.write_error.clone() {
                return Err(error);
            }
            self.writes
                .lock()
                .expect("writes")
                .push((path.to_path_buf(), png.to_vec()));
            Ok(())
        }

        fn copy_png(&self, _: u32, _: u32, _: &[u8]) -> Result<(), CaptureError> {
            Ok(())
        }
    }

    fn fake(selected: Result<Option<PathBuf>, CaptureError>) -> FakeExport {
        FakeExport {
            selected,
            write_error: None,
            writes: Mutex::new(Vec::new()),
        }
    }

    #[test]
    fn stable_filename_is_png_and_deterministic() {
        assert_eq!(
            stable_png_filename(1_725_000_123_456),
            "Plain-capture-1725000123456.png"
        );
    }

    #[test]
    fn save_cancel_is_non_destructive_and_writes_nothing() {
        let exports = fake(Ok(None));
        assert_eq!(
            save_capture_png(&exports, "capture.png", b"png").expect("cancel outcome"),
            SaveCaptureOutcome::Cancelled
        );
        assert!(exports.writes.lock().expect("writes").is_empty());
    }

    #[test]
    fn save_normalizes_png_extension_and_propagates_failures() {
        let destination = std::env::temp_dir().join("capture.jpeg");
        let exports = fake(Ok(Some(destination.with_extension("jpeg"))));
        assert_eq!(
            save_capture_png(&exports, "capture.png", b"png").expect("save outcome"),
            SaveCaptureOutcome::Saved
        );
        assert_eq!(
            exports.writes.lock().expect("writes")[0].0,
            destination.with_extension("png")
        );

        let denied = FakeExport {
            selected: Ok(Some(destination)),
            write_error: Some(CaptureError::new(CaptureErrorCode::SaveFailed, "denied")),
            writes: Mutex::new(Vec::new()),
        };
        assert_eq!(
            save_capture_png(&denied, "capture.png", b"png")
                .expect_err("write failure")
                .code,
            CaptureErrorCode::SaveFailed
        );
    }

    #[test]
    fn relative_or_root_destination_is_rejected() {
        for path in [
            PathBuf::from("relative.png"),
            PathBuf::from(std::path::MAIN_SEPARATOR_STR),
        ] {
            let exports = fake(Ok(Some(path)));
            assert_eq!(
                save_capture_png(&exports, "capture.png", b"png")
                    .expect_err("invalid destination")
                    .code,
                CaptureErrorCode::SaveFailed
            );
        }
    }
}
