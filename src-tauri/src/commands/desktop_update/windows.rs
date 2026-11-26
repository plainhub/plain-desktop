use std::{
    ffi::{OsStr, OsString, c_void},
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    ptr,
};

use tauri::utils::{config::BundleType, platform::bundle_type};

#[link(name = "shell32")]
unsafe extern "system" {
    fn ShellExecuteW(
        parent: *mut c_void,
        operation: *const u16,
        file: *const u16,
        parameters: *const u16,
        directory: *const u16,
        show: i32,
    ) -> isize;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetSystemDirectoryW(buffer: *mut u16, length: u32) -> u32;
}

pub(super) fn install(path: &Path, app: &tauri::AppHandle) -> Result<(), String> {
    if !path.is_absolute()
        || !path
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.is_file())
    {
        return Err("Verified Windows installer is missing".into());
    }
    let (program, parameters) = match (bundle_type(), path.extension().and_then(OsStr::to_str)) {
        (Some(BundleType::Nsis), Some("exe")) => (path.to_owned(), OsString::from("/P /UPDATE /R")),
        (Some(BundleType::Msi), Some("msi")) => {
            let mut parameters = OsString::from("/i \"");
            parameters.push(path);
            parameters.push("\" /passive /promptrestart AUTOLAUNCHAPP=True");
            (msiexec()?, parameters)
        }
        _ => return Err("The installer does not match this Windows installation".into()),
    };

    let operation = wide(OsStr::new("open"))?;
    let program = wide(program.as_os_str())?;
    let parameters = wide(&parameters)?;
    // Use the same installer-owned update and relaunch controls as Tauri's updater.
    let status = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            operation.as_ptr(),
            program.as_ptr(),
            parameters.as_ptr(),
            ptr::null(),
            5, // SW_SHOW
        )
    };
    if status <= 32 {
        return Err(format!(
            "Windows could not start the installer (error {status})"
        ));
    }

    // The staged installer must outlive this process. It restarts the newly installed app.
    app.exit(0);
    Ok(())
}

fn msiexec() -> Result<PathBuf, String> {
    let mut directory = [0; 260];
    let length = unsafe { GetSystemDirectoryW(directory.as_mut_ptr(), directory.len() as u32) };
    let length = length as usize;
    if length == 0 || length >= directory.len() {
        return Err("Windows system directory could not be located".into());
    }
    let directory = PathBuf::from(OsString::from_wide(&directory[..length]));
    if !directory.is_absolute() {
        return Err("Windows system directory is not absolute".into());
    }
    Ok(directory.join("msiexec.exe"))
}

fn wide(text: &OsStr) -> Result<Vec<u16>, String> {
    let mut result = text.encode_wide().collect::<Vec<_>>();
    if result.contains(&0) {
        return Err("Invalid Windows installer path or parameters".into());
    }
    result.push(0);
    Ok(result)
}
