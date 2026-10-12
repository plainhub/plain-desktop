use plain_server::host::platform::{AppUpdateFacts, DiskFacts, DisksFacts};

use crate::commands::updater::UpdateCheck;

struct MountEntry {
    device: String,
    mount_point: String,
}

pub fn disks() -> Result<DisksFacts, String> {
    disks_facts(collect_disks())
}

/// A readable OS that reports no disk is an empty list, not a failure.
fn disks_facts(disks: Result<Vec<DiskFacts>, String>) -> Result<DisksFacts, String> {
    disks.map(DisksFacts { disks })
}

pub async fn app_update(app: &tauri::AppHandle) -> Result<AppUpdateFacts, String> {
    let current_version = app.package_info().version.to_string();
    let check = crate::commands::updater::check_for_updates(app.clone()).await?;
    Ok(update_facts(current_version, check))
}

fn update_facts(current_version: String, check: UpdateCheck) -> AppUpdateFacts {
    AppUpdateFacts {
        current_version,
        latest_version: Some(check.latest_version),
        has_update: check.has_update,
        url: Some(check.release_url),
    }
}

#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_mounts(text: &str) -> Vec<MountEntry> {
    let mut entries = Vec::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let (Some(device), Some(mount_point)) = (fields.next(), fields.next()) else {
            continue;
        };
        if !device.starts_with("/dev/") {
            continue;
        }
        entries.push(MountEntry {
            device: device.to_string(),
            mount_point: decode_mount_point(mount_point),
        });
    }
    entries
}

#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn decode_mount_point(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escape = bytes[index] == b'\\' && index + 3 < bytes.len();
        if escape {
            if let Ok(byte) = u8::from_str_radix(&raw[index + 1..index + 4], 8) {
                decoded.push(byte);
                index += 4;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
fn is_nested(inner: &str, outer: &str) -> bool {
    if inner == outer {
        return false;
    }
    let prefix = if outer.ends_with('/') {
        outer.to_string()
    } else {
        format!("{outer}/")
    };
    inner.starts_with(&prefix)
}

#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
fn base_name(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    trimmed.rsplit('/').next().unwrap_or(trimmed).to_string()
}

#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
fn disk_name(entry: &MountEntry) -> String {
    let name = if entry.mount_point == "/" {
        base_name(&entry.device)
    } else {
        base_name(&entry.mount_point)
    };
    if name.is_empty() {
        entry.device.clone()
    } else {
        name
    }
}

#[cfg(target_os = "linux")]
fn collect_disks() -> Result<Vec<DiskFacts>, String> {
    let mounts = std::fs::read_to_string("/proc/self/mounts")
        .map_err(|error| format!("cannot read /proc/self/mounts: {error}"))?;
    let entries = parse_mounts(&mounts);
    let mut disks = Vec::with_capacity(entries.len());
    for entry in &entries {
        if entries
            .iter()
            .any(|other| is_nested(&entry.mount_point, &other.mount_point))
        {
            continue;
        }
        disks.push(to_disk(entry));
    }
    Ok(disks)
}

#[cfg(target_os = "linux")]
fn to_disk(entry: &MountEntry) -> DiskFacts {
    let block = base_name(&entry.device);
    let size_bytes = block_attribute(&block, "size")
        .and_then(|sectors| sectors.parse::<u64>().ok())
        .map(|sectors| sectors.saturating_mul(512).min(i64::MAX as u64) as i64)
        .unwrap_or(0);
    DiskFacts {
        id: entry.device.clone(),
        name: disk_name(entry),
        path: entry.mount_point.clone(),
        size_bytes,
        removable: block_attribute(&block, "removable").as_deref() == Some("1"),
        model: block_attribute(&block, "device/model"),
    }
}

#[cfg(target_os = "linux")]
fn block_attribute(block: &str, attribute: &str) -> Option<String> {
    std::fs::read_to_string(format!("/sys/class/block/{block}/{attribute}"))
        .ok()
        .or_else(|| {
            std::fs::read_to_string(format!("/sys/class/block/{block}/../{attribute}")).ok()
        })
        .map(|value| value.trim().to_string())
}

#[cfg(target_os = "macos")]
fn collect_disks() -> Result<Vec<DiskFacts>, String> {
    let mut buffer: *mut libc::statfs = std::ptr::null_mut();
    let count = unsafe { libc::getmntinfo(&mut buffer, libc::MNT_NOWAIT) };
    if count <= 0 || buffer.is_null() {
        return Err("getmntinfo reported no mounted volume".to_string());
    }
    let mounted: Vec<(MountEntry, i64)> =
        unsafe { std::slice::from_raw_parts(buffer, count as usize) }
            .iter()
            .filter_map(|stat| {
                let device = c_string(&stat.f_mntfromname);
                if !device.starts_with("/dev/") {
                    return None;
                }
                let mount_point = c_string(&stat.f_mntonname);
                let size_bytes = (stat.f_blocks as i64).saturating_mul(stat.f_bsize as i64);
                Some((
                    MountEntry {
                        device,
                        mount_point,
                    },
                    size_bytes,
                ))
            })
            .collect();
    let mut disks = Vec::with_capacity(mounted.len());
    for (entry, size_bytes) in &mounted {
        if mounted
            .iter()
            .any(|(other, _)| is_nested(&entry.mount_point, &other.mount_point))
        {
            continue;
        }
        disks.push(DiskFacts {
            id: entry.device.clone(),
            name: disk_name(entry),
            path: entry.mount_point.clone(),
            size_bytes: *size_bytes,
            // macOS exposes no per-volume removable flag; every volume
            // outside the boot volume is user-attached.
            removable: entry.mount_point != "/" && entry.mount_point.starts_with("/Volumes/"),
            model: None,
        });
    }
    Ok(disks)
}

#[cfg(target_os = "macos")]
fn c_string(raw: &[libc::c_char]) -> String {
    unsafe { std::ffi::CStr::from_ptr(raw.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn collect_disks() -> Result<Vec<DiskFacts>, String> {
    Err(format!(
        "disk enumeration is not implemented on {}",
        std::env::consts::OS
    ))
}

#[cfg(test)]
#[path = "../../../tests/unit/host/native/platform.rs"]
mod tests;
