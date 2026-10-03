use std::{io, path::Path};

#[cfg(unix)]
pub(super) fn no_replace(src: &Path, dst: &Path) -> io::Result<()> {
    let src = std::ffi::CString::new(src.as_os_str().as_encoded_bytes())?;
    let dst = std::ffi::CString::new(dst.as_os_str().as_encoded_bytes())?;
    #[cfg(target_vendor = "apple")]
    let result = unsafe { libc::renamex_np(src.as_ptr(), dst.as_ptr(), libc::RENAME_EXCL) as i64 };
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            src.as_ptr(),
            libc::AT_FDCWD,
            dst.as_ptr(),
            libc::RENAME_NOREPLACE,
        ) as i64
    };
    #[cfg(not(any(target_vendor = "apple", target_os = "linux", target_os = "android")))]
    let result = {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "atomic no-replace rename unavailable",
        ));
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
#[cfg(windows)]
pub(super) fn no_replace(src: &Path, dst: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(src: *const u16, dst: *const u16, flags: u32) -> i32;
    }
    let encode = |path: &Path| -> io::Result<Vec<u16>> {
        let mut encoded = path.as_os_str().encode_wide().collect::<Vec<_>>();
        if encoded.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path contains NUL",
            ));
        }
        encoded.push(0);
        Ok(encoded)
    };
    let src = encode(src)?;
    let dst = encode(dst)?;
    if unsafe { MoveFileExW(src.as_ptr(), dst.as_ptr(), 0) } != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

pub(super) fn copy_required(error: &io::Error, overwrite: bool) -> bool {
    if error.kind() == io::ErrorKind::CrossesDevices {
        return true;
    }
    #[cfg(unix)]
    if !overwrite
        && error
            .raw_os_error()
            .is_some_and(|code| matches!(code, libc::ENOSYS | libc::EINVAL | libc::EOPNOTSUPP))
    {
        return true;
    }
    false
}
#[cfg(unix)]
pub(super) fn same_identity(src: &Path, dst: &Path) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let source = std::fs::metadata(src)?;
    let destination = std::fs::metadata(dst)?;
    Ok(source.dev() == destination.dev() && source.ino() == destination.ino())
}
#[cfg(windows)]
pub(super) fn same_identity(src: &Path, dst: &Path) -> io::Result<bool> {
    use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
    #[repr(C)]
    #[derive(Default)]
    struct Information {
        attributes: u32,
        created: [u32; 2],
        accessed: [u32; 2],
        written: [u32; 2],
        volume: u32,
        size_high: u32,
        size_low: u32,
        links: u32,
        index_high: u32,
        index_low: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(handle: *mut std::ffi::c_void, info: *mut Information)
        -> i32;
    }
    let identity = |path: &Path| -> io::Result<(u32, u32, u32)> {
        let file = std::fs::OpenOptions::new()
            .access_mode(0)
            .share_mode(7)
            .custom_flags(0x02000000)
            .open(path)?;
        let mut info = Information::default();
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((info.volume, info.index_high, info.index_low))
    };
    Ok(identity(src)? == identity(dst)?)
}
