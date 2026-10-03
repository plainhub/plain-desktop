use axum_server::tls_rustls::RustlsConfig;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use super::ServerState;
use crate::http_transport::HttpListeners;

pub struct ServerRuntime {
    state: ServerState,
    rustls: Option<RustlsConfig>,
    server: Mutex<Option<HttpListeners>>,
    rebind_lock: tokio::sync::Mutex<()>,
}

impl ServerRuntime {
    pub async fn start(state: ServerState) -> Self {
        let rustls = match crate::http_server::tls::ensure_cert(&state.ctx.data_dir) {
            Ok((cert_pem, key_pem)) => match crate::http_transport::tls_config(&cert_pem, &key_pem)
            {
                Ok(config) => Some(config),
                Err(error) => {
                    log::error!("local_server: failed to build rustls config: {error}");
                    None
                }
            },
            Err(error) => {
                log::error!("local_server: failed to ensure cert: {error}");
                None
            }
        };
        let runtime = Self {
            state,
            rustls,
            server: Mutex::new(None),
            rebind_lock: tokio::sync::Mutex::new(()),
        };
        runtime.rebind().await.expect("initial local server bind");
        runtime
    }

    async fn rebind(&self) -> Result<(), String> {
        let _guard = self.rebind_lock.lock().await;
        let ctx = &self.state.ctx;
        let old_http = ctx.port.load(Ordering::Relaxed);
        let old_https = ctx.https_port.load(Ordering::Relaxed);
        let http_port = crate::prefs::server::http_port(&ctx.prefs);
        let https_port = crate::prefs::server::https_port(&ctx.prefs);

        let tls = self
            .rustls
            .clone()
            .ok_or("HTTPS certificate is unavailable")?;
        let previous = self.server.lock().unwrap().take();
        if let Some(previous) = previous {
            previous.shutdown().await;
        }
        let listeners = match HttpListeners::start_with_tls(
            super::build_router(self.state.clone()),
            http_port,
            https_port,
            tls,
        )
        .await
        {
            Ok(listeners) => listeners,
            Err(error) => {
                if old_http != 0 {
                    crate::prefs::server::set_http_port(&ctx.prefs, old_http);
                }
                if old_https != 0 {
                    crate::prefs::server::set_https_port(&ctx.prefs, old_https);
                }
                return Err(error);
            }
        };
        let new_port = listeners.http_port;
        let new_https_port = listeners.https_port;
        ctx.port.store(new_port, Ordering::Relaxed);
        ctx.https_port.store(new_https_port, Ordering::Relaxed);
        *self.server.lock().unwrap() = Some(listeners);
        ctx.discover_manager.set_https_port(new_https_port);
        Ok(())
    }

    pub async fn restart(&self) -> Result<(), String> {
        self.rebind().await
    }

    pub fn port(&self) -> u16 {
        self.state.ctx.port.load(Ordering::Relaxed)
    }

    pub fn https_port(&self) -> u16 {
        self.state.ctx.https_port.load(Ordering::Relaxed)
    }

    pub fn token(&self) -> &str {
        &self.state.ctx.token
    }
}
