use std::fs::{self, File, Metadata, OpenOptions, Permissions};
use std::io::{self, Read, Seek};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub(super) fn supports_install() -> bool {
    current_image().is_ok()
}

pub(super) fn install(image: &Path) -> Result<(), String> {
    let installed = current_image()?;
    let parent = installed.parent().ok_or("Missing AppImage directory")?;
    let scratch = plain_libs::temporary::Builder::new()
        .prefix(".plainapp-update-")
        .tempdir_in(parent)
        .map_err(|error| {
            format!("Cannot stage the update beside the installed AppImage: {error}")
        })?;
    replace(&installed, image, scratch.path()).map_err(|error| error.to_string())
}

fn current_image() -> Result<PathBuf, String> {
    if !matches!(
        tauri::utils::platform::bundle_type(),
        Some(tauri::utils::config::BundleType::AppImage)
    ) {
        return Err("In-app updates are only supported for AppImage on Linux".into());
    }
    let image = PathBuf::from(std::env::var_os("APPIMAGE").ok_or("Missing AppImage path")?);
    let mount = PathBuf::from(std::env::var_os("APPDIR").ok_or("Missing AppImage mount")?);
    let canonical_image = fs::canonicalize(&image).map_err(|error| error.to_string())?;
    let canonical_mount = fs::canonicalize(&mount).map_err(|error| error.to_string())?;
    let executable = std::env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|error| error.to_string())?;
    if canonical_image != image
        || !mount.is_absolute()
        || !executable.starts_with(&canonical_mount)
        || executable == canonical_mount
    {
        return Err("Cannot locate the running AppImage safely".into());
    }
    let meta = fs::symlink_metadata(&image).map_err(|error| error.to_string())?;
    if !meta.is_file() || meta.mode() & 0o100 == 0 || meta.mode() & 0o7022 != 0 || meta.nlink() != 1
    {
        return Err("The AppImage must be an ordinary, securely owned executable file".into());
    }
    let parent = image.parent().ok_or("Missing AppImage directory")?;
    let directory = fs::metadata(parent).map_err(|error| error.to_string())?;
    if directory.uid() != meta.uid() || directory.mode() & 0o022 != 0 {
        return Err("The AppImage directory must not be shared for writing".into());
    }
    Ok(image)
}

fn replace(installed: &Path, update: &Path, scratch: &Path) -> io::Result<()> {
    let original = fs::symlink_metadata(installed)?;
    let mut old = File::open(installed)?;
    let mut input = File::open(update)?;
    let stage = fs::metadata(scratch)?;
    if !original.is_file()
        || original.uid() == 0
        || original.uid() != stage.uid()
        || !same_inode(&original, &old.metadata()?)
    {
        return Err(io::Error::other(
            "Cannot update this AppImage as the current user",
        ));
    }
    if appimage_header(&mut old)? != appimage_header(&mut input)? {
        return Err(io::Error::other(
            "Downloaded AppImage architecture does not match",
        ));
    }

    let staged = scratch.join("PlainApp.AppImage");
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&staged)?;
    input.rewind()?;
    io::copy(&mut input, &mut output)?;
    output.set_permissions(Permissions::from_mode(original.mode() & 0o777))?;
    output.sync_all()?;
    drop(output);
    if !same_inode(&original, &fs::symlink_metadata(installed)?) {
        return Err(io::Error::other(
            "Installed AppImage changed during the update",
        ));
    }
    fs::rename(&staged, installed)
}

fn same_inode(left: &Metadata, right: &Metadata) -> bool {
    (left.dev(), left.ino()) == (right.dev(), right.ino())
}

fn appimage_header(file: &mut File) -> io::Result<[u8; 4]> {
    let mut header = [0; 20];
    file.read_exact(&mut header)?;
    if &header[..4] != b"\x7fELF" || &header[8..11] != b"AI\x02" {
        return Err(io::Error::other("Invalid type 2 AppImage"));
    }
    Ok([header[4], header[5], header[18], header[19]])
}

#[cfg(test)]
#[path = "../../../../tests/unit/commands/desktop_update_linux.rs"]
mod tests;
