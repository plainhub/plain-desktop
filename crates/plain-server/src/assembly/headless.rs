use super::server::ContentServer;
use crate::host::headless::runtime::Runtime;
use crate::platform::system::consts::AppPaths;
use anyhow::{Result, anyhow};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub(crate) mod storage;

#[derive(Default)]
pub struct Options {
    pub media_directories: Vec<PathBuf>,
    pub web_root: Option<PathBuf>,
    pub automount: bool,
}

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

pub struct HeadlessService {
    core: Arc<ContentServer>,
    host: tokio::task::JoinHandle<Result<(), String>>,
    #[cfg(target_os = "linux")]
    watcher: Option<plain_rs::platform::linux::mounts::automount::Watcher>,
}

impl HeadlessService {
    pub async fn start(paths: &AppPaths, options: Options) -> Result<Self> {
        storage::validate_automount(options.automount)?;
        initialize_tls();
        plain_media::storage::paths::set(paths.data_dir.clone(), paths.cache_dir.clone());
        let core = Arc::new(ContentServer::open(&paths.data_dir).map_err(anyhow::Error::msg)?);
        crate::platform::system::log::set_file(&crate::platform::system::log::default_log_file(
            &paths.data_dir,
        ));
        let runtime = Arc::new(Runtime::new(core.clone(), capabilities()));
        let host = tokio::spawn(runtime.run());
        let mut service = Self {
            core,
            host,
            #[cfg(target_os = "linux")]
            watcher: None,
        };
        if let Err(error) = service.start_services(paths, options).await {
            service.shutdown().await;
            return Err(error);
        }
        Ok(service)
    }

    async fn start_services(&mut self, paths: &AppPaths, options: Options) -> Result<()> {
        #[cfg(target_os = "linux")]
        if options.automount {
            let prefs = self.core.shared_preferences().map_err(anyhow::Error::msg)?;
            let events = storage::event_sink(&self.core, options.media_directories.is_empty())
                .map_err(anyhow::Error::msg)?;
            let mount_prefs = prefs.clone();
            let mount_events = events.clone();
            tokio::task::spawn_blocking(move || {
                plain_rs::platform::linux::mounts::automount::ensure_mounted_usb_volumes(
                    &mount_prefs,
                    &mount_events,
                )
            })
            .await??;
            self.watcher =
                Some(plain_rs::platform::linux::mounts::automount::Watcher::start(prefs, events));
        }
        let prefs = self.core.shared_preferences().map_err(anyhow::Error::msg)?;
        let roots = storage::source_directories(&prefs, &options.media_directories)?;
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
            .headless_state()
            .tls_identity
            .write()
            .map_err(|_| anyhow!("TLS identity configuration unavailable"))? =
            crate::http::tls_identity::IdentitySource::Pem {
                certificate: paths.tls_cert.clone(),
                key: paths.tls_key.clone(),
            };
        if let Some(root) = options.web_root.or_else(web_root) {
            anyhow::ensure!(
                root.is_dir(),
                "Web directory does not exist: {}",
                root.display()
            );
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
            storage::start_source_scan(&catalog).map_err(anyhow::Error::msg)?;
        }
        Ok(())
    }

    pub async fn wait(&mut self) -> Result<()> {
        #[cfg(target_os = "linux")]
        if let Some(watcher) = &mut self.watcher {
            return tokio::select! {
                result = &mut self.host => {
                    result.map_err(anyhow::Error::from)?.map_err(anyhow::Error::msg)?;
                    Err(anyhow!("Headless host session stopped"))
                }
                result = watcher.wait() => {
                    result?;
                    Err(anyhow!("Mount watcher stopped"))
                }
            };
        }
        (&mut self.host).await?.map_err(anyhow::Error::msg)?;
        Err(anyhow!("Headless host session stopped"))
    }

    pub async fn shutdown(self) {
        #[cfg(target_os = "linux")]
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

fn capabilities() -> Vec<String> {
    let mut capabilities = vec!["MEDIA_SCAN".into()];
    #[cfg(target_os = "linux")]
    {
        if !plain_rs::platform::linux::samba::detect_systemd_service_name().is_empty() {
            capabilities.push("LAN_SHARE".into());
        }
        if has_command("lsblk") {
            capabilities.push("DISK_MANAGER".into());
        }
    }
    if has_command("libreoffice") || has_command("soffice") {
        capabilities.push("DOC_PREVIEW".into());
    }
    capabilities
}

fn has_command(command: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|path| {
            let executable = path.join(command);
            executable.is_file() || cfg!(windows) && executable.with_extension("exe").is_file()
        })
    })
}

fn web_root() -> Option<PathBuf> {
    [
        "web/dist",
        "../web/dist",
        "../../web/dist",
        "src/web_dist",
        "crates/plain-cli/src/web_dist",
        "dist",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| path.is_dir())
}

#[cfg(test)]
#[path = "../../tests/unit/assembly/headless.rs"]
mod tests;
