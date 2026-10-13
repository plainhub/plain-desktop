use super::server::ContentServer;
use crate::host::nas::runtime::Runtime;
use crate::platform::system::consts::AppPaths;
use anyhow::{Result, anyhow};
use plain_rs::platform::linux::install::packages::has_cmd;
use plain_rs::platform::linux::mounts::automount::{EventSink, Watcher};
use std::{path::Path, sync::Arc};

pub fn initialize_tls() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

pub fn set_password(directory: &Path, password: &str) -> Result<()> {
    std::fs::create_dir_all(directory)?;
    let prefs = plain_rs::prefs::Prefs::load(&plain_rs::prefs::default_path(directory))?;
    plain_rs::prefs::system::Command::SetPassword {
        password: password.to_owned(),
    }
    .apply(&prefs)?;
    Ok(())
}

pub struct NasService {
    core: Arc<ContentServer>,
    host: tokio::task::JoinHandle<Result<(), String>>,
    watcher: Option<Watcher>,
}

impl NasService {
    pub async fn start(paths: &AppPaths) -> Result<Self> {
        if std::env::var_os("PLAIN_NAS_ALLOW_NONROOT").is_none() {
            plain_rs::platform::linux::install::service::require_root()?;
        }
        initialize_tls();
        plain_media::storage::paths::set(paths.data_dir.clone(), paths.cache_dir.clone());
        let core = Arc::new(ContentServer::open(&paths.data_dir).map_err(anyhow::Error::msg)?);
        crate::platform::system::log::set_file(&crate::platform::system::log::default_log_file(
            &paths.data_dir,
        ));
        let capabilities = capabilities();
        let runtime = Arc::new(Runtime::new(core.clone(), capabilities));
        let host = tokio::spawn(runtime.run());
        let mut service = Self {
            core,
            host,
            watcher: None,
        };
        if let Err(error) = service.start_services(paths).await {
            service.shutdown().await;
            return Err(error);
        }
        Ok(service)
    }

    async fn start_services(&mut self, paths: &AppPaths) -> Result<()> {
        let prefs = self.core.shared_preferences().map_err(anyhow::Error::msg)?;
        let events = event_sink(&self.core).map_err(anyhow::Error::msg)?;
        let mount_prefs = prefs.clone();
        let mount_events = events.clone();
        tokio::task::spawn_blocking(move || {
            plain_rs::platform::linux::mounts::automount::ensure_mounted_usb_volumes(
                &mount_prefs,
                &mount_events,
            )
        })
        .await??;
        self.watcher = Some(Watcher::start(prefs.clone(), events));
        let mount_prefs = prefs.clone();
        let roots = tokio::task::spawn_blocking(move || {
            plain_rs::platform::linux::mounts::list::list_mounts(&mount_prefs)
                .into_iter()
                .filter_map(|mount| mount.mount_point)
                .filter(|mount| is_usb_mount(mount))
                .collect::<Vec<_>>()
        })
        .await?;
        let catalog = self
            .core
            .native_media_catalog()
            .map_err(anyhow::Error::msg)?;
        catalog
            .set_source_directories(&roots)
            .map_err(anyhow::Error::msg)?;
        catalog
            .configure_scan_directories(&paths.cache_dir, None)
            .map_err(anyhow::Error::msg)?;
        catalog
            .watch_source_directories()
            .map_err(anyhow::Error::msg)?;
        *self
            .core
            .nas_state()
            .tls_identity
            .write()
            .map_err(|_| anyhow!("TLS identity configuration unavailable"))? =
            crate::http::tls_identity::IdentitySource::Pem {
                certificate: paths.tls_cert.clone(),
                key: paths.tls_key.clone(),
            };
        if let Some(root) = web_root() {
            self.core.set_web_root(&root.to_string_lossy());
        }
        let ports = self
            .core
            .start_configured_public()
            .await
            .map_err(anyhow::Error::msg)?;
        log::info!("HTTP server listening on :{}", ports.0);
        log::info!("HTTPS server listening on :{}", ports.1);
        if !roots.is_empty() {
            catalog.start_source_scan("").map_err(anyhow::Error::msg)?;
        }
        Ok(())
    }

    pub async fn wait(&mut self) -> Result<()> {
        if let Some(watcher) = &mut self.watcher {
            tokio::select! {
                result = &mut self.host => {
                    result.map_err(anyhow::Error::from)?.map_err(anyhow::Error::msg)?;
                    Err(anyhow!("NAS host session stopped"))
                }
                result = watcher.wait() => {
                    result?;
                    Err(anyhow!("NAS mount watcher stopped"))
                }
            }
        } else {
            (&mut self.host).await?.map_err(anyhow::Error::msg)?;
            Err(anyhow!("NAS host session stopped"))
        }
    }

    pub async fn shutdown(self) {
        if let Some(watcher) = self.watcher {
            if let Err(error) = watcher.shutdown().await {
                log::error!("storage watcher stopped: {error}");
            }
        }
        self.core.stop_core().await;
        if !self.host.is_finished() {
            let _ = self.host.await;
        }
    }
}

pub(crate) fn event_sink(core: &ContentServer) -> Result<EventSink, String> {
    let db = core.nas_state().db.clone();
    let catalog = core.native_media_catalog()?;
    let prefs = core.shared_preferences()?;
    let state = core.nas_state().clone();
    Ok(Arc::new(move |kind, message| {
        if kind == "mounts_changed" {
            if state
                .peer_status
                .public_active
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                let roots = plain_rs::platform::linux::mounts::list::list_mounts(&prefs)
                    .into_iter()
                    .filter_map(|mount| mount.mount_point)
                    .filter(|mount| is_usb_mount(mount))
                    .collect::<Vec<_>>();
                let result = (|| -> Result<(), String> {
                    if catalog.source_directories()? != roots {
                        catalog.set_source_directories(&roots)?;
                        catalog.watch_source_directories()?;
                        if !roots.is_empty() {
                            catalog.start_source_scan("")?;
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

fn capabilities() -> Vec<String> {
    let mut capabilities = vec!["MEDIA_SCAN".into()];
    if !plain_rs::platform::linux::samba::detect_systemd_service_name().is_empty() {
        capabilities.push("LAN_SHARE".into());
    }
    if has_cmd("lsblk") {
        capabilities.push("DISK_MANAGER".into());
    }
    if has_cmd("libreoffice") || has_cmd("soffice") {
        capabilities.push("DOC_PREVIEW".into());
    }
    capabilities
}

fn web_root() -> Option<std::path::PathBuf> {
    [
        "web/dist",
        "../web/dist",
        "../../web/dist",
        "src/web_dist",
        "crates/plain-cli/src/web_dist",
        "dist",
    ]
    .into_iter()
    .map(std::path::PathBuf::from)
    .find(|path| path.is_dir())
}

fn is_usb_mount(mount: &str) -> bool {
    mount.strip_prefix("/mnt/usb").is_some_and(|slot| {
        !slot.is_empty()
            && slot.chars().all(|ch| ch.is_ascii_digit())
            && slot.parse::<u64>().is_ok_and(|slot| slot > 0)
    })
}

#[cfg(test)]
#[path = "../../tests/unit/assembly/nas.rs"]
mod tests;
