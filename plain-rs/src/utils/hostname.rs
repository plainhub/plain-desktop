//! Tiny wrapper around `gethostname(2)`.
//!
//! Returns the OS-reported hostname as a `String` (lossy UTF-8 —
//! hostnames are ASCII on Linux, but the syscall returns bytes so we
//! round-trip through `String::from_utf8_lossy`).
//!
//! On non-Unix targets this returns an empty `String` so cross
//! `cargo check` still works.

#[cfg(unix)]
pub fn get() -> String {
    // `HOST_NAME_MAX` is 255 on Linux; 64 is a safe portable upper bound
    // that the kernel will simply truncate to anyway. We pass a
    // 1024-byte buffer to be safe — `gethostname` returns success even
    // when the name is truncated to fit, and the documented behaviour
    // is that it always null-terminates.
    let mut buf = [0u8; 1024];
    // SAFETY: `gethostname` writes at most `buf.len()` bytes and always
    // null-terminates. The slice pointer is valid for `buf.len()` bytes.
    let ret = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut _, buf.len()) };
    if ret != 0 {
        return String::new();
    }
    // Find the first NUL.
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..len]).into_owned()
}

#[cfg(not(unix))]
pub fn get() -> String {
    String::new()
}

#[cfg(all(test, unix))]
mod tests {
    #[test]
    fn returns_non_empty_on_linux() {
        let h = super::get();
        // A Linux box always has a hostname; it may be truncated but not empty.
        assert!(!h.is_empty());
    }
}
