use std::ffi::CString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BUNDLE_NAME: &str = "PlainApp.app";
const EXECUTABLE: &str = "Contents/MacOS/plainapp-desktop";
const DEVELOPER_ID: &str = concat!(
    r#"identifier "com.ismartcoding.plain.desktop" and anchor apple generic"#,
    " and certificate leaf[field.1.2.840.113635.100.6.1.13] exists"
);

pub(super) fn supports_install() -> bool {
    installed_bundle().is_ok()
}

pub(super) fn install(dmg: &Path) -> Result<(), String> {
    let installed = installed_bundle()?;
    let original = std::fs::symlink_metadata(&installed).map_err(|error| error.to_string())?;
    let parent = installed.parent().ok_or("Missing application directory")?;
    let scratch = plain_libs::temporary::Builder::new()
        .prefix(".plainapp-update-")
        .tempdir_in(parent)
        .map_err(|error| format!("Cannot stage the update beside the installed app: {error}"))?;

    verify_code(&installed, DEVELOPER_ID)?;
    let signature = command(
        Command::new("/usr/bin/codesign")
            .args(["--display", "--requirements", "-"])
            .arg(&installed),
    )?;
    let requirement = [&signature.stdout, &signature.stderr]
        .into_iter()
        .filter_map(|stream| std::str::from_utf8(stream).ok())
        .flat_map(str::lines)
        .find_map(|line| line.strip_prefix("designated => "))
        .filter(|value| !value.is_empty())
        .ok_or("Missing installed app signing requirement")?;

    let mount_point = scratch.path().join("mounted");
    std::fs::create_dir(&mount_point).map_err(|error| error.to_string())?;
    command(
        Command::new("/usr/bin/hdiutil")
            .args([
                "attach",
                "-readonly",
                "-nobrowse",
                "-noautoopen",
                "-mountpoint",
            ])
            .arg(&mount_point)
            .arg(dmg),
    )?;
    let mut mount = MountedImage(Some(mount_point));
    let source = mount
        .0
        .as_ref()
        .ok_or("Missing mounted update")?
        .join(BUNDLE_NAME);
    require_bundle(&source)?;
    let staged = scratch.path().join(BUNDLE_NAME);
    command(Command::new("/usr/bin/ditto").arg(&source).arg(&staged))?;
    require_bundle(&staged)?;
    verify_code(&staged, DEVELOPER_ID)?;
    verify_code(&staged, requirement)?;
    mount.detach()?;

    let current = std::fs::symlink_metadata(&installed).map_err(|error| error.to_string())?;
    if !current.is_dir() || (original.dev(), original.ino()) != (current.dev(), current.ino()) {
        return Err("Installed application changed during the update".into());
    }
    swap_bundles(&installed, &staged)
        .map_err(|error| format!("Cannot replace the installed app: {error}"))
}

fn installed_bundle() -> Result<PathBuf, String> {
    if std::env::var_os("APP_SANDBOX_CONTAINER_ID").is_some() {
        return Err("Update this app through the Mac App Store".into());
    }
    let executable = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map_err(|error| error.to_string())?;
    let bundle = executable
        .ancestors()
        .nth(3)
        .filter(|bundle| {
            bundle
                .extension()
                .is_some_and(|extension| extension == "app")
        })
        .filter(|bundle| bundle.join(EXECUTABLE) == executable)
        .ok_or("The running executable is not inside an installed app bundle")?;
    if bundle
        .components()
        .any(|part| part.as_os_str() == "AppTranslocation")
        || bundle.starts_with("/Volumes")
    {
        return Err(
            "Move PlainApp out of the disk image or download folder quarantine before updating"
                .into(),
        );
    }
    require_bundle(bundle)?;
    Ok(bundle.into())
}

fn require_bundle(bundle: &Path) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(bundle).map_err(|error| error.to_string())?;
    let executable = bundle.join(EXECUTABLE);
    let executable_meta =
        std::fs::symlink_metadata(&executable).map_err(|error| error.to_string())?;
    if !meta.is_dir()
        || !executable_meta.is_file()
        || std::fs::canonicalize(&executable).ok().as_deref() != Some(executable.as_path())
    {
        return Err("Invalid PlainApp application bundle".into());
    }
    if bundle.join("Contents/_MASReceipt").exists() {
        return Err("Update this app through the Mac App Store".into());
    }
    Ok(())
}

fn verify_code(bundle: &Path, requirement: &str) -> Result<(), String> {
    command(
        Command::new("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(format!("-R={requirement}"))
            .arg(bundle),
    )
    .map(|_| ())
}

fn command(command: &mut Command) -> Result<Output, String> {
    let name = command.get_program().to_owned();
    let result = command
        .stdin(Stdio::null())
        .output()
        .map_err(|error| error.to_string())?;
    if result.status.success() {
        Ok(result)
    } else {
        let detail = String::from_utf8_lossy(&result.stderr);
        Err(format!(
            "{} failed: {}",
            name.to_string_lossy(),
            detail.trim()
        ))
    }
}

struct MountedImage(Option<PathBuf>);

impl MountedImage {
    fn detach(&mut self) -> Result<(), String> {
        if let Some(path) = &self.0 {
            command(Command::new("/usr/bin/hdiutil").arg("detach").arg(path))?;
            self.0 = None;
        }
        Ok(())
    }
}

impl Drop for MountedImage {
    fn drop(&mut self) {
        let _ = self.detach();
    }
}

fn swap_bundles(current: &Path, replacement: &Path) -> io::Result<()> {
    // RENAME_SWAP keeps the old bundle in place if the atomic exchange fails.
    const RENAME_SWAP: std::ffi::c_uint = 0x00000002;
    unsafe extern "C" {
        fn renamex_np(
            from: *const std::ffi::c_char,
            to: *const std::ffi::c_char,
            flags: std::ffi::c_uint,
        ) -> std::ffi::c_int;
    }
    let current = CString::new(current.as_os_str().as_bytes())?;
    let replacement = CString::new(replacement.as_os_str().as_bytes())?;
    // Both C strings live for the duration of the call and are not modified.
    if unsafe { renamex_np(current.as_ptr(), replacement.as_ptr(), RENAME_SWAP) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/commands/desktop_update_macos.rs"]
mod tests;
