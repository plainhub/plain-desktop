//! Block device listing (the GraphQL `disks` query). Port of Go
//! `internal/graph/storage_disks_api.go`: whole disks only, virtual devices
//! hidden, stable `disk:`/`diskbyid:` IDs, sysfs fallbacks for model and
//! removable flags.

use crate::storage::blockdev::{
    LsblkDevice, disk_id_from_name, is_device_removable, is_user_visible_disk_name, parse_boolish,
    read_sys_block_model, run_lsblk,
};
use serde::Serialize;

#[derive(Debug, Serialize, Default)]
pub struct Disk {
    pub id: String,
    pub name: String,
    pub path: String,
    pub size_bytes: i64,
    pub removable: bool,
    pub model: Option<String>,
}

pub fn list_disks() -> Vec<Disk> {
    let devs = match run_lsblk(&["NAME", "PATH", "TYPE", "MODEL", "SIZE", "RM"]) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    disks_from_lsblk(&devs)
}

/// Pure mapping from an `lsblk` tree to user-visible whole disks — Go
/// `ListStorageDisks` minus the subprocess. Only top-level `TYPE=disk`
/// entries with user-visible names survive.
pub(crate) fn disks_from_lsblk(devs: &[LsblkDevice]) -> Vec<Disk> {
    devs.iter()
        .filter(|d| d.kind.trim() == "disk")
        .filter(|d| is_user_visible_disk_name(&d.name))
        .map(to_disk)
        .collect()
}

fn to_disk(d: &LsblkDevice) -> Disk {
    let name = d.name.trim().to_string();
    let mut path = d.path.trim().to_string();
    if path.is_empty() && !name.is_empty() {
        path = format!("/dev/{name}");
    }

    let size = d.size.unwrap_or(0);

    let removable = match parse_boolish(&d.rm) {
        Some(rm) => rm,
        None if !path.is_empty() => is_device_removable(&path),
        None => false,
    };

    // sysfs model is often more accurate than lsblk's column.
    let model = read_sys_block_model(&name).or_else(|| {
        d.model
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    });

    Disk {
        id: disk_id_from_name(&name),
        name,
        path,
        size_bytes: size,
        removable,
        model,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/storage/storage_disks.rs"]
mod tests;
