use plain_server::host::platform::{AppUpdateFacts, DiskFacts, DisksFacts};

use crate::platform::linux::disks::list::Disk;

pub fn disks() -> Result<DisksFacts, String> {
    let disks = crate::platform::linux::disks::list::list_disks();
    if disks.is_empty() {
        return Err("lsblk reported no user-visible whole disk".to_string());
    }
    Ok(DisksFacts {
        disks: disks.into_iter().map(to_facts).collect(),
    })
}

pub fn app_update() -> Result<AppUpdateFacts, String> {
    Ok(AppUpdateFacts {
        current_version: crate::version::VERSION.to_string(),
        latest_version: None,
        has_update: false,
        url: None,
    })
}

fn to_facts(disk: Disk) -> DiskFacts {
    DiskFacts {
        id: disk.id,
        name: disk.name,
        path: disk.path,
        size_bytes: disk.size_bytes,
        removable: disk.removable,
        model: disk.model,
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/host/native/platform.rs"]
mod tests;
