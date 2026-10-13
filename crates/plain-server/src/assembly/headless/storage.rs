use anyhow::Result;
use std::path::{Path, PathBuf};

pub(super) fn validate_automount(enabled: bool) -> Result<()> {
    if !enabled {
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    return plain_rs::platform::linux::install::service::require_root();
    #[cfg(not(target_os = "linux"))]
    anyhow::bail!("Automatic disk mounting is available on Linux");
}

pub(super) fn source_directories(
    prefs: &plain_rs::prefs::Prefs,
    requested: &[PathBuf],
) -> Result<Vec<String>> {
    let saved = plain_rs::prefs::media::source_directories(prefs)?;
    let home = crate::platform::system::consts::home_directory();
    let mounted = mounted_directories(prefs);
    choose_directories(&saved, requested, home.as_deref(), &mounted)
}

fn mounted_directories(_prefs: &plain_rs::prefs::Prefs) -> Vec<PathBuf> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mounted = disks
        .iter()
        .filter(|disk| disk.is_removable())
        .map(|disk| disk.mount_point().to_path_buf())
        .collect::<Vec<_>>();
    #[cfg(target_os = "linux")]
    let mounted = mounted
        .into_iter()
        .chain(
            plain_rs::platform::linux::mounts::list::list_mounts(_prefs)
                .into_iter()
                .filter_map(|mount| mount.mount_point)
                .filter(|path| {
                    plain_rs::platform::linux::mounts::automount::parse_usb_slot(path).is_some()
                })
                .map(PathBuf::from),
        )
        .collect();
    mounted
}

fn choose_directories(
    saved: &[String],
    requested: &[PathBuf],
    home: Option<&Path>,
    mounted: &[PathBuf],
) -> Result<Vec<String>> {
    let mut roots = if !requested.is_empty() {
        for path in requested {
            anyhow::ensure!(
                path.is_dir(),
                "Media directory does not exist: {}",
                path.display()
            );
        }
        requested.to_vec()
    } else if !saved.is_empty() {
        saved.iter().map(PathBuf::from).collect()
    } else {
        home.into_iter()
            .map(Path::to_path_buf)
            .chain(mounted.iter().cloned())
            .collect()
    };
    #[cfg(target_os = "macos")]
    if requested.is_empty() && saved.is_empty() {
        if let Some(home) = home {
            roots.push(home.join("Library/Mobile Documents/com~apple~CloudDocs"));
        }
    }
    roots = roots
        .into_iter()
        .filter(|path| path.is_dir())
        .map(|path| path.canonicalize())
        .collect::<std::io::Result<Vec<_>>>()?;
    roots.sort_by(|left, right| {
        left.components()
            .count()
            .cmp(&right.components().count())
            .then_with(|| left.cmp(right))
    });
    let mut unique: Vec<PathBuf> = Vec::new();
    for root in roots {
        let cloud = cfg!(target_os = "macos")
            && root.ends_with("Library/Mobile Documents/com~apple~CloudDocs");
        if cloud || !unique.iter().any(|parent| root.starts_with(parent)) {
            unique.push(root);
        }
    }
    Ok(unique
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect())
}

pub(super) fn start_source_scan(
    catalog: &crate::native::media_catalog::NativeMediaCatalog,
) -> Result<crate::services::files::media_scans::Snapshot, String> {
    catalog.start_source_scan("/")
}

#[cfg(target_os = "linux")]
pub(crate) fn event_sink(
    core: &crate::assembly::server::ContentServer,
    include_mounted: bool,
) -> Result<plain_rs::platform::linux::mounts::automount::EventSink, String> {
    let db = core.headless_state().db.clone();
    let catalog = core.native_media_catalog()?;
    let prefs = core.shared_preferences()?;
    let state = core.headless_state().clone();
    Ok(std::sync::Arc::new(move |kind, message| {
        if kind == "mounts_changed" {
            if state
                .peer_status
                .public_active
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                let result = (|| -> Result<(), String> {
                    let mut roots = plain_rs::prefs::media::source_directories(&prefs)?;
                    if include_mounted {
                        roots.extend(
                            mounted_directories(&prefs)
                                .into_iter()
                                .map(|path| path.to_string_lossy().into_owned()),
                        );
                    }
                    let roots = choose_directories(&roots, &[], None, &[])
                        .map_err(|error| error.to_string())?;
                    if catalog.source_directories()? != roots {
                        catalog.set_source_directories(&roots)?;
                        catalog.watch_source_directories()?;
                        if !roots.is_empty() {
                            start_source_scan(&catalog)?;
                        }
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    log::error!("mount sources update failed: {error}");
                }
            }
            return;
        }
        if let Err(error) = db.audit_event_add(kind, message, "") {
            log::error!("mount audit failed: {error}");
        }
    }))
}

#[cfg(test)]
#[path = "../../../tests/unit/assembly/headless/storage.rs"]
mod tests;
