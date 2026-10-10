//! Dev-only end-to-end capture self-test (macOS debug builds).
//!
//! Armed by the flag file `/tmp/plain-capture-selftest` at startup. When armed,
//! the app triggers the real global-capture path shortly after setup and, once
//! the overlay acknowledges presentation, re-captures the same display through
//! the app's own screen-recording permission and writes the pixels to
//! `/tmp/cap-exp/selfie-*.png`. That frame is ground-truth evidence of whether
//! the overlay actually composited on screen. The session is then cancelled
//! through the same runtime path as the overlay's Escape key.
//!
//! This module is compiled out of release builds (`debug_assertions`) and only
//! exists because macOS grants screen-recording and accessibility powers to
//! individual processes, so an external script cannot perform this check.

use std::path::Path;
use std::time::Duration;

use tauri::{AppHandle, Manager, Runtime};

use crate::commands::screen_capture::backend::capture_frame_at_cursor_exclusive;
use crate::commands::screen_capture::contract::CaptureError;
use crate::commands::screen_capture::contract::CaptureErrorCode;
use crate::commands::screen_capture::runtime::ScreenCaptureRuntime;
use crate::commands::screen_capture::window::TauriCaptureWindowPort;

const FLAG_PATH: &str = "/tmp/plain-capture-selftest";
const OUTPUT_DIR: &str = "/tmp/cap-exp";
const TRIGGER_DELAY: Duration = Duration::from_millis(2500);
const SELFIE_SETTLE: Duration = Duration::from_millis(1500);
const SELFIE_SECOND_SHOT_DELAY: Duration = Duration::from_millis(1000);
const PERMISSION_POLL_INTERVAL: Duration = Duration::from_secs(2);
const PERMISSION_POLL_ATTEMPTS: u32 = 45;

pub fn armed() -> bool {
    Path::new(FLAG_PATH).exists()
}

pub fn schedule_trigger<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(TRIGGER_DELAY).await;
        if !armed() {
            return;
        }
        if !ensure_capture_permission().await {
            return;
        }
        log::info!("screen capture self-test triggering the global capture path");
        crate::commands::screen_capture::shortcut::trigger_global_capture(&app);
    });
}

/// Dev-only convenience: surface the system permission prompt without the
/// frontend guidance flow, then wait for the grant to become visible. If
/// macOS still requires an app restart after the grant, the flag file keeps
/// this self-test armed across the restart and it re-runs automatically.
async fn ensure_capture_permission() -> bool {
    let granted = || crate::capture::preflight_capture_permission().is_ok();
    if granted() {
        return true;
    }
    log::info!(
        "screen capture self-test requesting screen-recording permission; \
         allow PlainApp in the system dialog and the self-test continues on its own"
    );
    let requested =
        tauri::async_runtime::spawn_blocking(crate::capture::request_capture_permission)
            .await
            .unwrap_or(false);
    if !requested {
        log::warn!("screen capture self-test could not show the permission request");
        return false;
    }
    for _ in 0..PERMISSION_POLL_ATTEMPTS {
        tokio::time::sleep(PERMISSION_POLL_INTERVAL).await;
        if granted() {
            log::info!("screen capture self-test permission granted");
            return true;
        }
    }
    log::warn!(
        "screen capture self-test permission is still missing; \
         enable PlainApp under System Settings > Privacy & Security > Screen Recording \
         and restart the app (the self-test stays armed)"
    );
    false
}

pub fn schedule_presentation_probe<R: Runtime>(
    app: AppHandle<R>,
    session_id: String,
    overlay_label: String,
    overlay_generation: u64,
) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SELFIE_SETTLE).await;
        if let Err(error) = capture_selfie("selfie-1").await {
            log::warn!("screen capture self-test first selfie failed: {error}");
        }
        tokio::time::sleep(SELFIE_SECOND_SHOT_DELAY).await;
        if let Err(error) = capture_selfie("selfie-2").await {
            log::warn!("screen capture self-test second selfie failed: {error}");
        }
        cancel_session(&app, &session_id, &overlay_label, overlay_generation);
        log::info!("screen capture self-test finished");
    });
}

async fn capture_selfie(stem: &str) -> Result<(), CaptureError> {
    let stem = stem.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let backend = crate::capture::platform_capture()?;
        let frame = capture_frame_at_cursor_exclusive(backend.as_ref(), "capture-selftest", &[])?;
        write_frame_png(frame, &stem)
    })
    .await
    .map_err(|_| CaptureError::new(CaptureErrorCode::CaptureFailed, "self-test worker failed"))?
}

fn write_frame_png(
    frame: crate::commands::screen_capture::contract::CapturedFrame,
    stem: &str,
) -> Result<(), CaptureError> {
    use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};

    let width = frame.descriptor().width;
    let height = frame.descriptor().height;
    let stride = frame.descriptor().stride as usize;
    let row_bytes = width as usize * 4;
    let pixels = frame.into_bytes();
    let mut packed = Vec::with_capacity(row_bytes * height as usize);
    for row in 0..height as usize {
        packed.extend_from_slice(&pixels[row * stride..row * stride + row_bytes]);
    }
    let mut encoded = Vec::new();
    PngEncoder::new(&mut encoded)
        .write_image(&packed, width, height, ExtendedColorType::Rgba8)
        .map_err(|error| {
            CaptureError::new(
                CaptureErrorCode::InvalidFrame,
                format!("self-test PNG encoding failed: {error}"),
            )
        })?;
    std::fs::create_dir_all(OUTPUT_DIR).map_err(|error| {
        CaptureError::new(
            CaptureErrorCode::SaveFailed,
            format!("self-test output directory failed: {error}"),
        )
    })?;
    let path = format!("{OUTPUT_DIR}/{stem}.png");
    std::fs::write(&path, &encoded).map_err(|error| {
        CaptureError::new(
            CaptureErrorCode::SaveFailed,
            format!("self-test selfie write failed: {error}"),
        )
    })?;
    log::info!(
        "screen capture self-test wrote {path} ({}x{} {} bytes)",
        width,
        height,
        encoded.len()
    );
    Ok(())
}

fn cancel_session<R: Runtime>(
    app: &AppHandle<R>,
    session_id: &str,
    overlay_label: &str,
    overlay_generation: u64,
) {
    let runtime = app.state::<ScreenCaptureRuntime>();
    let windows = TauriCaptureWindowPort::new(app.clone());
    match runtime.cancel_from_window(
        overlay_label,
        session_id,
        Some(overlay_generation),
        &windows,
    ) {
        Ok(()) => log::info!("screen capture self-test cancelled the session"),
        Err(error) => log::warn!("screen capture self-test cancel failed: {error}"),
    }
}
