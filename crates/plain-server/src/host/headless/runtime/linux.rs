use crate::assembly::server::ContentServer;
use serde_json::Value;
use std::sync::Arc;

pub(super) fn handles(method: &str) -> bool {
    matches!(
        method,
        "systemRelaunchApp"
            | crate::host::platform::SYSTEM_HOSTNAME_SET
            | crate::host::platform::SYSTEM_DISK_FORMAT
            | crate::host::platform::SYSTEM_SAMBA_SERVICE_FACTS
            | crate::host::platform::SYSTEM_SAMBA_APPLY
            | crate::host::platform::SYSTEM_SAMBA_PASSWORD
            | crate::host::platform::SYSTEM_MOUNT_FACTS
    )
}

pub(super) async fn dispatch(
    core: &Arc<ContentServer>,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    match method {
        "systemRelaunchApp" => {
            tokio::task::spawn_blocking(plain_rs::platform::linux::install::service::restart)
                .await
                .map_err(|error| error.to_string())?
                .map_err(|error| error.to_string())?;
            Ok(Value::Bool(true))
        }
        crate::host::platform::SYSTEM_HOSTNAME_SET => {
            let request: crate::host::platform::HostnameRequest =
                serde_json::from_value(params).map_err(|error| error.to_string())?;
            tokio::task::spawn_blocking(move || {
                plain_rs::platform::linux::hostname::set(&request.name)
            })
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
            Ok(Value::Bool(true))
        }
        crate::host::platform::SYSTEM_DISK_FORMAT => {
            let request: crate::host::platform::DiskFormatRequest =
                serde_json::from_value(params).map_err(|error| error.to_string())?;
            let core = core.clone();
            tokio::task::spawn_blocking(move || {
                    let prefs = core.shared_preferences()?;
                    let events = crate::assembly::headless::storage::event_sink(&core, false)?;
                    let db = core.headless_state().db.clone();
                    let result = plain_rs::platform::linux::disks::format::format_disk_single_partition(&prefs, &request.path, |mount| {
                        let _ = db.audit_event_add("unmount", mount, &request.client_id);
                    }, &events);
                    let _ = db.audit_event_add(if result.is_ok() { "format_disk" } else { "format_disk_failed" }, &request.path, &request.client_id);
                    let _ = core.headless_state().events.send(plain_rs::events::WsEvent::broadcast("DISK_FORMAT_DONE", serde_json::json!({"path":request.path,"ok":result.is_ok(),"error":result.as_ref().err().map(ToString::to_string)}).to_string()));
                    result.map(|_| Value::Bool(true)).map_err(|error| error.to_string())
                }).await.map_err(|error| error.to_string())?
        }
        crate::host::platform::SYSTEM_SAMBA_SERVICE_FACTS => {
            let facts = tokio::task::spawn_blocking(|| {
                let status = plain_rs::platform::linux::samba::get_service_status();
                crate::host::platform::SambaServiceFacts {
                    name: status.name,
                    active: status.active,
                    enabled: status.enabled,
                }
            })
            .await
            .map_err(|error| error.to_string())?;
            serde_json::to_value(facts).map_err(|error| error.to_string())
        }
        crate::host::platform::SYSTEM_SAMBA_APPLY => {
            let request: crate::host::platform::SambaApplyRequest =
                serde_json::from_value(params).map_err(|error| error.to_string())?;
            let core = core.clone();
            tokio::task::spawn_blocking(move || {
                plain_rs::platform::linux::samba::apply(core.preferences()?, &request.settings, "")
                    .map(|_| Value::Bool(true))
                    .map_err(|error| error.to_string())
            })
            .await
            .map_err(|error| error.to_string())?
        }
        crate::host::platform::SYSTEM_SAMBA_PASSWORD => {
            let request: crate::host::platform::SambaPasswordRequest =
                serde_json::from_value(params).map_err(|error| error.to_string())?;
            let core = core.clone();
            tokio::task::spawn_blocking(move || {
                core.preferences()?;
                plain_rs::platform::linux::samba::set_user_password(&request.password)
                    .map(|_| Value::Bool(true))
                    .map_err(|error| error.to_string())
            })
            .await
            .map_err(|error| error.to_string())?
        }
        crate::host::platform::SYSTEM_MOUNT_FACTS => {
            let core = core.clone();
            tokio::task::spawn_blocking(move || {
                    let mounts = plain_rs::platform::linux::mounts::list::list_mounts(core.preferences()?);
                    Ok(serde_json::Value::Array(mounts.into_iter().map(|mount| serde_json::json!({
                        "id":mount.id,"name":mount.name,"path":mount.path,"mountPoint":mount.mount_point,
                        "fsType":mount.fs_type,"totalBytes":mount.total_bytes,"usedBytes":mount.used_bytes,
                        "freeBytes":mount.free_bytes,"remote":mount.remote,"alias":mount.alias,"driveType":mount.drive_type,
                        "diskId":mount.disk_id,"partitionNum":mount.partition_num,"label":mount.label,"uuid":mount.uuid,
                    })).collect()))
                }).await.map_err(|error|error.to_string())?
        }
        _ => Err(format!("Unsupported Linux host method: {method}")),
    }
}
