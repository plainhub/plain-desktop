use crate::host::platform::{AppUpdateFacts, DiskFacts, DisksFacts};
use crate::platform::system::app_update::AppUpdate;

use plain_rs::platform::linux::disks::list::{Disk, read_disks};

pub fn disks() -> Result<DisksFacts, String> {
    disks_facts(read_disks())
}

pub fn app_update() -> Result<AppUpdateFacts, String> {
    Ok(update_facts(
        crate::platform::system::app_update::app_update(),
    ))
}

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

pub fn device_info() -> Result<serde_json::Value, String> {
    let info = plain_rs::platform::linux::device_info::collect(
        crate::platform::system::version::version(),
        &crate::platform::system::version::full_version(),
    )
    .map_err(|error| error.to_string())?;
    Ok(serde_json::json!({
        "name":info.hostname,"platform":std::env::consts::OS.to_uppercase(),
        "manufacturer":plain_rs::platform::linux::device_info::dmi_manufacturer(),
        "model":info.model,"osName":info.os,"osVersion":info.os,"kernelVersion":info.kernel_version,
        "appVersion":info.app_version,"appBuildNumber":info.app_full_version,"language":"",
        "cpuArch":info.arch,"cpuModel":info.cpu_model,"totalMemory":info.memory_total_bytes,
        "totalStorage":plain_rs::platform::linux::mounts::list::df_usage("/").0,
    }))
}

pub fn device_status() -> Result<serde_json::Value, String> {
    let status = plain_rs::platform::linux::device_info::collect_device_status()
        .map_err(|error| error.to_string())?;
    Ok(serde_json::json!({
        "uptimeSec":status.uptime_sec,"cpuUsage":status.cpu_usage,"memoryAvailable":status.memory_available,
        "temperatures":status.temperatures.into_iter().map(|value|serde_json::json!({"label":value.label,"celsius":value.celsius})).collect::<Vec<_>>(),
        "storageAvailable":plain_rs::platform::linux::mounts::list::df_usage("/").1,
    }))
}

#[cfg(test)]
#[path = "../../../../tests/unit/host/nas/native/platform.rs"]
mod tests;
