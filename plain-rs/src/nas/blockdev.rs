//! Shared block-device helpers: `lsblk -J` runner plus the disk-ID scheme,
//! sysfs probes and device-name math used by the disks/mounts listings and
//! the automounter. Port of the Go `internal/graph/{lsblk,storage_disk_ids,
//! storage_blockdev,storage}.go` helpers.

use anyhow::{Result, anyhow};
use serde::Deserialize;
use std::process::Command;

// ----- lsblk -----

/// One `lsblk -J` device node. All columns are optional so a single struct
/// serves every caller (each passes its own `-o` column list).
#[derive(Deserialize, Debug, Clone, Default)]
pub struct LsblkDevice {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub fstype: String,
    #[serde(default)]
    pub uuid: String,
    #[serde(default)]
    pub label: String,
    /// Bytes (`-b`); `json.Number` on the Go side.
    #[serde(default)]
    pub size: Option<i64>,
    #[serde(default)]
    pub mountpoint: Option<String>,
    #[serde(default)]
    pub pkname: String,
    #[serde(default)]
    pub partn: Option<i64>,
    /// lsblk emits bool for modern versions, strings for old ones.
    #[serde(default)]
    pub rm: Option<serde_json::Value>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub children: Vec<LsblkDevice>,
}

#[derive(Deserialize, Debug)]
struct LsblkOutput {
    #[serde(default)]
    blockdevices: Vec<LsblkDevice>,
}

/// `lsblk -b -J -o <cols>`. Mirrors Go `runLsblkJSON`.
pub fn run_lsblk(cols: &[&str]) -> Result<Vec<LsblkDevice>> {
    let out = Command::new("lsblk")
        .arg("-b")
        .arg("-J")
        .arg("-o")
        .arg(cols.join(","))
        .output()
        .map_err(|e| anyhow!("lsblk spawn: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(anyhow!("lsblk failed: {}", stderr.trim()));
    }
    let parsed: LsblkOutput =
        serde_json::from_slice(&out.stdout).map_err(|e| anyhow!("lsblk json: {e}"))?;
    Ok(parsed.blockdevices)
}

/// Flatten an `lsblk` tree depth-first (Go `scanFilesystems` walk).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn flatten_devices(devs: &[LsblkDevice]) -> Vec<&LsblkDevice> {
    fn walk<'a>(d: &'a LsblkDevice, out: &mut Vec<&'a LsblkDevice>) {
        out.push(d);
        for c in &d.children {
            walk(c, out);
        }
    }
    let mut v = Vec::new();
    for d in devs {
        walk(d, &mut v);
    }
    v
}

/// Go `parseLsblkBoolish`: accept bool / number / "1"/"true"/"yes" strings.
/// Second value is false when the field is absent or unrecognized.
pub fn parse_boolish(v: &Option<serde_json::Value>) -> Option<bool> {
    match v.as_ref()? {
        serde_json::Value::Bool(b) => Some(*b),
        serde_json::Value::Number(n) => n.as_f64().map(|f| f != 0.0),
        serde_json::Value::String(s) => {
            let s = s.trim().to_ascii_lowercase();
            match s.as_str() {
                "1" | "true" | "yes" => Some(true),
                "0" | "false" | "no" => Some(false),
                _ => None,
            }
        }
        _ => None,
    }
}

// ----- device-name math -----

pub fn all_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// Strip a trailing partition index: `sda3`→`sda`, `nvme0n1p2`→`nvme0n1`,
/// `mmcblk0p1`→`mmcblk0`; whole-disk names stay intact. Go
/// `baseBlockDeviceName`.
pub fn base_block_device_name(name: &str) -> String {
    let name = name.trim();
    if name.starts_with("mmcblk") {
        if let Some(i) = name.rfind('p')
            && i > 0
            && all_digits(&name[i + 1..])
        {
            return name[..i].to_string();
        }
        if all_digits(name.trim_start_matches("mmcblk")) {
            return name.to_string();
        }
    }
    if name.starts_with("md") {
        if let Some(i) = name.rfind('p')
            && i > 0
            && all_digits(&name[i + 1..])
        {
            return name[..i].to_string();
        }
        if all_digits(name.trim_start_matches("md")) {
            return name.to_string();
        }
    }
    if name.starts_with("nvme") {
        if let Some(i) = name.rfind('p')
            && i > 0
            && all_digits(&name[i + 1..])
        {
            return name[..i].to_string();
        }
        if let Some(i) = name.rfind('n') {
            let left = name[..i].trim_start_matches("nvme");
            let right = &name[i + 1..];
            if all_digits(left) && all_digits(right) {
                return name.to_string();
            }
        }
    }
    let bytes = name.as_bytes();
    let mut j = bytes.len();
    while j > 0 && bytes[j - 1].is_ascii_digit() {
        j -= 1;
    }
    if j > 0 && j < bytes.len() {
        name[..j].to_string()
    } else {
        name.to_string()
    }
}

/// Hide virtual/system devices that confuse users in a NAS UI. Go
/// `isUserVisibleDiskName`.
pub fn is_user_visible_disk_name(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty() && !n.starts_with("zram") && !n.starts_with("loop") && !n.starts_with("ram")
}

// ----- sysfs probes -----

fn read_sys_trimmed(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// `/sys/class/block/<dev>/device/model` — often more accurate than lsblk.
pub fn read_sys_block_model(dev_name: &str) -> Option<String> {
    read_sys_trimmed(&format!("/sys/class/block/{dev_name}/device/model"))
}

/// Go `isDeviceRemovable`: sysfs fallback when lsblk's `RM` is missing.
pub fn is_device_removable(src: &str) -> bool {
    if !src.starts_with("/dev/") {
        return false;
    }
    let base = &src["/dev/".len()..];
    let dev = base_block_device_name(base);
    for p in [
        format!("/sys/block/{dev}/removable"),
        format!("/sys/block/{dev}/device/removable"),
    ] {
        if let Some(s) = read_sys_trimmed(&p)
            && (s == "1" || s.eq_ignore_ascii_case("true"))
        {
            return true;
        }
    }
    false
}

// ----- disk IDs -----

const DISK_ID_PREFIX: &str = "disk:";
const DISK_ID_BY_ID_PREFIX: &str = "diskbyid:";

/// Stable identifier for a disk: prefer `/dev/disk/by-id` (stable across
/// `/dev/sdX` renames), fall back to the kernel name. Go `diskIDFromName`.
pub fn disk_id_from_name(name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        return String::new();
    }
    let dev_path = format!("/dev/{name}");
    if let Some(by_id) = best_by_id_name_for_dev_path(&dev_path) {
        return format!("{DISK_ID_BY_ID_PREFIX}{by_id}");
    }
    format!("{DISK_ID_PREFIX}{name}")
}

fn score_by_id_name(name: &str) -> i32 {
    // Prefer globally stable IDs.
    if name.starts_with("wwn-") {
        50
    } else if name.starts_with("nvme-") {
        40
    } else if name.starts_with("ata-") {
        30
    } else if name.starts_with("scsi-") {
        20
    } else if name.starts_with("usb-") {
        10
    } else {
        0
    }
}

/// Best `/dev/disk/by-id` link pointing at `dev_path`; partition-specific
/// links (`-part`) are ignored. Go `bestByIDNameForDevPath`.
pub fn best_by_id_name_for_dev_path(dev_path: &str) -> Option<String> {
    let dev_path = dev_path.trim();
    if dev_path.is_empty() {
        return None;
    }
    let resolved = std::fs::canonicalize(dev_path)
        .map(|p| p.to_string_lossy().to_string())
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| dev_path.to_string());

    let entries = std::fs::read_dir("/dev/disk/by-id").ok()?;
    let mut best: Option<(String, i32)> = None;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().trim().to_string();
        if name.is_empty() || name.contains("-part") {
            continue;
        }
        let link = format!("/dev/disk/by-id/{name}");
        let Ok(target) = std::fs::canonicalize(&link) else {
            continue;
        };
        let target = target.to_string_lossy().to_string();
        if target.is_empty() || target != resolved {
            continue;
        }
        let score = score_by_id_name(&name);
        if best.as_ref().map(|(_, s)| score > *s).unwrap_or(true) {
            best = Some((name, score));
        }
    }
    best.map(|(n, _)| n)
}

/// For device-mapper / md devices, resolve the single underlying base disk
/// via `/sys/class/block/<dev>/slaves` (one level of recursion). Go
/// `resolveUnderlyingSingleBaseDisk`.
pub fn resolve_underlying_single_base_disk(dev_name: &str) -> Option<String> {
    let dev_name = dev_name.trim();
    if dev_name.is_empty() {
        return None;
    }
    let entries = std::fs::read_dir(format!("/sys/class/block/{dev_name}/slaves")).ok()?;
    let mut uniq = std::collections::BTreeSet::new();
    for e in entries.flatten() {
        let mut n = e.file_name().to_string_lossy().trim().to_string();
        if n.is_empty() {
            continue;
        }
        if (n.starts_with("dm-") || n.starts_with("md"))
            && let Some(pd) = resolve_underlying_single_base_disk(&n)
        {
            n = pd;
        }
        let b = {
            let b = base_block_device_name(&n);
            if b.is_empty() { n.clone() } else { b }
        };
        uniq.insert(b);
    }
    if uniq.len() != 1 {
        return None;
    }
    uniq.into_iter().next()
}

#[cfg(test)]
#[path = "../../tests/unit/nas/blockdev.rs"]
mod tests;
