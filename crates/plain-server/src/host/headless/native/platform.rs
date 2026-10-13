use crate::host::platform::{AppUpdateFacts, DiskFacts, DisksFacts};
use crate::platform::system::app_update::AppUpdate;

#[cfg(target_os = "linux")]
use plain_rs::platform::linux::disks::list::{Disk, read_disks};

#[cfg(target_os = "linux")]
pub fn disks() -> Result<DisksFacts, String> {
    disks_facts(read_disks())
}

pub fn app_update() -> Result<AppUpdateFacts, String> {
    Ok(update_facts(
        crate::platform::system::app_update::app_update(),
    ))
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
pub fn device_status() -> Result<serde_json::Value, String> {
    let status = plain_rs::platform::linux::device_info::collect_device_status()
        .map_err(|error| error.to_string())?;
    Ok(serde_json::json!({
        "uptimeSec":status.uptime_sec,"cpuUsage":status.cpu_usage,"memoryAvailable":status.memory_available,
        "temperatures":status.temperatures.into_iter().map(|value|serde_json::json!({"label":value.label,"celsius":value.celsius})).collect::<Vec<_>>(),
        "storageAvailable":plain_rs::platform::linux::mounts::list::df_usage("/").1,
    }))
}

#[cfg(not(target_os = "linux"))]
pub fn disks() -> Result<DisksFacts, String> {
    Ok(DisksFacts {
        disks: sysinfo::Disks::new_with_refreshed_list()
            .iter()
            .map(|disk| DiskFacts {
                id: disk.mount_point().to_string_lossy().into_owned(),
                name: disk.name().to_string_lossy().into_owned(),
                path: disk.mount_point().to_string_lossy().into_owned(),
                size_bytes: disk.total_space().min(i64::MAX as u64) as i64,
                removable: disk.is_removable(),
                model: None,
            })
            .collect(),
    })
}

#[cfg(not(target_os = "linux"))]
pub fn device_info() -> Result<serde_json::Value, String> {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    system.refresh_cpu_all();
    let storage: u64 = sysinfo::Disks::new_with_refreshed_list()
        .iter()
        .map(|disk| disk.total_space())
        .sum();
    Ok(serde_json::json!({
        "name":sysinfo::System::host_name().unwrap_or_default(), "platform":std::env::consts::OS.to_uppercase(),
        "manufacturer":"", "model":"", "osName":sysinfo::System::name().unwrap_or_default(),
        "osVersion":sysinfo::System::os_version().unwrap_or_default(), "kernelVersion":sysinfo::System::kernel_version().unwrap_or_default(),
        "appVersion":crate::platform::system::version::version(), "appBuildNumber":crate::platform::system::version::full_version(),
        "language":"", "cpuArch":std::env::consts::ARCH, "cpuModel":system.cpus().first().map(|cpu|cpu.brand()).unwrap_or_default(),
        "totalMemory":system.total_memory(), "totalStorage":storage,
    }))
}

#[cfg(not(target_os = "linux"))]
pub fn device_status() -> Result<serde_json::Value, String> {
    let mut system = sysinfo::System::new();
    system.refresh_cpu_usage();
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    system.refresh_cpu_usage();
    system.refresh_memory();
    let available: u64 = sysinfo::Disks::new_with_refreshed_list()
        .iter()
        .map(|disk| disk.available_space())
        .sum();
    Ok(
        serde_json::json!({"uptimeSec":sysinfo::System::uptime(), "cpuUsage":system.global_cpu_usage(),
        "memoryAvailable":system.available_memory(), "temperatures":[], "storageAvailable":available}),
    )
}

#[cfg(test)]
#[path = "../../../../tests/unit/host/headless/native/platform.rs"]
mod tests;
