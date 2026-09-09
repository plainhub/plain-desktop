//! Explicit Wayland capture through the XDG ScreenCast portal.
//!
//! Portal and PipeWire objects intentionally stay in this module. The capture
//! coordinator receives only one tightly packed RGBA frame and an unambiguous
//! monitor match; it never needs to know about compositor stream metadata.

use std::{
    cell::RefCell,
    os::fd::OwnedFd,
    rc::Rc,
    time::{Duration, Instant},
};

use ashpd::desktop::{
    PersistMode, ResponseError, Session,
    screencast::{
        CursorMode, Screencast, SelectSourcesOptions, SourceType, StartCastOptions,
        Stream as ScreencastStream,
    },
};
use pipewire as pw;
use pw::{properties::properties, spa};
use tauri::{AppHandle, Runtime};

use super::{
    backend::NativeFrame,
    contract::{
        CaptureError, CaptureErrorCode, LogicalPoint, LogicalSize, MAX_RAW_FRAME_BYTES,
        MonitorGeometry, PhysicalPoint, PhysicalSize,
    },
};

const PIPEWIRE_POLL_INTERVAL: Duration = Duration::from_millis(50);
const PORTAL_CLOSE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortalPixelFormat {
    Rgba,
    Rgbx,
    Bgra,
    Bgrx,
}

#[derive(Debug, Clone, Copy)]
pub struct MappedFrame<'a> {
    pub format: PortalPixelFormat,
    pub width: u32,
    pub height: u32,
    pub stride: i32,
    pub chunk_offset: usize,
    pub chunk_size: usize,
    pub corrupted: bool,
    pub bytes: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortalStreamMetadata {
    pub position: Option<(i32, i32)>,
    pub size: Option<(i32, i32)>,
}

#[derive(Debug, Clone, PartialEq)]
struct TauriMonitorSnapshot {
    name: Option<String>,
    physical_origin: (i32, i32),
    physical_size: (u32, u32),
    scale_factor: f64,
}

#[derive(Debug, Clone, Copy)]
struct NegotiatedFormat {
    format: PortalPixelFormat,
    width: u32,
    height: u32,
}

struct PipeWireUserData {
    format: Option<NegotiatedFormat>,
}

/// Enumerate monitor candidates through Tauri/winit only. In particular this
/// path must never instantiate xcap, whose Linux enumeration requires XCB and
/// is unavailable in a genuinely pure-Wayland process.
pub fn available_monitor_geometries<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Vec<MonitorGeometry>, CaptureError> {
    let snapshots = app
        .available_monitors()
        .map_err(|_| {
            CaptureError::new(
                CaptureErrorCode::NoMonitor,
                "could not enumerate monitors for the desktop portal",
            )
        })?
        .into_iter()
        .map(|monitor| TauriMonitorSnapshot {
            name: monitor.name().cloned(),
            physical_origin: (monitor.position().x, monitor.position().y),
            physical_size: (monitor.size().width, monitor.size().height),
            scale_factor: monitor.scale_factor(),
        })
        .collect();
    monitor_geometries_from_snapshots(snapshots)
}

fn monitor_geometries_from_snapshots(
    snapshots: Vec<TauriMonitorSnapshot>,
) -> Result<Vec<MonitorGeometry>, CaptureError> {
    if snapshots.is_empty() {
        return Err(CaptureError::new(
            CaptureErrorCode::NoMonitor,
            "no active monitor is available for the desktop portal",
        ));
    }

    snapshots
        .into_iter()
        .enumerate()
        .map(|(index, snapshot)| {
            if !snapshot.scale_factor.is_finite()
                || snapshot.scale_factor <= 0.0
                || snapshot.physical_size.0 == 0
                || snapshot.physical_size.1 == 0
            {
                return Err(CaptureError::new(
                    CaptureErrorCode::InvalidFrame,
                    "the desktop portal monitor geometry is invalid",
                ));
            }
            let name = snapshot.name.as_deref().unwrap_or("unnamed");
            let geometry = MonitorGeometry {
                id: format!(
                    "wayland-winit:{index}:{name}:{}:{}:{}:{}",
                    snapshot.physical_origin.0,
                    snapshot.physical_origin.1,
                    snapshot.physical_size.0,
                    snapshot.physical_size.1,
                ),
                physical_origin: PhysicalPoint {
                    x: snapshot.physical_origin.0,
                    y: snapshot.physical_origin.1,
                },
                physical_size: PhysicalSize {
                    width: snapshot.physical_size.0,
                    height: snapshot.physical_size.1,
                },
                logical_origin: LogicalPoint {
                    x: f64::from(snapshot.physical_origin.0) / snapshot.scale_factor,
                    y: f64::from(snapshot.physical_origin.1) / snapshot.scale_factor,
                },
                logical_size: LogicalSize {
                    width: f64::from(snapshot.physical_size.0) / snapshot.scale_factor,
                    height: f64::from(snapshot.physical_size.1) / snapshot.scale_factor,
                },
                scale_factor: snapshot.scale_factor,
            };
            geometry.validate()?;
            Ok(geometry)
        })
        .collect()
}

/// Convert a mapped SPA buffer into the capture subsystem's single canonical
/// pixel layout. Chunk bounds are validated independently of the backing map;
/// padding never crosses into the returned tight RGBA buffer.
pub fn decode_mapped_frame(frame: MappedFrame<'_>) -> Result<NativeFrame, CaptureError> {
    if frame.corrupted || frame.width == 0 || frame.height == 0 || frame.stride == 0 {
        return Err(invalid_pipewire_frame());
    }

    let row_bytes_u64 = u64::from(frame.width)
        .checked_mul(4)
        .ok_or_else(frame_too_large)?;
    let output_len_u64 = row_bytes_u64
        .checked_mul(u64::from(frame.height))
        .ok_or_else(frame_too_large)?;
    if output_len_u64 > MAX_RAW_FRAME_BYTES as u64 {
        return Err(frame_too_large());
    }
    let row_bytes = usize::try_from(row_bytes_u64).map_err(|_| frame_too_large())?;
    let output_len = usize::try_from(output_len_u64).map_err(|_| frame_too_large())?;
    let stride = usize::try_from(frame.stride.unsigned_abs()).map_err(|_| frame_too_large())?;
    if stride < row_bytes {
        return Err(invalid_pipewire_frame());
    }

    let last_row_offset = stride
        .checked_mul(usize::try_from(frame.height - 1).map_err(|_| frame_too_large())?)
        .ok_or_else(frame_too_large)?;
    let required_chunk_size = last_row_offset
        .checked_add(row_bytes)
        .ok_or_else(frame_too_large)?;
    if required_chunk_size > frame.chunk_size {
        return Err(invalid_pipewire_frame());
    }
    let chunk_end = frame
        .chunk_offset
        .checked_add(frame.chunk_size)
        .ok_or_else(invalid_pipewire_frame)?;
    if chunk_end > frame.bytes.len() {
        return Err(invalid_pipewire_frame());
    }

    let mut output = Vec::new();
    output
        .try_reserve_exact(output_len)
        .map_err(|_| frame_too_large())?;
    let height = usize::try_from(frame.height).map_err(|_| frame_too_large())?;
    for output_row in 0..height {
        let storage_row = if frame.stride < 0 {
            height - 1 - output_row
        } else {
            output_row
        };
        let row_start = frame
            .chunk_offset
            .checked_add(
                storage_row
                    .checked_mul(stride)
                    .ok_or_else(frame_too_large)?,
            )
            .ok_or_else(invalid_pipewire_frame)?;
        let row_end = row_start
            .checked_add(row_bytes)
            .ok_or_else(invalid_pipewire_frame)?;
        if row_end > chunk_end {
            return Err(invalid_pipewire_frame());
        }
        for pixel in frame.bytes[row_start..row_end].chunks_exact(4) {
            match frame.format {
                PortalPixelFormat::Rgba => output.extend_from_slice(pixel),
                PortalPixelFormat::Rgbx => {
                    output.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255])
                }
                PortalPixelFormat::Bgra => {
                    output.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]])
                }
                PortalPixelFormat::Bgrx => {
                    output.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255])
                }
            }
        }
    }
    if output.len() != output_len {
        return Err(invalid_pipewire_frame());
    }

    Ok(NativeFrame {
        width: frame.width,
        height: frame.height,
        stride: u32::try_from(row_bytes).map_err(|_| frame_too_large())?,
        bytes: output,
    })
}

/// Match portal compositor coordinates only when they identify one monitor.
/// Older portals may omit both fields; that fallback is accepted solely when
/// the decoded physical frame size has exactly one local monitor candidate.
pub fn match_portal_monitor(
    monitors: &[MonitorGeometry],
    metadata: PortalStreamMetadata,
    frame_size: (u32, u32),
) -> Result<MonitorGeometry, CaptureError> {
    if monitors.is_empty() {
        return Err(CaptureError::new(
            CaptureErrorCode::NoMonitor,
            "no active monitor is available for the portal capture",
        ));
    }

    let candidates: Vec<&MonitorGeometry> = match (metadata.position, metadata.size) {
        (Some(position), Some(size)) if size.0 > 0 && size.1 > 0 => monitors
            .iter()
            .filter(|monitor| {
                portal_coordinate_matches(monitor.logical_origin.x, position.0)
                    && portal_coordinate_matches(monitor.logical_origin.y, position.1)
                    && portal_coordinate_matches(monitor.logical_size.width, size.0)
                    && portal_coordinate_matches(monitor.logical_size.height, size.1)
                    && monitor.physical_size.width == frame_size.0
                    && monitor.physical_size.height == frame_size.1
            })
            .collect(),
        (None, None) => monitors
            .iter()
            .filter(|monitor| {
                monitor.physical_size.width == frame_size.0
                    && monitor.physical_size.height == frame_size.1
            })
            .collect(),
        _ => Vec::new(),
    };

    if candidates.len() != 1 {
        return Err(CaptureError::new(
            CaptureErrorCode::MonitorSelectionUnavailable,
            "the portal stream could not be matched to exactly one active monitor",
        ));
    }
    candidates[0].validate()?;
    Ok(candidates[0].clone())
}

/// Ask the desktop portal for exactly one monitor, read exactly one mapped
/// PipeWire frame, close the session, and return a bounded tight RGBA image.
/// Dropping this future at any await point after session creation schedules a
/// best-effort session close through `PortalSessionGuard::drop`.
pub async fn capture_monitor_via_portal(
    monitors: Vec<MonitorGeometry>,
    interaction_timeout: Duration,
    frame_timeout: Duration,
) -> Result<(MonitorGeometry, NativeFrame), CaptureError> {
    if interaction_timeout.is_zero() || frame_timeout.is_zero() {
        return Err(capture_timeout());
    }

    let proxy = timeout_portal(interaction_timeout, Screencast::new()).await?;
    let session = timeout_portal(
        interaction_timeout,
        proxy.create_session(Default::default()),
    )
    .await?;
    let mut session = PortalSessionGuard::new(session);

    let capture = tokio::time::timeout(
        interaction_timeout,
        capture_from_portal_session(&proxy, session.session(), monitors, frame_timeout),
    )
    .await
    .map_err(|_| capture_timeout())?;

    let close = tokio::time::timeout(PORTAL_CLOSE_TIMEOUT, session.close())
        .await
        .map_err(|_| capture_timeout())?;
    close?;
    capture
}

async fn capture_from_portal_session(
    proxy: &Screencast,
    session: &Session<Screencast>,
    monitors: Vec<MonitorGeometry>,
    frame_timeout: Duration,
) -> Result<(MonitorGeometry, NativeFrame), CaptureError> {
    proxy
        .select_sources(
            session,
            SelectSourcesOptions::default()
                .set_cursor_mode(CursorMode::Hidden)
                .set_sources(Some(SourceType::Monitor.into()))
                .set_multiple(false)
                .set_restore_token(None)
                .set_persist_mode(PersistMode::DoNot),
        )
        .await
        .map_err(portal_capture_error)?;

    let response = proxy
        .start(session, None, StartCastOptions::default())
        .await
        .map_err(portal_capture_error)?
        .response()
        .map_err(portal_capture_error)?;
    if response.streams().len() != 1 {
        return Err(CaptureError::new(
            CaptureErrorCode::MonitorSelectionUnavailable,
            "the portal did not return exactly one monitor stream",
        ));
    }
    let stream = response.streams()[0].clone();
    if stream
        .source_type()
        .is_some_and(|source| source != SourceType::Monitor)
    {
        return Err(CaptureError::new(
            CaptureErrorCode::MonitorSelectionUnavailable,
            "the portal returned a non-monitor capture source",
        ));
    }
    let metadata = portal_stream_metadata(&stream);
    let node_id = stream.pipe_wire_node_id();
    let remote = proxy
        .open_pipe_wire_remote(session, Default::default())
        .await
        .map_err(portal_capture_error)?;
    let frame = tauri::async_runtime::spawn_blocking(move || {
        super::backend::with_native_acquisition_lease(|| {
            acquire_one_pipewire_frame(node_id, remote, frame_timeout)
        })
    })
    .await
    .map_err(|_| pipewire_capture_error())??;
    let monitor = match_portal_monitor(&monitors, metadata, (frame.width, frame.height))?;
    Ok((monitor, frame))
}

fn portal_stream_metadata(stream: &ScreencastStream) -> PortalStreamMetadata {
    PortalStreamMetadata {
        position: stream.position(),
        size: stream.size(),
    }
}

fn acquire_one_pipewire_frame(
    node_id: u32,
    remote: OwnedFd,
    timeout: Duration,
) -> Result<NativeFrame, CaptureError> {
    let mainloop = pw::main_loop::MainLoopBox::new(None).map_err(|_| pipewire_capture_error())?;
    let context = pw::context::ContextBox::new(mainloop.loop_(), None)
        .map_err(|_| pipewire_capture_error())?;
    let core = context
        .connect_fd(remote, None)
        .map_err(|_| pipewire_capture_error())?;
    let stream = pw::stream::StreamBox::new(
        &core,
        "plain-screen-capture",
        properties! {
          *pw::keys::MEDIA_TYPE => "Video",
          *pw::keys::MEDIA_CATEGORY => "Capture",
          *pw::keys::MEDIA_ROLE => "Screen",
        },
    )
    .map_err(|_| pipewire_capture_error())?;

    type SharedResult = Rc<RefCell<Option<Result<NativeFrame, CaptureError>>>>;
    let result: SharedResult = Rc::new(RefCell::new(None));
    let state_result = result.clone();
    let process_result = result.clone();
    let format_result = result.clone();
    let _listener = stream
        .add_local_listener_with_user_data(PipeWireUserData { format: None })
        .state_changed(move |_, _, _, state| {
            if matches!(state, pw::stream::StreamState::Error(_)) {
                store_first_result(&state_result, Err(pipewire_capture_error()));
            }
        })
        .param_changed(move |stream, user_data, id, parameter| {
            if id != spa::param::ParamType::Format.as_raw() {
                return;
            }
            let Some(parameter) = parameter else {
                user_data.format = None;
                return;
            };
            let configured = parse_negotiated_format(parameter).and_then(|format| {
                let bytes = pipewire_buffer_parameters(format)?;
                let parameter =
                    spa::pod::Pod::from_bytes(&bytes).ok_or_else(pipewire_capture_error)?;
                let mut parameters = [parameter];
                stream
                    .update_params(&mut parameters)
                    .map_err(|_| pipewire_capture_error())?;
                Ok(format)
            });
            match configured {
                Ok(format) => user_data.format = Some(format),
                Err(error) => store_first_result(&format_result, Err(error)),
            }
        })
        .process(move |stream, user_data| {
            if process_result.borrow().is_some() {
                return;
            }
            let Some(format) = user_data.format else {
                return;
            };
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let datas = buffer.datas_mut();
            if datas.len() != 1 {
                store_first_result(&process_result, Err(invalid_pipewire_frame()));
                return;
            }
            let data = &mut datas[0];
            if !cpu_mappable_pipewire_data(
                data.type_(),
                data.flags(),
                !data.as_raw().data.is_null(),
            ) {
                store_first_result(&process_result, Err(pipewire_memory_error()));
                return;
            }
            let chunk = data.chunk();
            let chunk_offset = chunk.offset() as usize;
            let chunk_size = chunk.size() as usize;
            let stride = chunk.stride();
            let corrupted = chunk.flags().contains(spa::buffer::ChunkFlags::CORRUPTED);
            if chunk_size == 0 {
                return;
            }
            let Some(bytes) = data.data() else {
                store_first_result(&process_result, Err(pipewire_memory_error()));
                return;
            };
            let decoded = decode_mapped_frame(MappedFrame {
                format: format.format,
                width: format.width,
                height: format.height,
                stride,
                chunk_offset,
                chunk_size,
                corrupted,
                bytes,
            });
            store_first_result(&process_result, decoded);
        })
        .register()
        .map_err(|_| pipewire_capture_error())?;

    let pod_bytes = pipewire_format_parameters()?;
    let mut params = [spa::pod::Pod::from_bytes(&pod_bytes).ok_or_else(pipewire_capture_error)?];
    stream
        .connect(
            spa::utils::Direction::Input,
            Some(node_id),
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut params,
        )
        .map_err(|_| pipewire_capture_error())?;

    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or_else(capture_timeout)?;
    loop {
        if let Some(captured) = result.borrow_mut().take() {
            return captured;
        }
        let now = Instant::now();
        if now >= deadline {
            return Err(capture_timeout());
        }
        mainloop.loop_().iterate(pw::loop_::Timeout::Finite(
            PIPEWIRE_POLL_INTERVAL.min(deadline.saturating_duration_since(now)),
        ));
    }
}

fn parse_negotiated_format(parameter: &spa::pod::Pod) -> Result<NegotiatedFormat, CaptureError> {
    let (media_type, media_subtype) =
        spa::param::format_utils::parse_format(parameter).map_err(|_| invalid_pipewire_frame())?;
    if media_type != spa::param::format::MediaType::Video
        || media_subtype != spa::param::format::MediaSubtype::Raw
    {
        return Err(invalid_pipewire_frame());
    }
    let mut info = spa::param::video::VideoInfoRaw::new();
    info.parse(parameter)
        .map_err(|_| invalid_pipewire_frame())?;
    let format = match info.format() {
        spa::param::video::VideoFormat::RGBA => PortalPixelFormat::Rgba,
        spa::param::video::VideoFormat::RGBx => PortalPixelFormat::Rgbx,
        spa::param::video::VideoFormat::BGRA => PortalPixelFormat::Bgra,
        spa::param::video::VideoFormat::BGRx => PortalPixelFormat::Bgrx,
        _ => return Err(invalid_pipewire_frame()),
    };
    let size = info.size();
    let expected = u64::from(size.width)
        .checked_mul(u64::from(size.height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(frame_too_large)?;
    if size.width == 0 || size.height == 0 || expected > MAX_RAW_FRAME_BYTES as u64 {
        return Err(frame_too_large());
    }
    Ok(NegotiatedFormat {
        format,
        width: size.width,
        height: size.height,
    })
}

fn pipewire_format_parameters() -> Result<Vec<u8>, CaptureError> {
    let object = spa::pod::object!(
        spa::utils::SpaTypes::ObjectParamFormat,
        spa::param::ParamType::EnumFormat,
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaType,
            Id,
            spa::param::format::MediaType::Video
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaSubtype,
            Id,
            spa::param::format::MediaSubtype::Raw
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            spa::param::video::VideoFormat::BGRx,
            spa::param::video::VideoFormat::BGRx,
            spa::param::video::VideoFormat::BGRA,
            spa::param::video::VideoFormat::RGBx,
            spa::param::video::VideoFormat::RGBA
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            spa::utils::Rectangle {
                width: 1920,
                height: 1080
            },
            spa::utils::Rectangle {
                width: 1,
                height: 1
            },
            spa::utils::Rectangle {
                width: 8192,
                height: 8192
            }
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            spa::utils::Fraction { num: 60, denom: 1 },
            spa::utils::Fraction { num: 0, denom: 1 },
            spa::utils::Fraction { num: 240, denom: 1 }
        ),
    );
    spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(object),
    )
    .map(|serialized| serialized.0.into_inner())
    .map_err(|_| pipewire_capture_error())
}

fn pipewire_buffer_parameters(format: NegotiatedFormat) -> Result<Vec<u8>, CaptureError> {
    let stride = format
        .width
        .checked_mul(4)
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(frame_too_large)?;
    let size = u64::from(format.width)
        .checked_mul(u64::from(format.height))
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(frame_too_large)?;
    let data_types = (1_i32 << spa::sys::SPA_DATA_MemPtr) | (1_i32 << spa::sys::SPA_DATA_MemFd);
    let buffer_count = spa::utils::Choice(
        spa::utils::ChoiceFlags::empty(),
        spa::utils::ChoiceEnum::Range {
            default: 2,
            min: 1,
            max: 8,
        },
    );
    let object = spa::pod::object!(
        spa::utils::SpaTypes::ObjectParamBuffers,
        spa::param::ParamType::Buffers,
        spa::pod::Property::new(
            spa::sys::SPA_PARAM_BUFFERS_buffers,
            spa::pod::Value::Choice(spa::pod::ChoiceValue::Int(buffer_count)),
        ),
        spa::pod::Property::new(spa::sys::SPA_PARAM_BUFFERS_blocks, spa::pod::Value::Int(1),),
        spa::pod::Property::new(spa::sys::SPA_PARAM_BUFFERS_size, spa::pod::Value::Int(size),),
        spa::pod::Property::new(
            spa::sys::SPA_PARAM_BUFFERS_stride,
            spa::pod::Value::Int(stride),
        ),
        spa::pod::Property::new(spa::sys::SPA_PARAM_BUFFERS_align, spa::pod::Value::Int(16),),
        spa::pod::Property::new(
            spa::sys::SPA_PARAM_BUFFERS_dataType,
            spa::pod::Value::Int(data_types),
        ),
    );
    spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(object),
    )
    .map(|serialized| serialized.0.into_inner())
    .map_err(|_| pipewire_capture_error())
}

fn cpu_mappable_pipewire_data(
    data_type: spa::buffer::DataType,
    flags: spa::buffer::DataFlags,
    has_mapping: bool,
) -> bool {
    matches!(
        data_type,
        spa::buffer::DataType::MemPtr | spa::buffer::DataType::MemFd
    ) && flags.contains(spa::buffer::DataFlags::READABLE)
        && has_mapping
}

fn store_first_result(
    destination: &Rc<RefCell<Option<Result<NativeFrame, CaptureError>>>>,
    result: Result<NativeFrame, CaptureError>,
) {
    let mut destination = destination.borrow_mut();
    if destination.is_none() {
        *destination = Some(result);
    }
}

struct PortalSessionGuard {
    session: Option<Session<Screencast>>,
}

impl PortalSessionGuard {
    fn new(session: Session<Screencast>) -> Self {
        Self {
            session: Some(session),
        }
    }

    fn session(&self) -> &Session<Screencast> {
        self.session.as_ref().expect("portal session guard is open")
    }

    async fn close(&mut self) -> Result<(), CaptureError> {
        if let Some(session) = self.session.as_ref() {
            session.close().await.map_err(portal_capture_error)?;
            self.session = None;
        }
        Ok(())
    }
}

impl Drop for PortalSessionGuard {
    fn drop(&mut self) {
        let Some(session) = self.session.take() else {
            return;
        };
        tauri::async_runtime::spawn(async move {
            let _ = session.close().await;
        });
    }
}

async fn timeout_portal<T>(
    timeout: Duration,
    operation: impl std::future::Future<Output = Result<T, ashpd::Error>>,
) -> Result<T, CaptureError> {
    tokio::time::timeout(timeout, operation)
        .await
        .map_err(|_| capture_timeout())?
        .map_err(portal_capture_error)
}

fn portal_coordinate_matches(local: f64, portal: i32) -> bool {
    local.is_finite() && (local - f64::from(portal)).abs() <= 0.51
}

fn portal_capture_error(error: ashpd::Error) -> CaptureError {
    let code = if matches!(error, ashpd::Error::Response(ResponseError::Cancelled)) {
        CaptureErrorCode::PermissionDenied
    } else {
        CaptureErrorCode::CaptureFailed
    };
    CaptureError::new(code, "the desktop capture portal request failed")
}

fn invalid_pipewire_frame() -> CaptureError {
    CaptureError::new(
        CaptureErrorCode::InvalidFrame,
        "the desktop portal returned an invalid mapped video frame",
    )
}

fn frame_too_large() -> CaptureError {
    CaptureError::new(
        CaptureErrorCode::FrameTooLarge,
        "the desktop portal frame exceeds the process memory limit",
    )
}

fn pipewire_memory_error() -> CaptureError {
    CaptureError::new(
        CaptureErrorCode::CaptureFailed,
        "the desktop portal did not provide a CPU-mappable video frame",
    )
}

fn pipewire_capture_error() -> CaptureError {
    CaptureError::new(
        CaptureErrorCode::CaptureFailed,
        "the desktop portal video stream failed",
    )
}

fn capture_timeout() -> CaptureError {
    CaptureError::new(
        CaptureErrorCode::TimedOut,
        "the desktop portal capture timed out",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::screen_capture::contract::{
        CaptureErrorCode, LogicalPoint, LogicalSize, MonitorGeometry, PhysicalPoint, PhysicalSize,
    };

    fn monitor(
        id: &str,
        physical_origin: (i32, i32),
        physical_size: (u32, u32),
        logical_origin: (f64, f64),
        logical_size: (f64, f64),
        scale_factor: f64,
    ) -> MonitorGeometry {
        MonitorGeometry {
            id: id.to_string(),
            physical_origin: PhysicalPoint {
                x: physical_origin.0,
                y: physical_origin.1,
            },
            physical_size: PhysicalSize {
                width: physical_size.0,
                height: physical_size.1,
            },
            logical_origin: LogicalPoint {
                x: logical_origin.0,
                y: logical_origin.1,
            },
            logical_size: LogicalSize {
                width: logical_size.0,
                height: logical_size.1,
            },
            scale_factor,
        }
    }

    #[test]
    fn tauri_snapshot_conversion_preserves_physical_logical_and_scale_spaces() {
        let converted = monitor_geometries_from_snapshots(vec![TauriMonitorSnapshot {
            name: Some("Left panel".to_string()),
            physical_origin: (-2560, 120),
            physical_size: (2560, 1440),
            scale_factor: 1.25,
        }])
        .expect("valid Tauri monitor snapshot");

        assert_eq!(converted.len(), 1);
        let monitor = &converted[0];
        assert_eq!(monitor.id, "wayland-winit:0:Left panel:-2560:120:2560:1440");
        assert_eq!(monitor.physical_origin, PhysicalPoint { x: -2560, y: 120 });
        assert_eq!(
            monitor.physical_size,
            PhysicalSize {
                width: 2560,
                height: 1440
            }
        );
        assert_eq!(
            monitor.logical_origin,
            LogicalPoint {
                x: -2048.0,
                y: 96.0
            }
        );
        assert_eq!(
            monitor.logical_size,
            LogicalSize {
                width: 2048.0,
                height: 1152.0
            }
        );
        assert_eq!(monitor.scale_factor, 1.25);
    }

    #[test]
    fn empty_tauri_monitor_snapshot_fails_before_opening_the_portal() {
        assert_eq!(
            monitor_geometries_from_snapshots(Vec::new())
                .expect_err("empty monitor enumeration")
                .code,
            CaptureErrorCode::NoMonitor
        );
    }

    #[test]
    fn converted_ambiguous_candidates_still_fail_closed() {
        let snapshots = vec![
            TauriMonitorSnapshot {
                name: Some("same".to_string()),
                physical_origin: (0, 0),
                physical_size: (1920, 1080),
                scale_factor: 1.0,
            },
            TauriMonitorSnapshot {
                name: Some("same".to_string()),
                physical_origin: (0, 0),
                physical_size: (1920, 1080),
                scale_factor: 1.0,
            },
        ];
        let monitors = monitor_geometries_from_snapshots(snapshots).expect("valid snapshots");
        assert_ne!(
            monitors[0].id, monitors[1].id,
            "snapshot IDs remain distinct"
        );
        assert_eq!(
            match_portal_monitor(
                &monitors,
                PortalStreamMetadata {
                    position: Some((0, 0)),
                    size: Some((1920, 1080)),
                },
                (1920, 1080),
            )
            .expect_err("metadata matches more than one candidate")
            .code,
            CaptureErrorCode::MonitorSelectionUnavailable
        );
    }

    #[test]
    fn mapped_rgba_with_padding_is_tightly_repacked() {
        let bytes = [
            1, 2, 3, 4, 5, 6, 7, 8, 90, 91, 92, 93, 9, 10, 11, 12, 13, 14, 15, 16, 94, 95, 96, 97,
        ];
        let decoded = decode_mapped_frame(MappedFrame {
            format: PortalPixelFormat::Rgba,
            width: 2,
            height: 2,
            stride: 12,
            chunk_offset: 0,
            chunk_size: bytes.len(),
            corrupted: false,
            bytes: &bytes,
        })
        .expect("valid padded frame");

        assert_eq!(decoded.stride, 8);
        assert_eq!(
            decoded.bytes,
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
        );
    }

    #[test]
    fn nonzero_chunk_offset_is_applied_before_row_decoding() {
        let bytes = [90, 91, 92, 93, 1, 2, 3, 4, 5, 6, 7, 8, 94, 95];
        let decoded = decode_mapped_frame(MappedFrame {
            format: PortalPixelFormat::Rgba,
            width: 2,
            height: 1,
            stride: 8,
            chunk_offset: 4,
            chunk_size: 8,
            corrupted: false,
            bytes: &bytes,
        })
        .expect("valid offset frame");

        assert_eq!(decoded.bytes, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn oversized_dimensions_fail_before_accessing_the_map() {
        let error = decode_mapped_frame(MappedFrame {
            format: PortalPixelFormat::Rgba,
            width: 8193,
            height: 8193,
            stride: 32_772,
            chunk_offset: 0,
            chunk_size: usize::MAX,
            corrupted: false,
            bytes: &[],
        })
        .expect_err("raw frames over the allocation cap must be rejected");

        assert_eq!(error.code, CaptureErrorCode::FrameTooLarge);
    }

    #[test]
    fn bgra_and_x_formats_are_normalized_to_opaque_rgba() {
        let bgra = decode_mapped_frame(MappedFrame {
            format: PortalPixelFormat::Bgra,
            width: 1,
            height: 1,
            stride: 4,
            chunk_offset: 0,
            chunk_size: 4,
            corrupted: false,
            bytes: &[30, 20, 10, 40],
        })
        .unwrap();
        assert_eq!(bgra.bytes, vec![10, 20, 30, 40]);

        let bgrx = decode_mapped_frame(MappedFrame {
            format: PortalPixelFormat::Bgrx,
            width: 1,
            height: 1,
            stride: 4,
            chunk_offset: 0,
            chunk_size: 4,
            corrupted: false,
            bytes: &[3, 2, 1, 0],
        })
        .unwrap();
        assert_eq!(bgrx.bytes, vec![1, 2, 3, 255]);

        let rgbx = decode_mapped_frame(MappedFrame {
            format: PortalPixelFormat::Rgbx,
            width: 1,
            height: 1,
            stride: 4,
            chunk_offset: 0,
            chunk_size: 4,
            corrupted: false,
            bytes: &[1, 2, 3, 0],
        })
        .unwrap();
        assert_eq!(rgbx.bytes, vec![1, 2, 3, 255]);
    }

    #[test]
    fn negative_stride_is_decoded_bottom_up_without_underflow() {
        let bytes = [9, 10, 11, 12, 13, 14, 15, 16, 1, 2, 3, 4, 5, 6, 7, 8];
        let decoded = decode_mapped_frame(MappedFrame {
            format: PortalPixelFormat::Rgba,
            width: 2,
            height: 2,
            stride: -8,
            chunk_offset: 0,
            chunk_size: bytes.len(),
            corrupted: false,
            bytes: &bytes,
        })
        .unwrap();
        assert_eq!(
            decoded.bytes,
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
        );
    }

    #[test]
    fn malformed_chunks_fail_closed_before_copying() {
        let cases = [
            MappedFrame {
                format: PortalPixelFormat::Rgba,
                width: 2,
                height: 2,
                stride: 7,
                chunk_offset: 0,
                chunk_size: 16,
                corrupted: false,
                bytes: &[0; 16],
            },
            MappedFrame {
                format: PortalPixelFormat::Rgba,
                width: 2,
                height: 2,
                stride: 8,
                chunk_offset: 9,
                chunk_size: 16,
                corrupted: false,
                bytes: &[0; 16],
            },
            MappedFrame {
                format: PortalPixelFormat::Rgba,
                width: 2,
                height: 2,
                stride: 8,
                chunk_offset: 0,
                chunk_size: 8,
                corrupted: false,
                bytes: &[0; 16],
            },
            MappedFrame {
                format: PortalPixelFormat::Rgba,
                width: 2,
                height: 2,
                stride: 8,
                chunk_offset: 0,
                chunk_size: 16,
                corrupted: true,
                bytes: &[0; 16],
            },
        ];

        for frame in cases {
            assert_eq!(
                decode_mapped_frame(frame)
                    .expect_err("malformed frame")
                    .code,
                CaptureErrorCode::InvalidFrame
            );
        }
    }

    #[test]
    fn stream_metadata_selects_exactly_one_logical_monitor() {
        let monitors = [
            monitor(
                "left",
                (-2560, 0),
                (2560, 1440),
                (-2048.0, 0.0),
                (2048.0, 1152.0),
                1.25,
            ),
            monitor(
                "main",
                (0, 0),
                (3840, 2160),
                (0.0, 0.0),
                (1920.0, 1080.0),
                2.0,
            ),
        ];
        let selected = match_portal_monitor(
            &monitors,
            PortalStreamMetadata {
                position: Some((-2048, 0)),
                size: Some((2048, 1152)),
            },
            (2560, 1440),
        )
        .expect("unique logical monitor");
        assert_eq!(selected.id, "left");
    }

    #[test]
    fn missing_metadata_falls_back_only_to_unique_exact_frame_size() {
        let unique = [
            monitor(
                "small",
                (0, 0),
                (1920, 1080),
                (0.0, 0.0),
                (1920.0, 1080.0),
                1.0,
            ),
            monitor(
                "large",
                (1920, 0),
                (3840, 2160),
                (1920.0, 0.0),
                (1920.0, 1080.0),
                2.0,
            ),
        ];
        assert_eq!(
            match_portal_monitor(
                &unique,
                PortalStreamMetadata {
                    position: None,
                    size: None,
                },
                (3840, 2160),
            )
            .unwrap()
            .id,
            "large"
        );

        let ambiguous = [
            monitor("a", (0, 0), (1920, 1080), (0.0, 0.0), (1920.0, 1080.0), 1.0),
            monitor(
                "b",
                (1920, 0),
                (1920, 1080),
                (1920.0, 0.0),
                (1920.0, 1080.0),
                1.0,
            ),
        ];
        assert_eq!(
            match_portal_monitor(
                &ambiguous,
                PortalStreamMetadata {
                    position: None,
                    size: None,
                },
                (1920, 1080),
            )
            .expect_err("ambiguous fallback")
            .code,
            CaptureErrorCode::MonitorSelectionUnavailable
        );
    }

    #[test]
    fn partial_or_ambiguous_portal_metadata_is_rejected() {
        let monitors = [
            monitor("a", (0, 0), (1920, 1080), (0.0, 0.0), (1920.0, 1080.0), 1.0),
            monitor("b", (0, 0), (1920, 1080), (0.0, 0.0), (1920.0, 1080.0), 1.0),
        ];
        for metadata in [
            PortalStreamMetadata {
                position: Some((0, 0)),
                size: None,
            },
            PortalStreamMetadata {
                position: Some((0, 0)),
                size: Some((1920, 1080)),
            },
        ] {
            assert_eq!(
                match_portal_monitor(&monitors, metadata, (1920, 1080))
                    .expect_err("metadata must identify one monitor")
                    .code,
                CaptureErrorCode::MonitorSelectionUnavailable
            );
        }
    }

    #[test]
    fn buffer_parameters_request_only_cpu_mappable_storage() {
        let bytes = pipewire_buffer_parameters(NegotiatedFormat {
            format: PortalPixelFormat::Bgra,
            width: 1920,
            height: 1080,
        })
        .expect("buffer parameters");
        let (remaining, value) =
            spa::pod::deserialize::PodDeserializer::deserialize_any_from(&bytes)
                .expect("serialized buffer pod");
        assert!(remaining.is_empty());
        let spa::pod::Value::Object(object) = value else {
            panic!("buffer parameters must be an object");
        };
        assert_eq!(
            object.type_,
            spa::utils::SpaTypes::ObjectParamBuffers.as_raw()
        );
        assert_eq!(object.id, spa::param::ParamType::Buffers.as_raw());

        let data_types = object
            .properties
            .iter()
            .find(|property| property.key == spa::sys::SPA_PARAM_BUFFERS_dataType)
            .expect("data type property");
        assert_eq!(
            data_types.value,
            spa::pod::Value::Int(
                (1_i32 << spa::sys::SPA_DATA_MemPtr) | (1_i32 << spa::sys::SPA_DATA_MemFd)
            )
        );
        assert!(!cpu_mappable_pipewire_data(
            spa::buffer::DataType::DmaBuf,
            spa::buffer::DataFlags::READABLE,
            true,
        ));
        assert!(!cpu_mappable_pipewire_data(
            spa::buffer::DataType::MemFd,
            spa::buffer::DataFlags::empty(),
            true,
        ));
        assert!(cpu_mappable_pipewire_data(
            spa::buffer::DataType::MemPtr,
            spa::buffer::DataFlags::READABLE,
            true,
        ));
    }
}
