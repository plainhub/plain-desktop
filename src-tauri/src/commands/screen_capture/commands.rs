use rand::{RngCore, rngs::OsRng};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{
    AppHandle, Manager, Runtime, State, WebviewWindow,
    ipc::{Request, Response},
};

use super::backend::capture_frame_at_cursor_exclusive;
use super::contract::{
    CaptureError, CaptureErrorCode, CaptureResultDescriptor, CaptureResultSubmission,
    CaptureTarget, CapturedFrame, CssRect, MAX_PNG_RESULT_BYTES, NativeCapturePhase,
};
use super::export::{SaveCaptureOutcome, TauriCaptureExportPort, stable_png_filename};
use super::ipc::{raw_response, require_raw_body};
use super::platform::XcapBackend;
use super::runtime::{
    CapturePublishOutcome, CaptureStartResponse, CaptureTicket, CaptureTimeoutKind, OverlayInit,
    ScreenCaptureRuntime, acquire_and_publish_once, is_overlay_window_label,
    is_regular_window_label,
};
use super::window::TauriCaptureWindowPort;

pub const RESULT_SESSION_HEADER: &str = "x-plain-capture-session-id";
pub const RESULT_GENERATION_HEADER: &str = "x-plain-capture-overlay-generation";
pub const RESULT_WIDTH_HEADER: &str = "x-plain-capture-width";
pub const RESULT_HEIGHT_HEADER: &str = "x-plain-capture-height";
const MAX_CLIENT_ERROR_DETAIL_CHARS: usize = 1024;
const CAPTURE_READINESS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const CAPTURE_PRESENTATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const CAPTURE_LIFETIME_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15 * 60);
// `WebviewWindow::hide` acknowledges the unmap request, not completion of the
// desktop compositor's fade-out. 100 ms still captured a partially faded
// origin under X11/KWin and a phantom outline under macOS; allow the animation
// to finish before reading pixels.
#[cfg(any(target_os = "linux", target_os = "macos"))]
const COMPOSITOR_UNMAP_SETTLE: Duration = Duration::from_millis(250);
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
const COMPOSITOR_UNMAP_SETTLE: Duration = Duration::from_millis(100);
const CAPTURE_BACKEND_TIMEOUT: Duration = Duration::from_secs(135);
#[cfg(target_os = "linux")]
const PORTAL_INTERACTION_TIMEOUT: Duration = Duration::from_secs(120);
#[cfg(target_os = "linux")]
const PORTAL_FRAME_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(target_os = "macos")]
const MACOS_SCREEN_CAPTURE_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";

fn bounded_client_error_detail(detail: &str) -> String {
    let sanitized: String = detail
        .chars()
        .take(MAX_CLIENT_ERROR_DETAIL_CHARS)
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect();
    if sanitized.trim().is_empty() {
        "screen capture client reported an unspecified failure".to_string()
    } else {
        sanitized
    }
}

fn authorize_permission_command_caller(window_label: &str) -> Result<(), CaptureError> {
    if !is_regular_window_label(window_label) {
        return Err(CaptureError::new(
            CaptureErrorCode::UnauthorizedCaller,
            "only a regular application window may manage screen capture permission",
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn screen_capture_request_permission(
    window: WebviewWindow,
) -> Result<bool, CaptureError> {
    authorize_permission_command_caller(window.label())?;
    #[cfg(target_os = "macos")]
    {
        return tauri::async_runtime::spawn_blocking(super::platform::request_capture_permission)
            .await
            .map_err(|error| {
                CaptureError::new(
                    CaptureErrorCode::CaptureFailed,
                    format!("screen capture permission request worker failed: {error}"),
                )
            });
    }
    #[cfg(not(target_os = "macos"))]
    Err(CaptureError::new(
        CaptureErrorCode::CaptureFailed,
        "screen capture permission requests are available only on macOS",
    ))
}

#[tauri::command]
pub fn screen_capture_open_permission_settings(
    window: WebviewWindow,
) -> Result<(), CaptureError> {
    authorize_permission_command_caller(window.label())?;
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_opener::OpenerExt;

        return window
            .opener()
            .open_url(MACOS_SCREEN_CAPTURE_SETTINGS_URL, None::<&str>)
            .map_err(|error| {
                CaptureError::new(
                    CaptureErrorCode::CaptureFailed,
                    format!("could not open macOS screen capture settings: {error}"),
                )
            });
    }
    #[cfg(not(target_os = "macos"))]
    Err(CaptureError::new(
        CaptureErrorCode::CaptureFailed,
        "screen capture permission settings are available only on macOS",
    ))
}

#[tauri::command]
pub fn screen_capture_report_client_error(
    window: WebviewWindow,
    detail: String,
) -> Result<(), CaptureError> {
    if !is_regular_window_label(window.label()) {
        return Err(CaptureError::new(
            CaptureErrorCode::UnauthorizedCaller,
            "only a regular application window may report a capture client failure",
        ));
    }
    log::error!(
        "screen capture client [{}]: {}",
        window.label(),
        bounded_client_error_detail(&detail)
    );
    Ok(())
}

#[tauri::command]
pub fn screen_capture_report_bootstrap_error(
    window: WebviewWindow,
    runtime: State<'_, ScreenCaptureRuntime>,
    overlay_generation: u64,
    detail: String,
) -> Result<OverlayInit, CaptureError> {
    runtime.authorize_overlay_identity(window.label(), overlay_generation)?;
    log::error!(
        "screen capture overlay bootstrap failed generation={}: {}",
        overlay_generation,
        bounded_client_error_detail(&detail)
    );
    let windows = TauriCaptureWindowPort::new(window.app_handle().clone());
    let init = runtime.overlay_bootstrap_failed(window.label(), overlay_generation, &windows)?;
    let app = window.app_handle().clone();
    let label = window.label().to_string();
    // A failed bootstrap document can never become ready. Retire it only
    // after this invoke has returned so the next trigger allocates a fresh
    // generation instead of waiting on the permanently failed WebView.
    tauri::async_runtime::spawn(async move {
        tokio::task::yield_now().await;
        if let Some(window) = app.get_webview_window(&label)
            && let Err(error) = window.destroy()
        {
            log::warn!("destroy failed capture bootstrap overlay failed: {error}");
        }
        // Do not let a platform destroy failure strand capture in Busy forever.
        // Overlay labels are generation-qualified, so a later trigger can safely
        // create a replacement even if this failed bootstrap window survived.
        if let Err(error) = app
            .state::<ScreenCaptureRuntime>()
            .finish_overlay_retirement(&label)
        {
            log::warn!("capture bootstrap overlay retirement cleanup failed: {error}");
        }
    });
    Ok(init)
}

#[tauri::command]
pub fn screen_capture_register_target(
    window: WebviewWindow,
    runtime: State<'_, ScreenCaptureRuntime>,
    target_token: String,
) -> Result<(), CaptureError> {
    let result = runtime.register_eligible_target(window.label(), &target_token);
    log::info!(
        "screen capture target registration caller={} success={}",
        window.label(),
        result.is_ok()
    );
    result
}

#[tauri::command]
pub fn screen_capture_unregister_target(
    window: WebviewWindow,
    runtime: State<'_, ScreenCaptureRuntime>,
    target_token: String,
) -> Result<(), CaptureError> {
    runtime.unregister_eligible_target(window.label(), &target_token)
}

#[tauri::command]
pub async fn screen_capture_start(
    window: WebviewWindow,
    runtime: State<'_, ScreenCaptureRuntime>,
    target_window_label: String,
    target_token: String,
) -> Result<CaptureStartResponse, CaptureError> {
    let caller_window_label = window.label().to_string();
    log::info!("screen capture start entered caller={caller_window_label}");
    let result: Result<CaptureStartResponse, CaptureError> = async {
        let app = window.app_handle().clone();
        tauri::async_runtime::spawn_blocking(super::platform::preflight_capture_permission)
            .await
            .map_err(|error| {
                CaptureError::new(
                    CaptureErrorCode::OverlayFailed,
                    format!("capture permission preflight worker failed: {error}"),
                )
            })??;
        let reservation_app = app.clone();
        let reservation_caller = caller_window_label.clone();
        let reservation = tauri::async_runtime::spawn_blocking(move || {
            let runtime = reservation_app.state::<ScreenCaptureRuntime>();
            let windows = TauriCaptureWindowPort::new(reservation_app.clone());
            runtime.reserve_composer_capture(
                &reservation_caller,
                new_capture_session_id(),
                CaptureTarget {
                    window_label: target_window_label,
                    target_token,
                },
                &windows,
            )
        })
        .await
        .map_err(|error| {
            CaptureError::new(
                CaptureErrorCode::OverlayFailed,
                format!("capture reservation worker failed: {error}"),
            )
        })??;
        let mut response = reservation.response;
        schedule_capture_timeouts(app.clone(), &response);
        if let Some(ticket) = reservation.ticket {
            finish_reserved_capture(app, &runtime, ticket).await?;
            response.phase = runtime.active_phase()?;
        }
        Ok(response)
    }
    .await;
    if let Err(error) = &result {
        log::error!(
            "screen capture start failed caller={} code={:?} detail={}",
            caller_window_label,
            error.code,
            error.detail
        );
    } else if let Ok(response) = &result {
        log::info!(
            "screen capture start completed caller={} generation={} phase={:?}",
            caller_window_label,
            response.overlay_generation,
            response.phase
        );
    }
    result
}

#[tauri::command]
pub async fn screen_capture_ready(
    window: WebviewWindow,
    runtime: State<'_, ScreenCaptureRuntime>,
    overlay_generation: u64,
    protocol_version: u32,
) -> Result<NativeCapturePhase, CaptureError> {
    let caller_window_label = window.label().to_string();
    log::info!(
        "screen capture overlay ready entered caller={} generation={}",
        caller_window_label,
        overlay_generation
    );
    let result: Result<NativeCapturePhase, CaptureError> = async {
        let app = window.app_handle().clone();
        let windows = TauriCaptureWindowPort::new(app.clone());
        let (phase, ticket) = runtime.mark_overlay_ready(
            &caller_window_label,
            overlay_generation,
            protocol_version,
            &windows,
        )?;
        if let Some(ticket) = ticket {
            finish_reserved_capture(app, &runtime, ticket).await?;
            runtime.active_phase()
        } else {
            Ok(phase)
        }
    }
    .await;
    if let Err(error) = &result {
        log::error!(
            "screen capture overlay ready failed caller={} generation={} code={:?} detail={}",
            caller_window_label,
            overlay_generation,
            error.code,
            error.detail
        );
    } else if let Ok(phase) = &result {
        log::info!(
            "screen capture overlay ready completed caller={} generation={} phase={:?}",
            caller_window_label,
            overlay_generation,
            phase
        );
    }
    result
}

#[tauri::command]
pub fn screen_capture_take_frame(
    window: WebviewWindow,
    runtime: State<'_, ScreenCaptureRuntime>,
    session_id: String,
    overlay_generation: u64,
) -> Result<Response, CaptureError> {
    log::info!(
        "screen capture overlay taking frame caller={} generation={}",
        window.label(),
        overlay_generation
    );
    let result = runtime
        .take_frame(window.label(), &session_id, overlay_generation)
        .map(raw_response);
    log::info!(
        "screen capture overlay frame read success={}",
        result.is_ok()
    );
    result
}

#[tauri::command]
pub fn screen_capture_pending_frame(
    window: WebviewWindow,
    runtime: State<'_, ScreenCaptureRuntime>,
    overlay_generation: u64,
) -> Result<Option<super::runtime::FrameAvailablePayload>, CaptureError> {
    #[cfg(target_os = "windows")]
    log::info!(
        "screen capture pending-frame command entered caller={} generation={}",
        window.label(),
        overlay_generation
    );
    let windows = TauriCaptureWindowPort::new(window.app_handle().clone());
    let result = runtime.pending_frame(window.label(), overlay_generation, &windows);
    #[cfg(target_os = "windows")]
    log::info!(
        "screen capture pending-frame command completed caller={} generation={} success={} frame={}",
        window.label(),
        overlay_generation,
        result.is_ok(),
        matches!(result, Ok(Some(_)))
    );
    result
}

#[tauri::command]
pub fn screen_capture_frame_presented(
    window: WebviewWindow,
    runtime: State<'_, ScreenCaptureRuntime>,
    session_id: String,
    overlay_generation: u64,
) -> Result<(), CaptureError> {
    let windows = TauriCaptureWindowPort::new(window.app_handle().clone());
    let result = runtime.frame_presented(window.label(), &session_id, overlay_generation, &windows);
    log::info!(
        "screen capture overlay presentation acknowledgment success={}",
        result.is_ok()
    );
    result
}

#[cfg(target_os = "macos")]
async fn macos_overlay_work_area(window: &WebviewWindow) -> Result<Option<CssRect>, CaptureError> {
    use objc2_app_kit::NSWindow;

    let (sender, receiver) = tokio::sync::oneshot::channel();
    let main_window = window.clone();
    window
        .run_on_main_thread(move || {
            let result = (|| {
                let pointer = main_window.ns_window().map_err(|error| {
                    CaptureError::new(
                        CaptureErrorCode::OverlayFailed,
                        format!("read capture overlay native window: {error}"),
                    )
                })?;
                // SAFETY: Tauri owns this NSWindow for the lifetime of
                // `main_window`, and this closure is running on AppKit's main
                // thread as required by NSWindow/NSScreen.
                let native_window = unsafe { &*pointer.cast::<NSWindow>() };
                let Some(screen) = native_window.screen() else {
                    return Ok(None);
                };
                let frame = screen.frame();
                let visible = screen.visibleFrame();
                super::platform::bottom_left_visible_frame_to_top_left_area(
                    super::contract::LogicalPoint {
                        x: frame.origin.x,
                        y: frame.origin.y,
                    },
                    super::contract::LogicalSize {
                        width: frame.size.width,
                        height: frame.size.height,
                    },
                    super::contract::LogicalPoint {
                        x: visible.origin.x,
                        y: visible.origin.y,
                    },
                    super::contract::LogicalSize {
                        width: visible.size.width,
                        height: visible.size.height,
                    },
                )
                .map(Some)
            })();
            let _ = sender.send(result);
        })
        .map_err(|error| {
            CaptureError::new(
                CaptureErrorCode::OverlayFailed,
                format!("schedule capture work-area query: {error}"),
            )
        })?;
    receiver.await.map_err(|_| {
        CaptureError::new(
            CaptureErrorCode::OverlayFailed,
            "capture work-area query ended without a result",
        )
    })?
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn native_overlay_work_area(window: &WebviewWindow) -> Result<Option<CssRect>, CaptureError> {
    let Some(monitor) = window.current_monitor().map_err(|error| {
        CaptureError::new(
            CaptureErrorCode::OverlayFailed,
            format!("read capture overlay monitor: {error}"),
        )
    })?
    else {
        return Ok(None);
    };
    let frame_origin = monitor.position();
    let frame_size = monitor.size();
    let work_area = monitor.work_area();
    super::platform::top_left_physical_work_area_to_local_css(
        super::contract::PhysicalPoint {
            x: frame_origin.x,
            y: frame_origin.y,
        },
        super::contract::PhysicalSize {
            width: frame_size.width,
            height: frame_size.height,
        },
        super::contract::PhysicalPoint {
            x: work_area.position.x,
            y: work_area.position.y,
        },
        super::contract::PhysicalSize {
            width: work_area.size.width,
            height: work_area.size.height,
        },
        monitor.scale_factor(),
    )
    .map(Some)
}

#[tauri::command]
pub async fn screen_capture_overlay_work_area(
    window: WebviewWindow,
    runtime: State<'_, ScreenCaptureRuntime>,
    overlay_generation: u64,
) -> Result<Option<CssRect>, CaptureError> {
    runtime.authorize_overlay_identity(window.label(), overlay_generation)?;
    #[cfg(target_os = "macos")]
    {
        macos_overlay_work_area(&window).await
    }
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        native_overlay_work_area(&window)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Ok(None)
    }
}

#[tauri::command]
pub fn screen_capture_submit_result(
    window: WebviewWindow,
    runtime: State<'_, ScreenCaptureRuntime>,
    request: Request<'_>,
) -> Result<CaptureResultDescriptor, CaptureError> {
    let session_id = required_header(&request, RESULT_SESSION_HEADER)?.to_string();
    let overlay_generation = parsed_header::<u64>(&request, RESULT_GENERATION_HEADER)?;
    runtime.authorize_overlay_identity(window.label(), overlay_generation)?;
    let width = parsed_header::<u32>(&request, RESULT_WIDTH_HEADER)?;
    let height = parsed_header::<u32>(&request, RESULT_HEIGHT_HEADER)?;
    let bytes = require_raw_body(request.body())?;
    if bytes.len() > MAX_PNG_RESULT_BYTES {
        return Err(CaptureError::new(
            CaptureErrorCode::FrameTooLarge,
            "capture result exceeds the process memory limit",
        ));
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| {
            CaptureError::new(CaptureErrorCode::EncodeFailed, "system time is unavailable")
        })?
        .as_millis();
    runtime.store_result(
        window.label(),
        &session_id,
        overlay_generation,
        CaptureResultSubmission {
            result_id: new_capture_result_id(),
            filename: stable_png_filename(now),
            width,
            height,
            bytes: bytes.to_vec(),
        },
    )
}

#[tauri::command]
pub fn screen_capture_send_result(
    window: WebviewWindow,
    runtime: State<'_, ScreenCaptureRuntime>,
    session_id: String,
    result_id: String,
    overlay_generation: u64,
) -> Result<(), CaptureError> {
    let windows = TauriCaptureWindowPort::new(window.app_handle().clone());
    runtime.publish_result(
        window.label(),
        &session_id,
        &result_id,
        overlay_generation,
        &windows,
    )
}

#[tauri::command]
pub fn screen_capture_take_result(
    window: WebviewWindow,
    runtime: State<'_, ScreenCaptureRuntime>,
    session_id: String,
    result_id: String,
    target_token: String,
) -> Result<Response, CaptureError> {
    runtime
        .take_result(
            window.label(),
            &session_id,
            &result_id,
            &target_token,
            new_capture_delivery_lease_id(),
        )
        .map(raw_response)
}

#[tauri::command]
pub fn screen_capture_release_result(
    window: WebviewWindow,
    runtime: State<'_, ScreenCaptureRuntime>,
    session_id: String,
    result_id: String,
    target_token: String,
) -> Result<(), CaptureError> {
    let windows = TauriCaptureWindowPort::new(window.app_handle().clone());
    runtime.release_result(
        window.label(),
        &session_id,
        &result_id,
        &target_token,
        &windows,
    )
}

#[tauri::command]
pub fn screen_capture_ack_result(
    window: WebviewWindow,
    runtime: State<'_, ScreenCaptureRuntime>,
    session_id: String,
    result_id: String,
    target_token: String,
) -> Result<(), CaptureError> {
    let windows = TauriCaptureWindowPort::new(window.app_handle().clone());
    runtime.ack_result(
        window.label(),
        &session_id,
        &result_id,
        &target_token,
        &windows,
    )
}

#[tauri::command]
pub async fn screen_capture_save_result(
    window: WebviewWindow,
    runtime: State<'_, ScreenCaptureRuntime>,
    session_id: String,
    result_id: String,
    overlay_generation: u64,
) -> Result<SaveCaptureOutcome, CaptureError> {
    let app = window.app_handle().clone();
    let windows = TauriCaptureWindowPort::new(app.clone());
    let exports = TauriCaptureExportPort::new(app);
    runtime.save_result(
        window.label(),
        &session_id,
        &result_id,
        overlay_generation,
        &exports,
        &windows,
    )
}

#[tauri::command]
pub fn screen_capture_copy_result(
    window: WebviewWindow,
    runtime: State<'_, ScreenCaptureRuntime>,
    session_id: String,
    result_id: String,
    overlay_generation: u64,
) -> Result<(), CaptureError> {
    let app = window.app_handle().clone();
    let windows = TauriCaptureWindowPort::new(app.clone());
    let exports = TauriCaptureExportPort::new(app);
    runtime.copy_result(
        window.label(),
        &session_id,
        &result_id,
        overlay_generation,
        &exports,
        &windows,
    )
}

#[tauri::command]
pub fn screen_capture_discard_result(
    window: WebviewWindow,
    runtime: State<'_, ScreenCaptureRuntime>,
    session_id: String,
    result_id: String,
    overlay_generation: u64,
) -> Result<NativeCapturePhase, CaptureError> {
    runtime.discard_result(window.label(), &session_id, &result_id, overlay_generation)
}

#[tauri::command]
pub fn screen_capture_invalidate_target(
    window: WebviewWindow,
    runtime: State<'_, ScreenCaptureRuntime>,
    session_id: String,
    target_token: String,
) -> Result<(), CaptureError> {
    let windows = TauriCaptureWindowPort::new(window.app_handle().clone());
    runtime.invalidate_target_from_window(window.label(), &session_id, &target_token, &windows)
}

#[tauri::command]
pub fn screen_capture_fail(
    window: WebviewWindow,
    runtime: State<'_, ScreenCaptureRuntime>,
    session_id: String,
    overlay_generation: u64,
    code: String,
    detail: String,
) -> Result<(), CaptureError> {
    let windows = TauriCaptureWindowPort::new(window.app_handle().clone());
    let result = runtime.fail_from_overlay(
        window.label(),
        &session_id,
        overlay_generation,
        &code,
        &detail,
        &windows,
    );
    log::warn!(
        "screen capture overlay reported failure code={} accepted={}",
        code,
        result.is_ok()
    );
    result
}

#[tauri::command]
pub fn screen_capture_cancel(
    window: WebviewWindow,
    runtime: State<'_, ScreenCaptureRuntime>,
    session_id: String,
    overlay_generation: Option<u64>,
) -> Result<(), CaptureError> {
    let windows = TauriCaptureWindowPort::new(window.app_handle().clone());
    runtime.cancel_from_window(window.label(), &session_id, overlay_generation, &windows)
}

#[tauri::command]
pub fn screen_capture_unavailable(
    window: WebviewWindow,
    runtime: State<'_, ScreenCaptureRuntime>,
    overlay_generation: u64,
) -> Result<OverlayInit, CaptureError> {
    let windows = TauriCaptureWindowPort::new(window.app_handle().clone());
    runtime.overlay_unavailable(window.label(), overlay_generation, &windows)
}

/// Hook for the application's global `WindowEvent::Destroyed` branch. Related
/// origin/target destruction terminates the session. For a retained overlay,
/// the delay covers rebuilding its replacement; session buffer cleanup and
/// origin restoration run synchronously before this function returns.
pub fn on_window_destroyed(app: &AppHandle, label: &str) {
    if !is_overlay_window_label(label) {
        let runtime = app.state::<ScreenCaptureRuntime>();
        let windows = TauriCaptureWindowPort::new(app.clone());
        if let Err(error) = runtime.prepare_window_close(label, &windows) {
            log::warn!("screen capture participant destruction cleanup failed: {error}");
        }
        return;
    }
    let runtime = app.state::<ScreenCaptureRuntime>();
    let windows = TauriCaptureWindowPort::new(app.clone());
    let rebuild = match runtime.overlay_destroyed(label, &windows) {
        Ok(rebuild) => rebuild,
        Err(error) => {
            log::warn!("screen capture overlay destruction cleanup failed: {error}");
            return;
        }
    };
    if !rebuild {
        return;
    }

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = tauri::async_runtime::spawn_blocking(|| {
            std::thread::sleep(std::time::Duration::from_millis(50));
        })
        .await;
        let runtime = app.state::<ScreenCaptureRuntime>();
        let windows = TauriCaptureWindowPort::new(app.clone());
        if let Err(error) = runtime.ensure_overlay_native(&windows) {
            log::warn!("screen capture overlay rebuild failed: {error}");
        }
    });
}

/// Hook for the application's global `WindowEvent::CloseRequested` branch.
/// Call this for every window before allowing it to close so a capture origin
/// is restored while both native windows still exist.
pub fn on_window_close_requested(app: &AppHandle, label: &str) {
    let runtime = app.state::<ScreenCaptureRuntime>();
    let windows = TauriCaptureWindowPort::new(app.clone());
    if let Err(error) = runtime.prepare_window_close(label, &windows) {
        log::warn!("screen capture window close cleanup failed: {error}");
    }
}

pub(crate) async fn finish_reserved_capture<R: Runtime>(
    app: AppHandle<R>,
    runtime: &ScreenCaptureRuntime,
    ticket: CaptureTicket,
) -> Result<CapturePublishOutcome, CaptureError> {
    let windows = TauriCaptureWindowPort::new(app.clone());
    let session_id = ticket.session_id().to_string();
    let timeout_response = CaptureStartResponse {
        session_id: session_id.clone(),
        overlay_generation: ticket.overlay_generation(),
        phase: NativeCapturePhase::Capturing,
    };
    let timeout_app = app.clone();
    let outcome = acquire_and_publish_once(
        runtime,
        ticket,
        &windows,
        COMPOSITOR_UNMAP_SETTLE,
        CAPTURE_BACKEND_TIMEOUT,
        move || acquire_native_frame(app, session_id),
    )
    .await?;
    if outcome == CapturePublishOutcome::Published {
        schedule_capture_timeout(
            timeout_app,
            &timeout_response,
            CaptureTimeoutKind::Presentation,
            CAPTURE_PRESENTATION_TIMEOUT,
        );
    }
    Ok(outcome)
}

async fn acquire_native_frame<R: Runtime>(
    app: AppHandle<R>,
    session_id: String,
) -> Result<CapturedFrame, CaptureError> {
    #[cfg(target_os = "linux")]
    if super::platform::wayland_cursor_is_unavailable(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
    ) {
        let monitors = super::wayland::available_monitor_geometries(&app)?;
        let (monitor, native) = super::wayland::capture_monitor_via_portal(
            monitors,
            PORTAL_INTERACTION_TIMEOUT,
            PORTAL_FRAME_TIMEOUT,
        )
        .await?;
        return CapturedFrame::new(
            session_id,
            monitor,
            native.width,
            native.height,
            native.stride,
            native.bytes,
        );
    }

    tauri::async_runtime::spawn_blocking(move || {
        let backend = XcapBackend::new(app);
        capture_frame_at_cursor_exclusive(&backend, &session_id)
    })
    .await
    .map_err(|_| {
        CaptureError::new(
            CaptureErrorCode::CaptureFailed,
            "screen capture backend task failed",
        )
    })?
}

/// Session IDs are correlation handles, not caller-provided authority. Native
/// shortcut wiring should use this helper too.
pub(crate) fn new_capture_session_id() -> String {
    new_random_id("capture")
}

pub(crate) fn schedule_capture_timeouts<R: Runtime>(
    app: AppHandle<R>,
    response: &CaptureStartResponse,
) {
    schedule_capture_timeout(
        app.clone(),
        response,
        CaptureTimeoutKind::Readiness,
        CAPTURE_READINESS_TIMEOUT,
    );
    schedule_capture_timeout(
        app,
        response,
        CaptureTimeoutKind::Lifetime,
        CAPTURE_LIFETIME_TIMEOUT,
    );
}

fn schedule_capture_timeout<R: Runtime>(
    app: AppHandle<R>,
    response: &CaptureStartResponse,
    kind: CaptureTimeoutKind,
    delay: std::time::Duration,
) {
    let session_id = response.session_id.clone();
    let overlay_generation = response.overlay_generation;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(delay).await;
        let runtime = app.state::<ScreenCaptureRuntime>();
        let windows = TauriCaptureWindowPort::new(app.clone());
        if let Err(error) = runtime.expire_session(&session_id, overlay_generation, kind, &windows)
        {
            log::warn!("screen capture timeout cleanup failed: {error}");
        }
    });
}

pub(crate) fn new_capture_result_id() -> String {
    new_random_id("result")
}

fn new_capture_delivery_lease_id() -> String {
    new_random_id("lease")
}

fn new_random_id(prefix: &str) -> String {
    let mut bytes = [0_u8; 16];
    OsRng.fill_bytes(&mut bytes);
    let mut encoded = String::with_capacity(prefix.len() + 1 + bytes.len() * 2);
    encoded.push_str(prefix);
    encoded.push('-');
    for byte in bytes {
        use std::fmt::Write;
        write!(&mut encoded, "{byte:02x}").expect("writing to String is infallible");
    }
    encoded
}

fn required_header<'a>(request: &'a Request<'_>, name: &str) -> Result<&'a str, CaptureError> {
    request
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            CaptureError::new(
                CaptureErrorCode::InvalidFrame,
                format!("capture result request is missing required header {name}"),
            )
        })
}

fn parsed_header<T>(request: &Request<'_>, name: &str) -> Result<T, CaptureError>
where
    T: std::str::FromStr,
{
    required_header(request, name)?.parse().map_err(|_| {
        CaptureError::new(
            CaptureErrorCode::InvalidFrame,
            format!("capture result header {name} is invalid"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_CLIENT_ERROR_DETAIL_CHARS, authorize_permission_command_caller,
        bounded_client_error_detail, new_capture_delivery_lease_id, new_capture_result_id,
        new_capture_session_id,
    };

    #[test]
    fn client_error_detail_is_bounded_and_cannot_forge_log_lines() {
        assert_eq!(
            bounded_client_error_detail("failure\nforged\tline"),
            "failure forged line"
        );
        assert_eq!(
            bounded_client_error_detail("\n\t"),
            "screen capture client reported an unspecified failure"
        );
        assert_eq!(
            bounded_client_error_detail(&"x".repeat(MAX_CLIENT_ERROR_DETAIL_CHARS + 1))
                .chars()
                .count(),
            MAX_CLIENT_ERROR_DETAIL_CHARS
        );
    }

    #[test]
    fn permission_commands_reject_utility_window_callers() {
        assert!(authorize_permission_command_caller("main").is_ok());
        assert!(authorize_permission_command_caller("window-chat").is_ok());
        let error = authorize_permission_command_caller("screen-capture-overlay-7")
            .expect_err("capture overlays cannot manage screen capture permission");
        assert_eq!(error.code, super::CaptureErrorCode::UnauthorizedCaller);
    }

    #[test]
    fn native_session_ids_are_nonempty_and_not_reused() {
        let first = new_capture_session_id();
        let second = new_capture_session_id();
        assert!(first.starts_with("capture-"));
        assert_eq!(first.len(), "capture-".len() + 32);
        assert_ne!(first, second);
    }

    #[test]
    fn result_and_lease_authority_are_native_generated_and_namespaced() {
        let result = new_capture_result_id();
        let lease = new_capture_delivery_lease_id();
        assert!(result.starts_with("result-"));
        assert!(lease.starts_with("lease-"));
        assert_ne!(result, lease);
    }
}
