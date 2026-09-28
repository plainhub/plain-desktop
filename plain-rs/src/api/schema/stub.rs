use async_graphql::Object;

use super::types::Mount;

#[derive(Default)]
pub struct StubQuery;

#[Object]
impl StubQuery {
    async fn sms_count(&self, _query: String) -> i32 {
        0
    }
    async fn contact_count(&self, _query: String) -> i32 {
        0
    }
    async fn call_count(&self, _query: String) -> i32 {
        0
    }
    async fn package_count(&self, _query: String) -> i32 {
        0
    }
    async fn note_count(&self, _query: String) -> i32 {
        0
    }
    async fn feed_entry_count(&self, _query: String) -> i32 {
        0
    }
    async fn mounts(&self) -> Vec<Mount> {
        let disks = sysinfo::Disks::new_with_refreshed_list();
        disks
            .iter()
            .map(|disk| {
                let mount_point = disk.mount_point().to_string_lossy().into_owned();
                let total_bytes = disk.total_space().min(i64::MAX as u64) as i64;
                let free_bytes = disk.available_space().min(i64::MAX as u64) as i64;
                let fs_type = disk.file_system().to_string_lossy().into_owned();
                let remote = is_remote_filesystem(&fs_type);
                Mount {
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
                        crate::api::enums::DriveType::UsbStorage
                    } else {
                        crate::api::enums::DriveType::InternalStorage
                    },
                    disk_id: mount_point,
                }
            })
            .collect()
    }
}

fn is_remote_filesystem(fs_type: &str) -> bool {
    let fs_type = fs_type.to_ascii_lowercase();
    ["nfs", "smb", "cifs", "sshfs", "webdav"]
        .iter()
        .any(|kind| fs_type.contains(kind))
}

#[cfg(test)]
#[path = "../../../tests/unit/api/schema/stub.rs"]
mod tests;
