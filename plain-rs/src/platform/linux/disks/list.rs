use crate::platform::linux::disks::blockdev::LsblkDevice;
use crate::platform::linux::disks::blockdev::disk_id_from_name;
use crate::platform::linux::disks::blockdev::is_device_removable;
use crate::platform::linux::disks::blockdev::is_user_visible_disk_name;
use crate::platform::linux::disks::blockdev::parse_boolish;
use crate::platform::linux::disks::blockdev::read_sys_block_model;
use crate::platform::linux::disks::blockdev::run_lsblk;
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

pub fn read_disks() -> Result<Vec<Disk>, String> {
    let devs = run_lsblk(&["NAME", "PATH", "TYPE", "MODEL", "SIZE", "RM"])
        .map_err(|error| error.to_string())?;
    Ok(disks_from_lsblk(&devs))
}

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
#[path = "../../../../tests/unit/platform/linux/disks/list.rs"]
mod tests;
