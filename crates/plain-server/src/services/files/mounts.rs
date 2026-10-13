#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MountFacts {
    pub id: String,
    pub name: String,
    pub path: String,
    pub mount_point: String,
    pub fs_type: String,
    pub total_bytes: i64,
    pub used_bytes: i64,
    pub free_bytes: i64,
    pub remote: bool,
    pub alias: String,
    pub drive_type: String,
    pub disk_id: String,
}

pub fn list() -> Vec<MountFacts> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .iter()
        .filter(|disk| should_include_mount(disk.mount_point().to_string_lossy().as_ref()))
        .map(|disk| {
            let mount_point = disk.mount_point().to_string_lossy().into_owned();
            let total_bytes = disk.total_space().min(i64::MAX as u64) as i64;
            let free_bytes = disk.available_space().min(i64::MAX as u64) as i64;
            let fs_type = disk.file_system().to_string_lossy().into_owned();
            let remote = is_remote_filesystem(&fs_type);
            MountFacts {
                id: mount_point.clone(),
                name: disk.name().to_string_lossy().into_owned(),
                path: mount_point.clone(),
                mount_point: mount_point.clone(),
                fs_type,
                total_bytes,
                used_bytes: total_bytes.saturating_sub(free_bytes),
                free_bytes,
                remote,
                alias: String::new(),
                drive_type: if mount_point.starts_with("/Volumes/") {
                    "USB_STORAGE"
                } else {
                    "INTERNAL_STORAGE"
                }
                .into(),
                disk_id: mount_point,
            }
        })
        .collect()
}

pub fn facts() -> Result<serde_json::Value, String> {
    serde_json::to_value(list()).map_err(|error| error.to_string())
}

pub(crate) fn should_include_mount(mount_point: &str) -> bool {
    #[cfg(target_os = "macos")]
    if mount_point == "/System/Volumes/Data" {
        return false;
    }
    let _ = mount_point;
    true
}

pub(crate) fn is_remote_filesystem(fs_type: &str) -> bool {
    let fs_type = fs_type.to_ascii_lowercase();
    ["nfs", "smb", "cifs", "sshfs", "webdav"]
        .iter()
        .any(|kind| fs_type.contains(kind))
}

#[cfg(test)]
#[path = "../../../tests/unit/services/files/mounts.rs"]
mod tests;
