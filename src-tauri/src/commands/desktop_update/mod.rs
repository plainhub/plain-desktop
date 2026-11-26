use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use plain_libs::anyhow::{Context, Result, ensure};
use plain_libs::crypto::{hash::sha256_reader, release};
use plain_libs::protocols::http_client::{Client, Url, redirect};
use serde::Deserialize;
use tauri::{Manager, WebviewWindow, utils::config::BundleType};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

const REPOSITORY: &str = "plainhub/plain-desktop";
const RELEASES: &str = "https://api.github.com/repos/plainhub/plain-desktop/releases/latest";
const R2: &str = "https://d.plainapp.app/desktop/";
const MAX_INSTALLER: u64 = 512 * 1024 * 1024;
static INSTALLING: AtomicBool = AtomicBool::new(false);

#[derive(Deserialize)]
struct PublishedRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
}

fn supported_suffix(os: &str, arch: &str, bundle: BundleType) -> Option<&'static str> {
    match (os, arch, bundle) {
        ("macos", "aarch64", BundleType::App) => Some("macos-aarch64.dmg"),
        ("macos", "x86_64", BundleType::App) => Some("macos-x64.dmg"),
        ("windows", "x86_64", BundleType::Msi) => Some("windows.msi"),
        ("windows", "x86_64", BundleType::Nsis) => Some("windows.exe"),
        ("linux", "x86_64", BundleType::AppImage) => Some("linux.AppImage"),
        _ => None,
    }
}

fn installed_suffix() -> Option<&'static str> {
    if cfg!(debug_assertions) {
        return None;
    }
    #[cfg(target_os = "macos")]
    if !macos::supports_install() {
        return None;
    }
    #[cfg(target_os = "linux")]
    if !linux::supports_install() {
        return None;
    }
    supported_suffix(
        std::env::consts::OS,
        std::env::consts::ARCH,
        tauri::utils::platform::bundle_type()?,
    )
}

pub(super) fn supported() -> bool {
    installed_suffix().is_some()
}

fn allowed_caller(label: &str, url: &Url) -> bool {
    label == "about"
        && url.path() == "/about"
        && match (url.scheme(), url.host_str()) {
            ("tauri", Some("localhost")) | ("http" | "https", Some("tauri.localhost")) => {
                url.port().is_none()
            }
            _ => false,
        }
}

fn stable_version(raw: &str) -> Result<[u64; 3]> {
    let mut version = [0u64; 3];
    let mut segments = raw.split('.');
    for value in &mut version {
        let segment = segments.next().context("Invalid version")?;
        ensure!(
            !segment.is_empty()
                && (segment == "0" || !segment.starts_with('0'))
                && segment.bytes().all(|b| b.is_ascii_digit()),
            "Invalid version"
        );
        *value = segment.parse()?;
    }
    ensure!(segments.next().is_none(), "Invalid version");
    Ok(version)
}

async fn download(client: &Client, url: &str, limit: u64, mut output: impl Write) -> Result<()> {
    let mut response = client.get(url).send().await?;
    ensure!(
        response.status().is_success(),
        "Update download failed (HTTP {})",
        response.status()
    );
    ensure!(
        response.content_length().is_none_or(|size| size <= limit),
        "Update file is too large"
    );
    let mut size = 0u64;
    while let Some(chunk) = response.chunk().await? {
        size += chunk.len() as u64;
        ensure!(size <= limit, "Update file is too large");
        output.write_all(&chunk)?;
    }
    ensure!(size > 0, "Update file is empty");
    output.flush()?;
    Ok(())
}

fn verify_package(path: &Path, expected: &str) -> Result<()> {
    ensure!(
        sha256_reader(std::fs::File::open(path)?)? == expected,
        "Update package checksum mismatch"
    );
    Ok(())
}

async fn update(window: &WebviewWindow, suffix: &str) -> Result<()> {
    let client = plain_libs::protocols::http_client::builder()
        .https_only(true)
        .redirect(redirect::Policy::none())
        .user_agent("PlainApp Desktop Updater")
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(900))
        .build()?;

    let mut metadata = Vec::new();
    download(&client, RELEASES, 4 * 1024 * 1024, &mut metadata).await?;
    let latest: PublishedRelease = plain_libs::serde_json::from_slice(&metadata)?;
    ensure!(
        !latest.draft && !latest.prerelease,
        "No published stable update"
    );
    let version = latest
        .tag_name
        .strip_prefix('v')
        .context("Invalid release tag")?;
    let app = window.app_handle();
    let installed = app.package_info().version.to_string();
    ensure!(
        stable_version(version)? > stable_version(&installed)?,
        "No newer desktop update"
    );

    let mut manifest = Vec::new();
    let mut signature = Vec::new();
    let folder = format!("{R2}v{version}/");
    download(
        &client,
        &format!("{folder}release-manifest.txt"),
        16 * 1024,
        &mut manifest,
    )
    .await?;
    download(
        &client,
        &format!("{folder}release-manifest.sig"),
        64,
        &mut signature,
    )
    .await?;
    let name = format!("PlainApp-{version}-{suffix}");
    let hash = release::package_hash(
        &manifest,
        &signature,
        release::RELEASE_SIGNING_PUBLIC_KEY.trim(),
        REPOSITORY,
        version,
        &name,
    )?
    .to_owned();

    let staging = plain_libs::temporary::Builder::new()
        .prefix("plainapp-update-")
        .tempdir()?;
    let path = staging.path().join(&name);
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    download(
        &client,
        &format!("{R2}{name}"),
        MAX_INSTALLER,
        io::BufWriter::new(file),
    )
    .await?;
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<()> {
        verify_package(&path, &hash)?;
        #[cfg(target_os = "macos")]
        macos::install(&path).map_err(plain_libs::anyhow::Error::msg)?;
        #[cfg(target_os = "linux")]
        linux::install(&path).map_err(plain_libs::anyhow::Error::msg)?;
        #[cfg(target_os = "windows")]
        {
            let kept = staging.keep();
            if let Err(error) = windows::install(&path, &app) {
                let _ = std::fs::remove_dir_all(kept);
                return Err(plain_libs::anyhow::Error::msg(error));
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            drop(staging);
            app.request_restart();
        }
        Ok(())
    })
    .await??;
    Ok(())
}

struct InstallGuard;
impl Drop for InstallGuard {
    fn drop(&mut self) {
        INSTALLING.store(false, Ordering::Release);
    }
}

#[tauri::command]
pub async fn install_desktop_update(window: WebviewWindow) -> Result<(), String> {
    let url = window.url().map_err(|e| e.to_string())?;
    if !allowed_caller(window.label(), &url) {
        return Err("Desktop updates are only available in the local About window".into());
    }
    let suffix = installed_suffix().ok_or("This installation uses system-managed updates")?;
    if INSTALLING.swap(true, Ordering::AcqRel) {
        return Err("A desktop update is already running".into());
    }
    let guard = InstallGuard;
    update(&window, suffix)
        .await
        .map_err(|error| error.to_string())?;
    std::mem::forget(guard);
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/unit/commands/desktop_update.rs"]
mod tests;
