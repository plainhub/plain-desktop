use std::{ffi::c_void, os::windows::ffi::OsStrExt, path::Path};

#[link(name = "user32")]
unsafe extern "system" {
    fn OpenClipboard(owner: *mut c_void) -> i32;
    fn EmptyClipboard() -> i32;
    fn SetClipboardData(format: u32, memory: *mut c_void) -> *mut c_void;
    fn CloseClipboard() -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalAlloc(flags: u32, bytes: usize) -> *mut c_void;
    fn GlobalLock(memory: *mut c_void) -> *mut c_void;
    fn GlobalUnlock(memory: *mut c_void) -> i32;
    fn GlobalFree(memory: *mut c_void) -> *mut c_void;
}

#[repr(C)]
struct DropFiles {
    files_offset: u32,
    point: [i32; 2],
    nonclient: i32,
    wide: i32,
}

struct ClipboardMemory(*mut c_void);

impl Drop for ClipboardMemory {
    fn drop(&mut self) {
        // SAFETY: This handle remains owned here until SetClipboardData succeeds.
        unsafe { GlobalFree(self.0) };
    }
}

pub(super) fn write(path: &Path, owner: *mut c_void) -> Result<(), String> {
    const GMEM_MOVEABLE: u32 = 0x0002;
    const CF_HDROP: u32 = 15;
    let files = file_list(path);
    let header_size = std::mem::size_of::<DropFiles>();
    // SAFETY: The allocation holds the DROPFILES header and double-terminated UTF-16 list.
    let memory = ClipboardMemory(unsafe {
        GlobalAlloc(GMEM_MOVEABLE, header_size + files.len() * size_of::<u16>())
    });
    if memory.0.is_null() {
        return Err(std::io::Error::last_os_error().to_string());
    }
    // SAFETY: GlobalLock receives the live allocation owned by memory.
    let data = unsafe { GlobalLock(memory.0) };
    if data.is_null() {
        return Err(std::io::Error::last_os_error().to_string());
    }
    // SAFETY: Both writes fit the locked allocation; the handle is unlocked before transfer.
    unsafe {
        data.cast::<DropFiles>().write(DropFiles {
            files_offset: header_size as u32,
            point: [0, 0],
            nonclient: 0,
            wide: 1,
        });
        std::ptr::copy_nonoverlapping(
            files.as_ptr(),
            data.cast::<u8>().add(header_size).cast(),
            files.len(),
        );
        GlobalUnlock(memory.0);
    }
    // SAFETY: Tauri provides the live window handle on the window's main thread.
    if unsafe { OpenClipboard(owner) } == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let result = (|| {
        // SAFETY: This thread has opened the clipboard for the live owner window.
        if unsafe { EmptyClipboard() } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        // SAFETY: CF_HDROP receives unlocked movable memory with the documented layout.
        if unsafe { SetClipboardData(CF_HDROP, memory.0) }.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        std::mem::forget(memory);
        Ok(())
    })();
    // SAFETY: Close every successful OpenClipboard, including failed writes.
    let closed = unsafe { CloseClipboard() };
    let close_result = if closed == 0 {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(())
    };
    result.and(close_result)
}

fn file_list(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain([0, 0]).collect()
}

#[cfg(test)]
#[path = "../../tests/unit/commands/file_clipboard_windows.rs"]
mod tests;
