//! Storage mount listing — the GraphQL `mounts` query. Port of Go
//! `internal/graph/storage{,_mounts_list,_mounts_partitions,_mounts_
//! blockmeta}.go`: mounted volumes from `/proc/mounts` (with lsblk block
//! metadata for UUID/label/diskID) plus unmounted partitions from `lsblk`.
//!
//! The list is intentionally flat; the UI correlates mounts with disks via
//! `disk_id` and tells volumes from partitions by `path` (partitions carry
//! a block-device path, volumes don't).

use crate::storage::blockdev::{
    LsblkDevice, base_block_device_name, disk_id_from_name, is_user_visible_disk_name,
    resolve_underlying_single_base_disk, run_lsblk,
};
use crate::prefs::Prefs;
use serde::Serialize;
use serde_json::Value;

const VOLUME_ID_REMOTE_PREFIX: &str = "remote:";
const VOLUME_ID_FSUUID_PREFIX: &str = "fsuuid:";
const VOLUME_ID_DEV_PREFIX: &str = "dev:";
const MOUNT_ID_PARTITION_PREFIX: &str = "part:";

#[derive(Debug, Serialize, Default)]
pub struct Mount {
    pub id: String,
    pub name: String,
    pub path: Option<String>,
    pub partition_num: Option<i32>,
    pub label: Option<String>,
    pub uuid: Option<String>,
    pub mount_point: Option<String>,
    pub fs_type: Option<String>,
    pub total_bytes: i64,
    pub used_bytes: Option<i64>,
    pub free_bytes: Option<i64>,
    pub alias: Option<String>,
    pub remote: bool,
    pub drive_type: Option<String>,
    pub disk_id: Option<String>,
}

/// `ListMounts` = mounted volumes + disk partitions.
pub fn list_mounts(prefs: &Prefs) -> Vec<Mount> {
    let mut out = list_mounted_volumes(prefs);
    out.extend(list_disk_partitions());
    out
}

/// Metadata per resolved device path, from one `lsblk` pass. Also indexed
/// by resolved symlink target so `/dev/mapper/*` and `/dev/disk/by-*`
/// sources resolve too. Go `buildLsblkVolumeDevMetaMap`.
#[derive(Default, Clone)]
struct VolumeDevMeta {
    pkname: String,
    label: String,
    uuid: String,
}

fn build_volume_dev_meta_map() -> std::collections::HashMap<String, VolumeDevMeta> {
    let mut m = std::collections::HashMap::new();
    let Ok(devs) = run_lsblk(&["NAME", "PATH", "TYPE", "PKNAME", "LABEL", "UUID"]) else {
        return m;
    };
    fn walk(d: &LsblkDevice, m: &mut std::collections::HashMap<String, VolumeDevMeta>) {
        let path = d.path.trim();
        if !path.is_empty() {
            let meta = VolumeDevMeta {
                pkname: d.pkname.trim().to_string(),
                label: d.label.trim().to_string(),
                uuid: d.uuid.trim().to_string(),
            };
            m.insert(path.to_string(), meta.clone());
            if let Ok(resolved) = std::fs::canonicalize(path) {
                let resolved = resolved.to_string_lossy().trim().to_string();
                if !resolved.is_empty() {
                    m.insert(resolved, meta);
                }
            }
        }
        for c in &d.children {
            walk(c, m);
        }
    }
    for d in &devs {
        walk(d, &mut m);
    }
    m
}

/// Pseudo filesystems we don't care about (Go `skipTypes`, plus the ones
/// the earlier MVP listed).
const SKIP_FS_TYPES: &[&str] = &[
    "proc",
    "sysfs",
    "devpts",
    "tmpfs",
    "devtmpfs",
    "securityfs",
    "cgroup",
    "cgroup2",
    "pstore",
    "bpf",
    "overlay",
    "squashfs",
    "autofs",
    "mqueue",
    "hugetlbfs",
    "ramfs",
    "fusectl",
    "debugfs",
    "tracefs",
    "configfs",
    "efivarfs",
    "nsfs",
    "fuse.gvfsd-fuse",
    "binfmt_misc",
];

/// System mountpoints hidden from the user-facing volumes concept.
const SKIP_MOUNT_POINTS: &[&str] = &["/boot", "/boot/efi", "/efi", "/boot/firmware"];

/// One classified `/proc/mounts` line that survives the volume filters.
#[derive(Debug, Clone, PartialEq)]
pub struct MountLine {
    pub src: String,
    pub target: String,
    pub fstype: String,
    pub is_block: bool,
    pub is_remote: bool,
}

/// Pure filter for one `/proc/mounts` line: `None` when the line is not a
/// user-facing storage volume (pseudo fs, system mountpoint, bind mount,
/// neither block nor remote, zram/loop). Extracted so the skip rules are
/// deterministically testable.
pub fn classify_mount_line(line: &str) -> Option<MountLine> {
    let cols: Vec<&str> = line.split_whitespace().collect();
    if cols.len() < 3 {
        return None;
    }
    let src = cols[0];
    let target = cols[1];
    let fstype = cols[2];
    let opts = cols.get(3).copied().unwrap_or("");

    if SKIP_FS_TYPES.contains(&fstype) {
        return None;
    }
    if SKIP_MOUNT_POINTS.contains(&target) {
        return None;
    }
    // Bind mounts are not standalone storage volumes.
    if !opts.is_empty() && opts.contains("bind") {
        return None;
    }

    let is_block = src.starts_with("/dev/");
    let is_remote = src.contains(':')
        || fstype.starts_with("nfs")
        || fstype.contains("cifs")
        || fstype.contains("smb")
        || fstype.contains("sshfs");
    if !is_block && !is_remote {
        return None;
    }
    // RAM-backed or loop devices often represent ephemeral/system mounts.
    if src.starts_with("/dev/zram") || src.starts_with("/dev/loop") {
        return None;
    }
    Some(MountLine {
        src: src.to_string(),
        target: target.to_string(),
        fstype: fstype.to_string(),
        is_block,
        is_remote,
    })
}

/// Mounted volumes from `/proc/mounts`. Go `listMountedVolumes`.
fn list_mounted_volumes(prefs: &Prefs) -> Vec<Mount> {
    let mut volumes = Vec::new();
    let Ok(content) = std::fs::read_to_string("/proc/mounts") else {
        return volumes;
    };

    let alias_map = crate::media::kv::storage::get_map(prefs);
    let dev_meta = build_volume_dev_meta_map();

    let mut seen_src = std::collections::HashSet::new();
    let mut by_target: std::collections::HashMap<String, Mount> = std::collections::HashMap::new();
    let mut target_order: Vec<String> = Vec::new();

    for line in content.lines() {
        let Some(ml) = classify_mount_line(line) else {
            continue;
        };
        let (src, target, fstype, is_block, is_remote) = (
            ml.src.as_str(),
            ml.target.as_str(),
            ml.fstype.as_str(),
            ml.is_block,
            ml.is_remote,
        );

        // Dedupe multiple mount points of the same block device.
        if is_block && !seen_src.insert(src.to_string()) {
            continue;
        }

        let Some((total, used, free)) = statfs_usage(target) else {
            continue;
        };

        // Stable ID + block metadata.
        let mut fs_uuid = String::new();
        let id;
        if is_remote {
            id = format!("{VOLUME_ID_REMOTE_PREFIX}{src}");
        } else if is_block {
            let dev_resolved = std::fs::canonicalize(src)
                .map(|p| p.to_string_lossy().to_string())
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| src.to_string());
            if let Some(meta) = dev_meta.get(&dev_resolved) {
                fs_uuid = meta.uuid.trim().to_string();
            }
            if !fs_uuid.is_empty() {
                id = format!("{VOLUME_ID_FSUUID_PREFIX}{fs_uuid}");
            } else {
                id = format!("{VOLUME_ID_DEV_PREFIX}{dev_resolved}");
            }
        } else {
            id = src.to_string();
        }

        let mut v = Mount {
            id,
            name: target.rsplit('/').next().unwrap_or("").to_string(),
            mount_point: Some(target.to_string()),
            fs_type: Some(fstype.to_string()),
            total_bytes: total,
            used_bytes: Some(used),
            free_bytes: Some(free),
            remote: is_remote,
            uuid: if fs_uuid.is_empty() {
                None
            } else {
                Some(fs_uuid)
            },
            ..Default::default()
        };

        if is_block {
            apply_volume_block_meta(&mut v, src, &dev_meta);
        }

        if let Some(Value::String(a)) = alias_map.get(&v.id)
            && !a.trim().is_empty()
        {
            v.alias = Some(a.clone());
        }

        // Keep the latest mount for a given mountpoint but preserve
        // first-seen ordering (mountpoints can be reused).
        if !by_target.contains_key(target) {
            target_order.push(target.to_string());
        }
        by_target.insert(target.to_string(), v);
    }

    for target in target_order {
        if let Some(v) = by_target.remove(&target) {
            volumes.push(v);
        }
    }
    volumes
}

/// Go `applyVolumeBlockMeta`: fill label + owning-disk ID for a volume.
fn apply_volume_block_meta(
    v: &mut Mount,
    src: &str,
    meta: &std::collections::HashMap<String, VolumeDevMeta>,
) {
    let resolved = std::fs::canonicalize(src)
        .map(|p| p.to_string_lossy().to_string())
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| src.to_string());
    if !resolved.starts_with("/dev/") {
        return;
    }
    let dev_name = resolved.trim_start_matches("/dev/").to_string();

    if v.label.is_none()
        && let Some(m) = meta.get(&resolved)
    {
        let lbl = m.label.trim();
        if !lbl.is_empty() {
            v.label = Some(lbl.to_string());
        }
    }

    // Determine the owning disk.
    let mut disk_name = String::new();
    if dev_name.starts_with("dm-") || dev_name.starts_with("md") {
        // Virtual devices: only set diskID when resolvable to a single
        // underlying base disk.
        if let Some(pd) = resolve_underlying_single_base_disk(&dev_name) {
            disk_name = pd;
        }
    } else if let Some(m) = meta.get(&resolved) {
        // Prefer lsblk PKNAME (partition -> disk), normalized.
        let pk = m.pkname.trim();
        if !pk.is_empty() {
            disk_name = base_block_device_name(pk).trim().to_string();
        }
    }
    if disk_name.is_empty() {
        disk_name = base_block_device_name(&dev_name).trim().to_string();
    }

    let id = disk_id_from_name(&disk_name);
    if !id.is_empty() {
        v.disk_id = Some(id);
    }
}

/// Partitions discovered from `lsblk` (mounted or not). ID namespace
/// `part:` avoids colliding with mounted-volume IDs. Go
/// `listDiskPartitions`.
fn list_disk_partitions() -> Vec<Mount> {
    let Ok(devs) = run_lsblk(&[
        "NAME",
        "PATH",
        "TYPE",
        "SIZE",
        "FSTYPE",
        "UUID",
        "LABEL",
        "MOUNTPOINT",
        "PKNAME",
        "PARTN",
    ]) else {
        return vec![];
    };

    let mut parts = Vec::new();
    for d in &devs {
        if d.kind.trim() != "disk" || !is_user_visible_disk_name(&d.name) {
            continue;
        }
        let disk_id = disk_id_from_name(d.name.trim());
        for c in &d.children {
            if c.kind.trim() != "part" {
                continue;
            }
            // Hide LVM physical volumes and other non-user-facing roles.
            if c.fstype.trim().eq_ignore_ascii_case("LVM2_member") {
                continue;
            }
            let mp = c.mountpoint.as_deref().unwrap_or("").trim();
            if SKIP_MOUNT_POINTS.contains(&mp) {
                continue;
            }
            let name = c.name.trim().to_string();
            let mut path = c.path.trim().to_string();
            if path.is_empty() && !name.is_empty() {
                path = format!("/dev/{name}");
            }
            if path.is_empty() {
                continue;
            }

            let size = c.size.unwrap_or(0);
            // Hide tiny partitions without a filesystem (GPT/BIOS/metadata).
            if c.fstype.trim().is_empty() && size > 0 && size < 32 * 1024 * 1024 {
                continue;
            }

            let uuid = c.uuid.trim();
            let id_suffix = if uuid.is_empty() { path.as_str() } else { uuid };
            let partn = c.partn.filter(|n| *n > 0).map(|n| n as i32);

            parts.push(Mount {
                id: format!("{MOUNT_ID_PARTITION_PREFIX}{id_suffix}"),
                name,
                path: Some(path),
                partition_num: partn,
                label: non_empty(c.label.trim()),
                uuid: non_empty(uuid),
                mount_point: non_empty(mp),
                fs_type: non_empty(c.fstype.trim()),
                total_bytes: size,
                remote: false,
                disk_id: non_empty(&disk_id),
                ..Default::default()
            });
        }
    }
    parts
}

fn non_empty(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// `statfs(2)` total/used/available bytes. Replaces the old `df -B1`
/// subprocess on this path (the Go side uses statfs too).
fn statfs_usage(path: &str) -> Option<(i64, i64, i64)> {
    #[cfg(target_os = "linux")]
    {
        let st = nix::sys::statfs::statfs(path).ok()?;
        let bsize = st.block_size() as i64;
        let total = st.blocks() as i64 * bsize;
        let free = st.blocks_available() as i64 * bsize;
        Some((total, total - free, free))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let (total, free) = df_usage(path);
        Some((total, total - free, free))
    }
}

/// Total/available bytes via `df -B1` — kept for `deviceInfo.totalStorage`
/// (root fs = "device storage") and as the non-Linux statfs fallback.
pub fn df_usage(path: &str) -> (i64, i64) {
    let Ok(out) = std::process::Command::new("df")
        .args(["-B1", "--output=size,avail", path])
        .output()
    else {
        return (0, 0);
    };
    let Ok(text) = String::from_utf8(out.stdout) else {
        return (0, 0);
    };
    let mut lines = text.lines();
    let _ = lines.next();
    if let Some(line) = lines.next() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 2 {
            let total = cols[0].parse().unwrap_or(0);
            let free = cols[1].parse().unwrap_or(0);
            return (total, free);
        }
    }
    (0, 0)
}

#[cfg(test)]
#[path = "../../tests/unit/storage/mounts.rs"]
mod tests;
