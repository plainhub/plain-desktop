//! Windows plain-capture backend: Windows Graphics Capture single-frame
//! screenshots (Windows 10 1903+) over native Win32 enumeration.
//!
//! One capture = one free-threaded frame pool that delivers exactly one frame
//! through a bounded channel and is then closed, so the WGC yellow border
//! never outlives the screenshot. The D3D work happens on the calling thread
//! (serialized by the capture acquisition gate); the FrameArrived handler
//! only forwards the frame object.

use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, RecvTimeoutError, sync_channel};
use std::time::Duration;

use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Win32::Foundation::{HMODULE, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Resource, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
};
use windows::Win32::System::Com::CoIncrementMTAUsage;
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
use windows::core::BOOL;
use windows::core::{IInspectable, Interface, factory};

use crate::capture::Capture;
use crate::capture::CaptureError;
use crate::capture::CaptureErrorCode;
use crate::capture::DisplayInfo;
use crate::capture::Frame;
use crate::capture::PhysicalPoint;
use crate::capture::checked_rounded_i32;
use crate::capture::select_display_at;

const FRAME_TIMEOUT: Duration = Duration::from_secs(3);
const BASE_DPI: u32 = 96;

/// Free-threaded COM apartment so WinRT activation works on any thread. The
/// usage cookie is intentionally leaked: capture can (re)start at any point.
fn ensure_mta() -> Result<(), CaptureError> {
    static MTA: OnceLock<Result<(), String>> = OnceLock::new();
    MTA.get_or_init(|| {
        unsafe { CoIncrementMTAUsage() }
            .map(|_cookie| ())
            .map_err(|error| format!("CoIncrementMTAUsage: {error}"))
    })
    .clone()
    .map_err(|error| CaptureError::new(CaptureErrorCode::CaptureFailed, error))
}

/// windows-rs derives neither Send nor Sync for COM interface pointers; the
/// D3D11 device/context here are free-threaded and every use is serialized by
/// the capture acquisition gate in the app layer.
struct SendD3d<T>(T);
unsafe impl<T> Send for SendD3d<T> {}
unsafe impl<T> Sync for SendD3d<T> {}

#[derive(Clone)]
struct MonitorEntry {
    hmonitor: HMONITOR,
    name: String,
    frame: RECT,
    dpi: u32,
}

pub struct WindowsCapture {
    device: SendD3d<ID3D11Device>,
    context: SendD3d<ID3D11DeviceContext>,
}

impl WindowsCapture {
    pub fn connect() -> Result<Self, CaptureError> {
        ensure_mta()?;
        if !GraphicsCaptureSession::IsSupported().map_err(platform_error("IsSupported"))? {
            return Err(CaptureError::new(
                CaptureErrorCode::CaptureFailed,
                "Windows Graphics Capture is not supported (needs Windows 10 1903+)",
            ));
        }
        let (device, context) = create_d3d_device()?;
        Ok(Self {
            device: SendD3d(device),
            context: SendD3d(context),
        })
    }
}

fn platform_error(stage: &str) -> impl Fn(windows::core::Error) -> CaptureError + '_ {
    move |error| {
        CaptureError::new(
            CaptureErrorCode::CaptureFailed,
            format!("failed to {stage}: {error}"),
        )
    }
}

fn create_d3d_device() -> Result<(ID3D11Device, ID3D11DeviceContext), CaptureError> {
    let mut device = None;
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )
        .map_err(platform_error("D3D11CreateDevice"))?;
    }
    let device = device.ok_or_else(|| {
        CaptureError::new(
            CaptureErrorCode::CaptureFailed,
            "D3D11CreateDevice returned no device",
        )
    })?;
    let context =
        unsafe { device.GetImmediateContext() }.map_err(platform_error("GetImmediateContext"))?;
    Ok((device, context))
}

fn utf16_to_string(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

fn enumerate_monitors() -> Result<Vec<MonitorEntry>, CaptureError> {
    let mut entries: Vec<MonitorEntry> = Vec::new();
    let lparam = LPARAM(&mut entries as *mut Vec<MonitorEntry> as isize);
    let ok = unsafe { EnumDisplayMonitors(None, None, Some(enum_monitor_proc), lparam) };
    if !ok.as_bool() {
        return Err(CaptureError::new(
            CaptureErrorCode::NoMonitor,
            "EnumDisplayMonitors failed to enumerate the desktop",
        ));
    }
    if entries.is_empty() {
        return Err(CaptureError::new(
            CaptureErrorCode::NoMonitor,
            "no active monitor is available",
        ));
    }
    Ok(entries)
}

unsafe extern "system" fn enum_monitor_proc(
    hmonitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    let entries = unsafe { &mut *(lparam.0 as *mut Vec<MonitorEntry>) };

    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = u32::try_from(std::mem::size_of::<MONITORINFOEXW>()).unwrap_or(0);
    if !unsafe { GetMonitorInfoW(hmonitor, &mut info as *mut MONITORINFOEXW as *mut _) }.as_bool() {
        return BOOL(1);
    }

    let mut dpi_x = BASE_DPI;
    let mut dpi_y = BASE_DPI;
    let _ = unsafe { GetDpiForMonitor(hmonitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };

    entries.push(MonitorEntry {
        hmonitor,
        name: utf16_to_string(&info.szDevice),
        frame: info.monitorInfo.rcMonitor,
        dpi: dpi_x,
    });
    BOOL(1)
}

fn monitor_id(index: usize, entry: &MonitorEntry) -> String {
    format!(
        "{index}:{}:{}:{}:{}:{}",
        entry.name,
        entry.frame.left,
        entry.frame.top,
        entry.frame.right - entry.frame.left,
        entry.frame.bottom - entry.frame.top,
    )
}

fn display_info(index: usize, entry: &MonitorEntry) -> Result<DisplayInfo, CaptureError> {
    let width = u32::try_from((entry.frame.right - entry.frame.left).max(0)).map_err(|_| {
        CaptureError::new(CaptureErrorCode::InvalidMonitor, "monitor width overflow")
    })?;
    let height = u32::try_from((entry.frame.bottom - entry.frame.top).max(0)).map_err(|_| {
        CaptureError::new(CaptureErrorCode::InvalidMonitor, "monitor height overflow")
    })?;
    let scale_factor = f64::from(entry.dpi) / f64::from(BASE_DPI);
    let geometry = DisplayInfo {
        id: monitor_id(index, entry),
        physical_origin: PhysicalPoint {
            x: entry.frame.left,
            y: entry.frame.top,
        },
        physical_size: crate::capture::PhysicalSize { width, height },
        logical_origin: crate::capture::LogicalPoint {
            x: f64::from(entry.frame.left) / scale_factor,
            y: f64::from(entry.frame.top) / scale_factor,
        },
        logical_size: crate::capture::LogicalSize {
            width: f64::from(width) / scale_factor,
            height: f64::from(height) / scale_factor,
        },
        scale_factor,
    };
    geometry.validate()?;
    Ok(geometry)
}

fn find_monitor_for(display: &DisplayInfo) -> Result<(MonitorEntry, DisplayInfo), CaptureError> {
    let entries = enumerate_monitors()?;
    for (index, entry) in entries.iter().enumerate() {
        let info = display_info(index, entry)?;
        if info.id == display.id {
            return Ok((entry.clone(), info));
        }
    }
    Err(CaptureError::new(
        CaptureErrorCode::NoMonitor,
        "the selected monitor disappeared before capture",
    ))
}

struct OwnedCaptureItem {
    rx: Receiver<Direct3D11CaptureFrame>,
    // Kept alive until the capture ends: dropping the session stops the
    // stream and removes the system capture border.
    _item: GraphicsCaptureItem,
    _frame_pool: Direct3D11CaptureFramePool,
    _session: GraphicsCaptureSession,
}

fn create_capture_item(
    winrt_device: &IDirect3DDevice,
    hmonitor: HMONITOR,
) -> Result<OwnedCaptureItem, CaptureError> {
    let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
        .map_err(platform_error("IGraphicsCaptureItemInterop factory"))?;
    let item: GraphicsCaptureItem = unsafe { interop.CreateForMonitor(hmonitor) }
        .map_err(platform_error("CreateForMonitor"))?;

    let size = item.Size().map_err(platform_error("item.Size"))?;
    let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        winrt_device,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        2,
        size,
    )
    .map_err(platform_error("CreateFreeThreaded"))?;

    let (tx, rx) = sync_channel::<Direct3D11CaptureFrame>(1);
    frame_pool
        .FrameArrived(
            &TypedEventHandler::<Direct3D11CaptureFramePool, IInspectable>::new(
                move |pool, _args| {
                    let Some(pool) = pool.as_ref() else {
                        return Ok(());
                    };
                    if let Ok(frame) = pool.TryGetNextFrame() {
                        let _ = tx.try_send(frame);
                    }
                    Ok(())
                },
            ),
        )
        .map_err(platform_error("FrameArrived"))?;

    let session = frame_pool
        .CreateCaptureSession(&item)
        .map_err(platform_error("CreateCaptureSession"))?;

    let _ = session.SetIsCursorCaptureEnabled(false);
    let _ = session.SetIsBorderRequired(false);
    session
        .StartCapture()
        .map_err(platform_error("StartCapture"))?;

    Ok(OwnedCaptureItem {
        rx,
        _item: item,
        _frame_pool: frame_pool,
        _session: session,
    })
}

/// Copies the frame's GPU texture into a tight RGBA host buffer via a
/// staging texture (BGRA8 on the GPU, RowPitch-aware copy, swizzle on CPU).
fn frame_texture_to_host(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    frame: &Direct3D11CaptureFrame,
) -> Result<Frame, CaptureError> {
    let surface = frame.Surface().map_err(platform_error("frame.Surface"))?;
    let access: IDirect3DDxgiInterfaceAccess = surface
        .cast()
        .map_err(platform_error("IDirect3DDxgiInterfaceAccess cast"))?;
    let texture: ID3D11Texture2D =
        unsafe { access.GetInterface() }.map_err(platform_error("GetInterface"))?;

    unsafe {
        let mut source_desc = D3D11_TEXTURE2D_DESC::default();
        texture.GetDesc(&mut source_desc);
        let width = source_desc.Width;
        let height = source_desc.Height;
        if width == 0 || height == 0 {
            return Err(CaptureError::new(
                CaptureErrorCode::InvalidFrame,
                "the captured texture is empty",
            ));
        }
        let stride = width.checked_mul(4).ok_or_else(|| {
            CaptureError::new(CaptureErrorCode::FrameTooLarge, "RGBA stride overflow")
        })?;

        let mut staging_desc = source_desc;
        staging_desc.MipLevels = 1;
        staging_desc.ArraySize = 1;
        staging_desc.SampleDesc.Count = 1;
        staging_desc.SampleDesc.Quality = 0;
        staging_desc.BindFlags = 0;
        staging_desc.MiscFlags = 0;
        staging_desc.Usage = D3D11_USAGE_STAGING;
        staging_desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;

        let mut staging = None;
        device
            .CreateTexture2D(&staging_desc, None, Some(&mut staging))
            .map_err(platform_error("CreateTexture2D(staging)"))?;
        let staging = staging.ok_or_else(|| {
            CaptureError::new(
                CaptureErrorCode::CaptureFailed,
                "CreateTexture2D returned no texture",
            )
        })?;

        let staging_resource: ID3D11Resource =
            staging.cast().map_err(platform_error("staging cast"))?;
        let source_resource: ID3D11Resource =
            texture.cast().map_err(platform_error("texture cast"))?;
        context.CopyResource(Some(&staging_resource), Some(&source_resource));

        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        context
            .Map(
                Some(&staging_resource),
                0,
                D3D11_MAP_READ,
                0,
                Some(&mut mapped),
            )
            .map_err(platform_error("Map(staging)"))?;

        let row_bytes = stride as usize;
        let mut bytes = vec![0u8; row_bytes * height as usize];
        let source = mapped.pData as *const u8;
        for (row, out_row) in bytes.chunks_exact_mut(row_bytes).enumerate() {
            let src =
                std::slice::from_raw_parts(source.add(row * mapped.RowPitch as usize), row_bytes);
            out_row.copy_from_slice(src);
        }
        context.Unmap(Some(&staging_resource), 0);

        for pixel in bytes.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }

        Ok(Frame {
            width,
            height,
            stride,
            bytes,
        })
    }
}

impl Capture for WindowsCapture {
    fn displays(&self) -> Result<Vec<DisplayInfo>, CaptureError> {
        enumerate_monitors()?
            .into_iter()
            .enumerate()
            .map(|(index, entry)| display_info(index, &entry))
            .collect()
    }

    fn display_index_at_cursor(&self, displays: &[DisplayInfo]) -> Result<usize, CaptureError> {
        let mut point = POINT::default();
        unsafe { GetCursorPos(&mut point) }.map_err(platform_error("GetCursorPos"))?;
        let physical = PhysicalPoint {
            x: checked_rounded_i32(f64::from(point.x))?,
            y: checked_rounded_i32(f64::from(point.y))?,
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
        crate::capture::preflight_capture_permission()?;
        let (entry, _) = find_monitor_for(display)?;
        let dxgi_device: IDXGIDevice = self
            .device
            .0
            .cast()
            .map_err(platform_error("IDXGIDevice cast"))?;
        let winrt_device: IDirect3DDevice =
            unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi_device) }
                .map_err(platform_error("CreateDirect3D11DeviceFromDXGIDevice"))?
                .cast()
                .map_err(platform_error("IDirect3DDevice cast"))?;

        let owned = create_capture_item(&winrt_device, entry.hmonitor)?;
        let result = (|| {
            let frame = match owned.rx.recv_timeout(FRAME_TIMEOUT) {
                Ok(frame) => frame,
                Err(RecvTimeoutError::Timeout) => {
                    return Err(CaptureError::new(
                        CaptureErrorCode::TimedOut,
                        "the Windows capture pool delivered no frame in time",
                    ));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(CaptureError::new(
                        CaptureErrorCode::CaptureFailed,
                        "the Windows capture pool was closed before a frame arrived",
                    ));
                }
            };
            let out = frame_texture_to_host(&self.device.0, &self.context.0, &frame);
            let _ = frame.Close();
            out
        })();
        drop(owned);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::MonitorEntry;
    use super::monitor_id;
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::HMONITOR;

    #[test]
    fn monitor_ids_are_stable_indexed_descriptors() {
        let entry = MonitorEntry {
            hmonitor: HMONITOR(std::ptr::null_mut()),
            name: "\\\\.\\DISPLAY1".to_string(),
            frame: RECT {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            dpi: 96,
        };
        assert_eq!(monitor_id(0, &entry), "0:\\\\.\\DISPLAY1:0:0:1920:1080");
    }
}
