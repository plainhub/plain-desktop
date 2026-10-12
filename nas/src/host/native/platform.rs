use plain_server::host::platform::{AppUpdateFacts, DiskFacts, DisksFacts};
use plain_server::platform::system::app_update::AppUpdate;

use crate::platform::linux::disks::list::{read_disks, Disk};

pub fn disks() -> Result<DisksFacts, String> {
    disks_facts(read_disks())
}

pub fn app_update() -> Result<AppUpdateFacts, String> {
    Ok(update_facts(
        plain_server::platform::system::app_update::app_update(),
    ))
}

/// A readable `lsblk` that reports no disk is an empty list, not a failure.
fn disks_facts(disks: Result<Vec<Disk>, String>) -> Result<DisksFacts, String> {
    disks.map(|disks| DisksFacts {
        disks: disks.into_iter().map(to_facts).collect(),
    })
}

fn update_facts(update: AppUpdate) -> AppUpdateFacts {
    AppUpdateFacts {
        current_version: update.current_version,
        latest_version: update.latest_version,
        has_update: update.has_update,
        url: update.url,
    }
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
