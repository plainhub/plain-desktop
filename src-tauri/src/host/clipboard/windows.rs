use std::{
    ffi::c_void, io, os::windows::ffi::OsStrExt, path::Path, ptr, slice, thread, time::Duration,
};

use super::{MAX_TEXT_UNITS, dib::dib_v5};

const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;
const CF_DIBV5: u32 = 17;
const GMEM_MOVEABLE: u32 = 0x0002;

#[link(name = "user32")]
unsafe extern "system" {
    fn OpenClipboard(owner: *mut c_void) -> i32;
    fn CloseClipboard() -> i32;
    fn EmptyClipboard() -> i32;
    fn IsClipboardFormatAvailable(format: u32) -> i32;
    fn GetClipboardData(format: u32) -> *mut c_void;
    fn SetClipboardData(format: u32, memory: *mut c_void) -> *mut c_void;
    fn RegisterClipboardFormatW(name: *const u16) -> u32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalAlloc(flags: u32, bytes: usize) -> *mut c_void;
    fn GlobalLock(memory: *mut c_void) -> *mut c_void;
    fn GlobalUnlock(memory: *mut c_void) -> i32;
    fn GlobalFree(memory: *mut c_void) -> *mut c_void;
    fn GlobalSize(memory: *mut c_void) -> usize;
}

struct OpenClipboardGuard(bool);

impl Drop for OpenClipboardGuard {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: Only a successful open on this thread constructs the guard.
            unsafe { CloseClipboard() };
        }
    }
}

fn with_clipboard<T>(
    owner: *mut c_void,
    operation: impl FnOnce() -> io::Result<T>,
) -> io::Result<T> {
    for attempt in 0..=5 {
        // SAFETY: The owner is null for a read or a live HWND supplied by Tauri.
        if unsafe { OpenClipboard(owner) } != 0 {
            let mut guard = OpenClipboardGuard(true);
            let result = operation();
            // SAFETY: The clipboard is open on this thread and its operation has ended.
            let closed = unsafe { CloseClipboard() };
            guard.0 = false;
            return result.and_then(|value| {
                if closed != 0 {
                    Ok(value)
                } else {
                    Err(io::Error::last_os_error())
                }
            });
        }
        if attempt < 5 {
            thread::sleep(Duration::from_millis(5));
        }
    }
    Err(io::Error::last_os_error())
}

struct LockedMemory {
    handle: *mut c_void,
    data: *mut c_void,
}

impl LockedMemory {
    fn new(handle: *mut c_void) -> io::Result<Self> {
        // SAFETY: The caller holds a live global allocation or the opened clipboard.
        let data = unsafe { GlobalLock(handle) };
        if data.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { handle, data })
    }
}

impl Drop for LockedMemory {
    fn drop(&mut self) {
        // SAFETY: The handle was successfully locked and is still live.
        unsafe { GlobalUnlock(self.handle) };
    }
}

struct ClipboardMemory(*mut c_void);

impl ClipboardMemory {
    fn new(bytes: &[u8]) -> io::Result<Self> {
        // SAFETY: The caller supplies the exact byte length needed for a movable clipboard allocation.
        let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let memory = Self(handle);
        let locked = LockedMemory::new(handle)?;
        // SAFETY: The source and the new locked allocation are disjoint and both hold bytes.len() bytes.
        unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), locked.data.cast(), bytes.len()) };
        drop(locked);
        Ok(memory)
    }

    fn transfer(self, format: u32) -> io::Result<()> {
        // SAFETY: The caller opened and emptied the clipboard, and this movable handle is unlocked.
        if unsafe { SetClipboardData(format, self.0) }.is_null() {
            return Err(io::Error::last_os_error());
        }
        std::mem::forget(self);
        Ok(())
    }
}

impl Drop for ClipboardMemory {
    fn drop(&mut self) {
        // SAFETY: This handle remains owned here until SetClipboardData succeeds.
        unsafe { GlobalFree(self.0) };
    }
}

fn write(
    owner: *mut c_void,
    formats: impl IntoIterator<Item = (u32, ClipboardMemory)>,
) -> io::Result<()> {
    with_clipboard(owner, || {
        // SAFETY: The clipboard is open on this thread with the app window as its owner.
        if unsafe { EmptyClipboard() } == 0 {
            return Err(io::Error::last_os_error());
        }
        for (format, memory) in formats {
            memory.transfer(format)?;
        }
        Ok(())
    })
}

pub(super) fn read_text() -> io::Result<Option<String>> {
    with_clipboard(ptr::null_mut(), || {
        // SAFETY: The current thread holds the clipboard open for all clipboard and memory calls below.
        if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT) } == 0 {
            return Ok(None);
        }
        let handle = unsafe { GetClipboardData(CF_UNICODETEXT) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let length = (unsafe { GlobalSize(handle) } / size_of::<u16>()).min(MAX_TEXT_UNITS + 1);
        let locked = LockedMemory::new(handle)?;
        // SAFETY: GlobalLock returns aligned memory, and the slice stays within the allocation and its lock.
        let units = unsafe { slice::from_raw_parts(locked.data.cast::<u16>(), length) };
        Ok(units
            .iter()
            .position(|&unit| unit == 0)
            .map(|end| String::from_utf16_lossy(&units[..end])))
    })
}

pub(super) fn write_text(owner: *mut c_void, text: &str) -> io::Result<()> {
    let bytes: Vec<u8> = text
        .encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect();
    write(owner, [(CF_UNICODETEXT, ClipboardMemory::new(&bytes)?)])
}

pub(super) fn write_image(
    owner: *mut c_void,
    png: &[u8],
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Result<(), String> {
    // SAFETY: This is a NUL-terminated UTF-16 format name.
    let png_format = unsafe { RegisterClipboardFormatW([80u16, 78, 71, 0].as_ptr()) };
    if png_format == 0 {
        return Err(io::Error::last_os_error().to_string());
    }
    let dib = dib_v5(width, height, rgba)?;
    let png = ClipboardMemory::new(png).map_err(|error| error.to_string())?;
    let dib = ClipboardMemory::new(&dib).map_err(|error| error.to_string())?;
    write(owner, [(png_format, png), (CF_DIBV5, dib)]).map_err(|error| error.to_string())
}

pub(super) fn write_file(owner: *mut c_void, path: &Path) -> io::Result<()> {
    let bytes = file_drop(path);
    write(owner, [(CF_HDROP, ClipboardMemory::new(&bytes)?)])
}

fn file_drop(path: &Path) -> Vec<u8> {
    // DROPFILES followed by the double-NUL-terminated list of UTF-16 paths.
    [20u32, 0, 0, 0, 1]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .chain(
            path.as_os_str()
                .encode_wide()
                .chain([0, 0])
                .flat_map(u16::to_le_bytes),
        )
        .collect()
}

#[cfg(test)]
#[path = "../../../tests/unit/commands/file_clipboard_windows.rs"]
mod tests;
