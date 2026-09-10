pub mod backend;
pub mod buffers;
pub mod commands;
pub mod contract;
pub mod coordinator;
pub mod export;
pub mod ipc;
pub mod platform;
pub mod runtime;
pub mod session;
pub mod shortcut;
#[cfg(target_os = "linux")]
pub mod wayland;
pub mod window;

#[cfg(test)]
pub(crate) fn png_fixture(width: u32, height: u32) -> Vec<u8> {
    use xcap::image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};

    let pixels = vec![0x5a; width as usize * height as usize * 4];
    let mut encoded = Vec::new();
    PngEncoder::new(&mut encoded)
        .write_image(&pixels, width, height, ExtendedColorType::Rgba8)
        .expect("encode PNG fixture");
    encoded
}

#[cfg(test)]
mod contract_tests {
    use super::contract::{
        CaptureErrorCode, CaptureOrigin, CaptureRequest, CaptureTarget, CaptureTriggerKind,
        CapturedFrame, LogicalPoint, LogicalSize, MAX_RAW_FRAME_BYTES, MonitorGeometry,
        NativeCapturePhase, PhysicalPoint, PhysicalSize, select_monitor_at,
    };

    fn monitor(
        id: &str,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        scale_factor: f64,
    ) -> MonitorGeometry {
        MonitorGeometry {
            id: id.to_string(),
            physical_origin: PhysicalPoint { x, y },
            physical_size: PhysicalSize { width, height },
            logical_origin: LogicalPoint {
                x: f64::from(x) / scale_factor,
                y: f64::from(y) / scale_factor,
            },
            logical_size: LogicalSize {
                width: f64::from(width) / scale_factor,
                height: f64::from(height) / scale_factor,
            },
            scale_factor,
        }
    }

    #[test]
    fn selects_monitor_using_half_open_physical_bounds() {
        let monitors = [
            monitor("left", -1920, 0, 1920, 1080, 1.0),
            monitor("main", 0, 0, 2560, 1440, 1.25),
        ];

        assert_eq!(
            select_monitor_at(&monitors, PhysicalPoint { x: -1, y: 100 })
                .map(|item| item.id.as_str()),
            Some("left")
        );
        assert_eq!(
            select_monitor_at(&monitors, PhysicalPoint { x: 0, y: 100 })
                .map(|item| item.id.as_str()),
            Some("main")
        );
        assert!(select_monitor_at(&monitors, PhysicalPoint { x: 2560, y: 100 }).is_none());
    }

    #[test]
    fn monitor_selection_handles_origins_near_integer_limits() {
        let monitors = [monitor("edge", i32::MAX - 99, i32::MIN, 100, 200, 1.0)];

        assert_eq!(
            select_monitor_at(
                &monitors,
                PhysicalPoint {
                    x: i32::MAX,
                    y: i32::MIN + 199,
                },
            )
            .map(|item| item.id.as_str()),
            Some("edge")
        );
    }

    #[test]
    fn frame_validation_accepts_tightly_packed_rgba() {
        let frame = CapturedFrame::new(
            "capture-1",
            monitor("main", 0, 0, 2, 2, 1.0),
            2,
            2,
            8,
            vec![0; 16],
        )
        .expect("valid frame");

        assert_eq!(frame.bytes().len(), 16);
        assert_eq!(frame.descriptor().byte_len, 16);
    }

    #[test]
    fn frame_validation_rejects_short_stride_and_length_mismatch() {
        let short_stride = CapturedFrame::new(
            "capture-1",
            monitor("main", 0, 0, 2, 2, 1.0),
            2,
            2,
            7,
            vec![0; 14],
        )
        .expect_err("stride cannot be shorter than RGBA row");
        assert_eq!(short_stride.code, CaptureErrorCode::InvalidFrame);

        let wrong_length = CapturedFrame::new(
            "capture-1",
            monitor("main", 0, 0, 2, 2, 1.0),
            2,
            2,
            8,
            vec![0; 15],
        )
        .expect_err("buffer length must match stride times height");
        assert_eq!(wrong_length.code, CaptureErrorCode::InvalidFrame);
    }

    #[test]
    fn frame_validation_checks_overflow_before_allocation_contract() {
        let error = CapturedFrame::validate_layout(u32::MAX, u32::MAX, u32::MAX, 0)
            .expect_err("overflowing dimensions must fail closed");
        assert_eq!(error.code, CaptureErrorCode::FrameTooLarge);
    }

    #[test]
    fn frame_validation_enforces_process_memory_cap() {
        let error = CapturedFrame::validate_layout(
            1,
            2,
            u32::try_from(MAX_RAW_FRAME_BYTES).expect("cap fits u32"),
            MAX_RAW_FRAME_BYTES + 1,
        )
        .expect_err("oversized frames must fail closed");
        assert_eq!(error.code, CaptureErrorCode::FrameTooLarge);
    }

    #[test]
    fn monitor_geometry_rejects_zero_size_and_invalid_scale() {
        let zero = monitor("zero", 0, 0, 0, 100, 1.0)
            .validate()
            .expect_err("zero-width monitor is invalid");
        assert_eq!(zero.code, CaptureErrorCode::InvalidMonitor);

        let scale = monitor("scale", 0, 0, 100, 100, f64::NAN)
            .validate()
            .expect_err("non-finite scale is invalid");
        assert_eq!(scale.code, CaptureErrorCode::InvalidMonitor);
    }

    #[test]
    fn capture_request_keeps_origin_and_delivery_target_independent() {
        let request = CaptureRequest {
            session_id: "capture-1".to_string(),
            trigger: CaptureTriggerKind::Global,
            origin: None,
            target: Some(CaptureTarget {
                window_label: "main".to_string(),
                target_token: "chat-token".to_string(),
            }),
        };

        request.validate().expect("valid global target request");
        assert!(request.origin.is_none());

        let invalid = CaptureRequest {
            session_id: " ".to_string(),
            trigger: CaptureTriggerKind::Composer,
            origin: Some(CaptureOrigin {
                window_label: "main".to_string(),
            }),
            target: None,
        };
        assert_eq!(
            invalid
                .validate()
                .expect_err("empty session id must fail")
                .code,
            CaptureErrorCode::InvalidSession
        );
    }

    #[test]
    fn native_phase_contract_does_not_claim_frontend_annotation_state() {
        let phase = NativeCapturePhase::Active;
        let json = serde_json::to_string(&phase).expect("serialize native phase");

        assert_eq!(json, "\"active\"");
        assert!(!json.contains("annotating"));
    }
}

#[cfg(test)]
mod backend_tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::backend::{NativeFrame, ScreenCaptureBackend, capture_frame_at_cursor};
    use super::contract::{
        CaptureError, CaptureErrorCode, LogicalPoint, LogicalSize, MonitorGeometry, PhysicalPoint,
        PhysicalSize,
    };

    struct FakeBackend {
        monitor_snapshots: Mutex<VecDeque<Vec<MonitorGeometry>>>,
        cursor: PhysicalPoint,
        frame: Mutex<Option<Result<NativeFrame, CaptureError>>>,
    }

    impl ScreenCaptureBackend for FakeBackend {
        fn monitors(&self) -> Result<Vec<MonitorGeometry>, CaptureError> {
            self.monitor_snapshots
                .lock()
                .expect("monitor snapshots lock")
                .pop_front()
                .ok_or_else(|| CaptureError::new(CaptureErrorCode::NoMonitor, "no snapshot"))
        }

        fn monitor_index_at_cursor(
            &self,
            monitors: &[MonitorGeometry],
        ) -> Result<usize, CaptureError> {
            super::contract::select_monitor_at(monitors, self.cursor)
                .and_then(|selected| {
                    monitors
                        .iter()
                        .position(|monitor| monitor.id == selected.id)
                })
                .ok_or_else(|| {
                    CaptureError::new(
                        CaptureErrorCode::NoMonitor,
                        "the pointer is not inside an active monitor",
                    )
                })
        }

        fn capture_monitor(&self, _monitor: &MonitorGeometry) -> Result<NativeFrame, CaptureError> {
            self.frame
                .lock()
                .expect("frame lock")
                .take()
                .expect("one frame configured")
        }
    }

    fn monitor(id: &str, x: i32, width: u32) -> MonitorGeometry {
        MonitorGeometry {
            id: id.to_string(),
            physical_origin: PhysicalPoint { x, y: 0 },
            physical_size: PhysicalSize { width, height: 2 },
            logical_origin: LogicalPoint {
                x: f64::from(x),
                y: 0.0,
            },
            logical_size: LogicalSize {
                width: f64::from(width),
                height: 2.0,
            },
            scale_factor: 1.0,
        }
    }

    fn frame(width: u32) -> NativeFrame {
        NativeFrame {
            width,
            height: 2,
            stride: width * 4,
            bytes: vec![0; width as usize * 8],
        }
    }

    #[test]
    fn capture_reenumerates_monitors_and_uses_the_latest_snapshot() {
        let backend = FakeBackend {
            monitor_snapshots: Mutex::new(VecDeque::from([
                vec![monitor("removed", 100, 2)],
                vec![monitor("current", 0, 2)],
            ])),
            cursor: PhysicalPoint { x: 1, y: 1 },
            frame: Mutex::new(Some(Ok(frame(2)))),
        };

        let first = capture_frame_at_cursor(&backend, "first")
            .expect_err("the cursor is outside the stale snapshot");
        assert_eq!(first.code, CaptureErrorCode::NoMonitor);

        let second = capture_frame_at_cursor(&backend, "second").expect("fresh snapshot captures");
        assert_eq!(second.descriptor().monitor.id, "current");
    }

    #[test]
    fn capture_rejects_empty_enumeration_and_cursor_outside_displays() {
        let no_monitors = FakeBackend {
            monitor_snapshots: Mutex::new(VecDeque::from([vec![]])),
            cursor: PhysicalPoint { x: 0, y: 0 },
            frame: Mutex::new(None),
        };
        assert_eq!(
            capture_frame_at_cursor(&no_monitors, "empty")
                .expect_err("empty enumeration")
                .code,
            CaptureErrorCode::NoMonitor
        );

        let outside = FakeBackend {
            monitor_snapshots: Mutex::new(VecDeque::from([vec![monitor("main", 0, 2)]])),
            cursor: PhysicalPoint { x: 10, y: 10 },
            frame: Mutex::new(None),
        };
        assert_eq!(
            capture_frame_at_cursor(&outside, "outside")
                .expect_err("cursor outside displays")
                .code,
            CaptureErrorCode::NoMonitor
        );
    }

    #[test]
    fn capture_validates_backend_frame_before_returning_it() {
        let backend = FakeBackend {
            monitor_snapshots: Mutex::new(VecDeque::from([vec![monitor("main", 0, 2)]])),
            cursor: PhysicalPoint { x: 1, y: 1 },
            frame: Mutex::new(Some(Ok(frame(1)))),
        };

        assert_eq!(
            capture_frame_at_cursor(&backend, "bad-frame")
                .expect_err("backend returned wrong dimensions")
                .code,
            CaptureErrorCode::InvalidFrame
        );
    }

    #[test]
    fn backend_permission_and_capture_failures_keep_stable_error_codes() {
        for (code, detail) in [
            (CaptureErrorCode::PermissionDenied, "screen capture denied"),
            (CaptureErrorCode::CaptureFailed, "native capture failed"),
        ] {
            let backend = FakeBackend {
                monitor_snapshots: Mutex::new(VecDeque::from([vec![monitor("main", 0, 2)]])),
                cursor: PhysicalPoint { x: 1, y: 1 },
                frame: Mutex::new(Some(Err(CaptureError::new(code, detail)))),
            };

            assert_eq!(
                capture_frame_at_cursor(&backend, "failed")
                    .expect_err("backend failure must propagate")
                    .code,
                code
            );
        }
    }

    #[test]
    fn oversized_monitor_is_rejected_before_the_backend_allocates_a_frame() {
        let backend = FakeBackend {
            monitor_snapshots: Mutex::new(VecDeque::from([vec![MonitorGeometry {
                id: "huge".to_string(),
                physical_origin: PhysicalPoint { x: 0, y: 0 },
                physical_size: PhysicalSize {
                    width: 20_000,
                    height: 20_000,
                },
                logical_origin: LogicalPoint { x: 0.0, y: 0.0 },
                logical_size: LogicalSize {
                    width: 20_000.0,
                    height: 20_000.0,
                },
                scale_factor: 1.0,
            }]])),
            cursor: PhysicalPoint { x: 1, y: 1 },
            frame: Mutex::new(None),
        };

        assert_eq!(
            capture_frame_at_cursor(&backend, "too-large")
                .expect_err("oversized monitor must fail before capture")
                .code,
            CaptureErrorCode::FrameTooLarge
        );
    }
}

#[cfg(test)]
mod buffer_tests {
    use super::buffers::SessionBuffers;
    use super::contract::{
        CaptureErrorCode, CaptureResultDescriptor, CapturedFrame, LogicalPoint, LogicalSize,
        MonitorGeometry, PhysicalPoint, PhysicalSize,
    };

    fn frame(session_id: &str, width: u32, height: u32) -> CapturedFrame {
        CapturedFrame::new(
            session_id,
            MonitorGeometry {
                id: "main".to_string(),
                physical_origin: PhysicalPoint { x: 0, y: 0 },
                physical_size: PhysicalSize { width, height },
                logical_origin: LogicalPoint { x: 0.0, y: 0.0 },
                logical_size: LogicalSize {
                    width: f64::from(width),
                    height: f64::from(height),
                },
                scale_factor: 1.0,
            },
            width,
            height,
            width * 4,
            vec![0x5a; width as usize * height as usize * 4],
        )
        .expect("fixture frame")
    }

    fn png_header(width: u32, height: u32) -> Vec<u8> {
        super::png_fixture(width, height)
    }

    #[test]
    fn frame_is_one_shot_and_wrong_sessions_fail_closed() {
        let mut buffers = SessionBuffers::new("session-a").expect("session buffers");
        buffers
            .store_frame(frame("session-a", 2, 2))
            .expect("store frame");

        assert_eq!(
            buffers
                .take_frame("session-b")
                .expect_err("wrong session")
                .code,
            CaptureErrorCode::InvalidSession
        );
        let (descriptor, bytes) = buffers.take_frame("session-a").expect("take once");
        assert_eq!(descriptor.byte_len, 16);
        assert_eq!(bytes, vec![0x5a; 16]);
        assert_eq!(
            buffers
                .take_frame("session-a")
                .expect_err("second take")
                .code,
            CaptureErrorCode::InvalidFrame
        );
    }

    #[test]
    fn result_is_retryable_until_explicit_acknowledgement() {
        let mut buffers = SessionBuffers::new("session-a").expect("session buffers");
        let png = png_header(1, 1);
        let descriptor = CaptureResultDescriptor {
            session_id: "session-a".to_string(),
            result_id: "result-a".to_string(),
            width: 1,
            height: 1,
            filename: "screenshot.png".to_string(),
            mime_type: "image/png".to_string(),
            byte_len: png.len(),
        };
        buffers
            .store_result(descriptor.clone(), png.clone())
            .expect("store result");

        assert_eq!(
            buffers
                .read_result("session-a", "wrong", "attempt-1")
                .expect_err("wrong result id")
                .code,
            CaptureErrorCode::InvalidSession
        );
        assert_eq!(
            buffers
                .read_result("session-a", "result-a", "attempt-1")
                .expect("read result"),
            (descriptor, png)
        );
        assert_eq!(
            buffers
                .read_result("session-a", "result-a", "attempt-2")
                .expect_err("concurrent result read must fail")
                .code,
            CaptureErrorCode::Busy
        );
        buffers
            .release_result_read("session-a", "result-a", "attempt-1")
            .expect("failed delivery releases its lease");
        assert!(
            buffers
                .read_result("session-a", "result-a", "attempt-2")
                .is_ok()
        );
        assert_eq!(
            buffers
                .ack_result("session-a", "wrong", "attempt-2")
                .expect_err("wrong result cannot be acknowledged")
                .code,
            CaptureErrorCode::InvalidSession
        );
        buffers
            .ack_result("session-a", "result-a", "attempt-2")
            .expect("acknowledge result");
        assert!(!buffers.has_result());
        assert!(
            buffers
                .read_result("session-a", "result-a", "attempt-3")
                .is_err()
        );
    }

    #[test]
    fn result_contract_rejects_mismatched_length_and_non_png_mime() {
        let mut buffers = SessionBuffers::new("session-a").expect("session buffers");
        let png = png_header(1, 1);
        let mut descriptor = CaptureResultDescriptor {
            session_id: "session-a".to_string(),
            result_id: "result-a".to_string(),
            width: 1,
            height: 1,
            filename: "screenshot.png".to_string(),
            mime_type: "image/png".to_string(),
            byte_len: png.len() + 1,
        };
        assert_eq!(
            buffers
                .store_result(descriptor.clone(), png.clone())
                .expect_err("wrong byte length")
                .code,
            CaptureErrorCode::InvalidFrame
        );

        descriptor.byte_len = png.len();
        descriptor.mime_type = "text/plain".to_string();
        assert_eq!(
            buffers
                .store_result(descriptor, png)
                .expect_err("wrong MIME")
                .code,
            CaptureErrorCode::InvalidFrame
        );

        let invalid_header = vec![0; 24];
        let descriptor = CaptureResultDescriptor {
            session_id: "session-a".to_string(),
            result_id: "result-a".to_string(),
            width: 1,
            height: 1,
            filename: "screenshot.png".to_string(),
            mime_type: "image/png".to_string(),
            byte_len: invalid_header.len(),
        };
        assert_eq!(
            buffers
                .store_result(descriptor, invalid_header)
                .expect_err("invalid PNG signature")
                .code,
            CaptureErrorCode::InvalidFrame
        );

        let mut truncated_png = png_header(1, 1);
        truncated_png.truncate(24);
        let descriptor = CaptureResultDescriptor {
            session_id: "session-a".to_string(),
            result_id: "result-a".to_string(),
            width: 1,
            height: 1,
            filename: "screenshot.png".to_string(),
            mime_type: "image/png".to_string(),
            byte_len: truncated_png.len(),
        };
        assert_eq!(
            buffers
                .store_result(descriptor, truncated_png)
                .expect_err("a matching IHDR without a decodable PNG body must be rejected")
                .code,
            CaptureErrorCode::InvalidFrame
        );
    }

    #[test]
    fn clearing_drops_every_sensitive_buffer() {
        let mut buffers = SessionBuffers::new("session-a").expect("session buffers");
        buffers
            .store_frame(frame("session-a", 2, 2))
            .expect("store frame");
        buffers.clear();
        assert!(!buffers.has_frame());
        assert!(!buffers.has_result());
    }

    #[test]
    fn four_k_frame_round_trip_preserves_binary_checksum() {
        let mut buffers = SessionBuffers::new("session-4k").expect("session buffers");
        let source = frame("session-4k", 3840, 2160);
        let expected = source
            .bytes()
            .iter()
            .fold(0_u64, |sum, byte| sum.wrapping_add(u64::from(*byte)));
        buffers.store_frame(source).expect("store 4K frame");
        let (_, bytes) = buffers.take_frame("session-4k").expect("take 4K frame");
        let actual = bytes
            .iter()
            .fold(0_u64, |sum, byte| sum.wrapping_add(u64::from(*byte)));
        assert_eq!(actual, expected);
        assert_eq!(bytes.len(), 3840 * 2160 * 4);
    }
}

#[cfg(test)]
mod ipc_tests {
    use tauri::ipc::{InvokeBody, InvokeResponseBody, IpcResponse};

    use super::contract::CaptureErrorCode;
    use super::ipc::{raw_response, require_raw_body};

    fn fixture(width: usize, height: usize) -> Vec<u8> {
        (0..width * height * 4)
            .map(|index| ((index * 31 + 17) % 251) as u8)
            .collect()
    }

    fn assert_raw_round_trip(width: usize, height: usize) {
        let source = fixture(width, height);
        let expected_checksum = source
            .iter()
            .fold(0_u64, |sum, byte| sum.wrapping_add(u64::from(*byte)));
        let response = raw_response(source);
        let InvokeResponseBody::Raw(bytes) = response.body().expect("IPC response body") else {
            panic!("capture bytes must not be serialized as JSON");
        };
        let actual_checksum = bytes
            .iter()
            .fold(0_u64, |sum, byte| sum.wrapping_add(u64::from(*byte)));

        assert_eq!(bytes.len(), width * height * 4);
        assert_eq!(actual_checksum, expected_checksum);
    }

    #[test]
    fn raw_ipc_response_preserves_1080p_and_4k_fixtures() {
        assert_raw_round_trip(1920, 1080);
        assert_raw_round_trip(3840, 2160);
    }

    #[test]
    fn raw_ipc_request_accepts_bytes_and_rejects_json() {
        let raw = InvokeBody::Raw(vec![1, 2, 3, 4]);
        assert_eq!(require_raw_body(&raw).expect("raw body"), &[1, 2, 3, 4]);

        let json = InvokeBody::Json(serde_json::json!({ "bytes": [1, 2, 3, 4] }));
        assert_eq!(
            require_raw_body(&json)
                .expect_err("JSON byte payloads must fail")
                .code,
            CaptureErrorCode::InvalidFrame
        );
    }
}

#[cfg(test)]
mod session_tests {
    use super::contract::CaptureErrorCode;
    use super::session::CaptureSessionGuard;

    #[test]
    fn repeated_or_concurrent_sessions_are_rejected_until_completion() {
        let mut guard = CaptureSessionGuard::default();
        guard.start("session-a").expect("first session starts");

        assert_eq!(
            guard
                .start("session-a")
                .expect_err("repeated start must fail")
                .code,
            CaptureErrorCode::Busy
        );
        assert_eq!(
            guard
                .start("session-b")
                .expect_err("concurrent start must fail")
                .code,
            CaptureErrorCode::Busy
        );

        guard.finish("session-a").expect("active session finishes");
        guard.start("session-b").expect("next session starts");
    }

    #[test]
    fn stale_session_completion_fails_closed_without_clearing_the_active_one() {
        let mut guard = CaptureSessionGuard::default();
        guard.start("session-a").expect("session starts");

        assert_eq!(
            guard
                .finish("stale")
                .expect_err("stale completion must fail")
                .code,
            CaptureErrorCode::InvalidSession
        );
        assert_eq!(guard.active_session_id(), Some("session-a"));
    }

    #[test]
    fn empty_session_ids_are_never_accepted() {
        let mut guard = CaptureSessionGuard::default();
        assert_eq!(
            guard.start(" ").expect_err("empty id must fail").code,
            CaptureErrorCode::InvalidSession
        );
        assert!(guard.active_session_id().is_none());
    }
}

#[cfg(test)]
mod platform_tests {
    use super::contract::{
        CaptureError, CaptureErrorCode, LogicalPoint, LogicalSize, MonitorGeometry, PhysicalPoint,
        PhysicalRect, PhysicalSize,
    };
    use super::platform::{
        GeometryConvention, bottom_left_visible_frame_to_top_left_area, find_matching_candidate,
        permission_preflight_result, select_logical_monitor,
        top_left_physical_work_area_to_local_css, wayland_cursor_is_unavailable,
        xcap_bounds_to_physical,
    };

    #[test]
    fn windows_xcap_bounds_are_already_physical() {
        assert_eq!(
            xcap_bounds_to_physical(-1920, 0, 1920, 1080, 1.5, GeometryConvention::Physical)
                .expect("physical bounds"),
            PhysicalRect {
                origin: PhysicalPoint { x: -1920, y: 0 },
                size: PhysicalSize {
                    width: 1920,
                    height: 1080,
                },
            }
        );
    }

    #[test]
    fn macos_and_x11_logical_bounds_are_scaled_to_capture_pixels() {
        assert_eq!(
            xcap_bounds_to_physical(-1280, 0, 1280, 720, 2.0, GeometryConvention::Logical)
                .expect("logical bounds"),
            PhysicalRect {
                origin: PhysicalPoint { x: -2560, y: 0 },
                size: PhysicalSize {
                    width: 2560,
                    height: 1440,
                },
            }
        );
    }

    #[test]
    fn macos_visible_frame_reserves_the_menu_bar_and_dock() {
        assert_eq!(
            bottom_left_visible_frame_to_top_left_area(
                LogicalPoint { x: 0.0, y: 0.0 },
                LogicalSize {
                    width: 1710.0,
                    height: 1107.0,
                },
                LogicalPoint { x: 0.0, y: 76.0 },
                LogicalSize {
                    width: 1710.0,
                    height: 997.0,
                },
            )
            .expect("valid AppKit visible frame"),
            super::contract::CssRect {
                origin: super::contract::CssPoint { x: 0.0, y: 34.0 },
                size: super::contract::CssSize {
                    width: 1710.0,
                    height: 997.0,
                },
            }
        );
    }

    #[test]
    fn macos_visible_frame_handles_a_left_side_dock() {
        assert_eq!(
            bottom_left_visible_frame_to_top_left_area(
                LogicalPoint { x: -1000.0, y: 40.0 },
                LogicalSize {
                    width: 1000.0,
                    height: 800.0,
                },
                LogicalPoint { x: -920.0, y: 70.0 },
                LogicalSize {
                    width: 920.0,
                    height: 750.0,
                },
            )
            .expect("valid offset AppKit visible frame"),
            super::contract::CssRect {
                origin: super::contract::CssPoint { x: 80.0, y: 20.0 },
                size: super::contract::CssSize {
                    width: 920.0,
                    height: 750.0,
                },
            }
        );
    }

    #[test]
    fn macos_visible_frame_rejects_geometry_outside_the_display() {
        assert!(
            bottom_left_visible_frame_to_top_left_area(
                LogicalPoint { x: 0.0, y: 0.0 },
                LogicalSize {
                    width: 100.0,
                    height: 80.0,
                },
                LogicalPoint { x: -1.0, y: 0.0 },
                LogicalSize {
                    width: 100.0,
                    height: 80.0,
                },
            )
            .is_err()
        );
    }

    #[test]
    fn windows_work_area_converts_physical_taskbar_insets_to_css_pixels() {
        assert_eq!(
            top_left_physical_work_area_to_local_css(
                PhysicalPoint { x: -1920, y: 0 },
                PhysicalSize {
                    width: 1920,
                    height: 1080,
                },
                PhysicalPoint { x: -1872, y: 0 },
                PhysicalSize {
                    width: 1872,
                    height: 1032,
                },
                1.5,
            )
            .expect("valid Win32 monitor work area"),
            super::contract::CssRect {
                origin: super::contract::CssPoint { x: 32.0, y: 0.0 },
                size: super::contract::CssSize {
                    width: 1248.0,
                    height: 688.0,
                },
            }
        );
    }

    #[test]
    fn linux_work_area_converts_gdk_panel_insets_to_css_pixels() {
        assert_eq!(
            top_left_physical_work_area_to_local_css(
                PhysicalPoint { x: 2560, y: -1440 },
                PhysicalSize {
                    width: 2560,
                    height: 1440,
                },
                PhysicalPoint { x: 2560, y: -1392 },
                PhysicalSize {
                    width: 2560,
                    height: 1392,
                },
                2.0,
            )
            .expect("valid GDK monitor work area"),
            super::contract::CssRect {
                origin: super::contract::CssPoint { x: 0.0, y: 24.0 },
                size: super::contract::CssSize {
                    width: 1280.0,
                    height: 696.0,
                },
            }
        );
    }

    #[test]
    fn physical_work_area_rejects_invalid_scale_and_outside_geometry() {
        let frame_origin = PhysicalPoint { x: 0, y: 0 };
        let frame_size = PhysicalSize {
            width: 1920,
            height: 1080,
        };
        assert!(
            top_left_physical_work_area_to_local_css(
                frame_origin,
                frame_size,
                frame_origin,
                frame_size,
                0.0,
            )
            .is_err()
        );
        assert!(
            top_left_physical_work_area_to_local_css(
                frame_origin,
                frame_size,
                PhysicalPoint { x: -1, y: 0 },
                frame_size,
                1.0,
            )
            .is_err()
        );
    }

    #[test]
    fn invalid_scale_and_coordinate_overflow_fail_closed() {
        assert!(xcap_bounds_to_physical(0, 0, 1, 1, 0.0, GeometryConvention::Logical).is_err());
        assert!(
            xcap_bounds_to_physical(i32::MAX, 0, 1, 1, 2.0, GeometryConvention::Logical).is_err()
        );
    }

    #[test]
    fn macos_mixed_dpi_selection_uses_one_native_logical_space() {
        let monitors = [
            MonitorGeometry {
                id: "retina".to_string(),
                physical_origin: PhysicalPoint { x: 0, y: 0 },
                physical_size: PhysicalSize {
                    width: 2880,
                    height: 1800,
                },
                logical_origin: LogicalPoint { x: 0.0, y: 0.0 },
                logical_size: LogicalSize {
                    width: 1440.0,
                    height: 900.0,
                },
                scale_factor: 2.0,
            },
            MonitorGeometry {
                id: "external".to_string(),
                physical_origin: PhysicalPoint { x: 1440, y: 0 },
                physical_size: PhysicalSize {
                    width: 1920,
                    height: 1080,
                },
                logical_origin: LogicalPoint { x: 1440.0, y: 0.0 },
                logical_size: LogicalSize {
                    width: 1920.0,
                    height: 1080.0,
                },
                scale_factor: 1.0,
            },
        ];

        assert_eq!(select_logical_monitor(&monitors, 1500.0, 100.0), Some(1));
    }

    #[test]
    fn wayland_cursor_selection_fails_closed_instead_of_using_zero_zero() {
        assert!(wayland_cursor_is_unavailable(Some("wayland"), false));
        assert!(wayland_cursor_is_unavailable(None, true));
        assert!(!wayland_cursor_is_unavailable(Some("x11"), false));
    }

    #[test]
    fn permission_denial_has_a_stable_machine_code() {
        permission_preflight_result(true).expect("preflight grant");
        assert_eq!(
            permission_preflight_result(false)
                .expect_err("denied preflight")
                .code,
            CaptureErrorCode::PermissionDenied
        );
    }

    #[test]
    fn stale_xcap_candidate_does_not_hide_a_later_matching_monitor() {
        let selected = PhysicalRect {
            origin: PhysicalPoint { x: 1920, y: 0 },
            size: PhysicalSize {
                width: 2560,
                height: 1440,
            },
        };
        let candidates = [
            Err(CaptureError::new(
                CaptureErrorCode::InvalidMonitor,
                "stale output",
            )),
            Ok((
                PhysicalRect {
                    origin: PhysicalPoint { x: 0, y: 0 },
                    size: PhysicalSize {
                        width: 1920,
                        height: 1080,
                    },
                },
                "other",
            )),
            Ok((selected, "selected")),
        ];

        assert_eq!(
            find_matching_candidate(candidates, selected).expect("matching healthy output"),
            "selected"
        );
    }
}
