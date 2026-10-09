pub mod bind;

use axum::{Extension, Router};

#[derive(Clone)]
pub struct ConnectionScheme(pub &'static str);
use axum_server::{Handle, tls_rustls::RustlsConfig};
use std::{net::SocketAddr, time::Duration};
use tokio::{sync::watch, task::JoinHandle};

pub struct HttpListeners {
    pub http_port: u16,
    pub https_port: u16,
    stop: watch::Sender<bool>,
    tls_handle: Handle,
    tasks: Vec<JoinHandle<()>>,
    failure: watch::Receiver<Option<String>>,
}

impl HttpListeners {
    pub async fn start(
        router: Router,
        http_port: u16,
        https_port: u16,
        certificate_pem: Vec<u8>,
        private_key_pem: Vec<u8>,
    ) -> Result<Self, String> {
        let tls = tls_config(&certificate_pem, &private_key_pem)?;
        Self::start_with_tls(router, http_port, https_port, tls).await
    }

    pub async fn start_with_tls(
        router: Router,
        http_port: u16,
        https_port: u16,
        tls: RustlsConfig,
    ) -> Result<Self, String> {
        let router = router.route(
            "/__plain_server_ready",
            axum::routing::get(|| async { axum::http::StatusCode::NO_CONTENT }),
        );
        let http = bind::bind_listener_fallback(http_port, &bind::HTTP_PORTS)
            .map_err(|e| format!("HTTP bind failed: {e}"))?;
        let https = bind::bind_listener_fallback(https_port, &bind::HTTPS_PORTS)
            .map_err(|e| format!("HTTPS bind failed: {e}"))?;
        let http_port = http.local_addr().map_err(|e| e.to_string())?.port();
        let https_port = https.local_addr().map_err(|e| e.to_string())?.port();
        http.set_nonblocking(true).map_err(|e| e.to_string())?;
        https.set_nonblocking(true).map_err(|e| e.to_string())?;
        let http = tokio::net::TcpListener::from_std(http).map_err(|e| e.to_string())?;
        let (stop, mut receiver) = watch::channel(false);
        let app = router
            .clone()
            .layer(Extension(ConnectionScheme("http")))
            .into_make_service_with_connect_info::<SocketAddr>();
        let (failure_sender, failure) = watch::channel(None);
        let http_failure = failure_sender.clone();
        let http_task = tokio::spawn(async move {
            let mut task_guard = ListenerTaskGuard::new("HTTP", http_failure.clone());
            if let Err(error) = axum::serve(http, app)
                .with_graceful_shutdown(async move {
                    let _ = receiver.changed().await;
                })
                .await
            {
                let _ = http_failure.send(Some(format!("HTTP serve failed: {error}")));
            }
            task_guard.completed = true;
        });
        let tls_handle = Handle::new();
        let handle = tls_handle.clone();
        let tls_task = tokio::spawn(async move {
            let mut task_guard = ListenerTaskGuard::new("HTTPS", failure_sender.clone());
            if let Err(error) = axum_server::from_tcp_rustls(https, tls)
                .handle(handle)
                .serve(
                    router
                        .layer(Extension(ConnectionScheme("https")))
                        .into_make_service_with_connect_info::<SocketAddr>(),
                )
                .await
            {
                let _ = failure_sender.send(Some(format!("HTTPS serve failed: {error}")));
            }
            task_guard.completed = true;
        });
        let listeners = Self {
            http_port,
            https_port,
            stop,
            tls_handle,
            tasks: vec![http_task, tls_task],
            failure,
        };
        if let Err(error) = listeners.check_health().await {
            listeners.shutdown().await;
            return Err(error);
        }
        Ok(listeners)
    }

    pub fn failures(&self) -> watch::Receiver<Option<String>> {
        self.failure.clone()
    }

    pub async fn check_health(&self) -> Result<(), String> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_millis(500))
            .build()
            .map_err(|e| e.to_string())?;
        tokio::time::timeout(Duration::from_millis(8_500), async {
            loop {
                if let Some(error) = self.failure.borrow().clone() {
                    return Err(error);
                }
                let mut healthy = true;
                for (scheme, port) in [("http", self.http_port), ("https", self.https_port)] {
                    match client
                        .get(format!("{scheme}://127.0.0.1:{port}/__plain_server_ready"))
                        .send()
                        .await
                    {
                        Ok(response) if response.status() == reqwest::StatusCode::NO_CONTENT => {}
                        _ => {
                            healthy = false;
                            break;
                        }
                    }
                }
                if healthy {
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| "HTTP/HTTPS server readiness timed out".to_string())?
    }

    pub async fn shutdown(mut self) {
        let _ = self.stop.send(true);
        self.tls_handle
            .graceful_shutdown(Some(Duration::from_secs(1)));
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        for task in &mut self.tasks {
            if tokio::time::timeout_at(deadline, &mut *task).await.is_err() {
                task.abort();
                let _ = task.await;
            }
        }
    }
}

impl Drop for HttpListeners {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
        self.tls_handle.shutdown();
        for task in &self.tasks {
            task.abort();
        }
    }
}

struct ListenerTaskGuard {
    scheme: &'static str,
    failure: watch::Sender<Option<String>>,
    completed: bool,
}

impl ListenerTaskGuard {
    fn new(scheme: &'static str, failure: watch::Sender<Option<String>>) -> Self {
        Self {
            scheme,
            failure,
            completed: false,
        }
    }
}

impl Drop for ListenerTaskGuard {
    fn drop(&mut self) {
        if !self.completed {
            let _ = self.failure.send(Some(format!(
                "{} listener task terminated unexpectedly",
                self.scheme
            )));
        }
    }
}

pub fn tls_config(certificate_pem: &[u8], private_key_pem: &[u8]) -> Result<RustlsConfig, String> {
    let certificates = rustls_pemfile::certs(&mut std::io::Cursor::new(certificate_pem))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let key = rustls_pemfile::private_key(&mut std::io::Cursor::new(private_key_pem))
        .map_err(|e| e.to_string())?
        .ok_or("TLS private key is missing")?;
    let mut config = rustls::ServerConfig::builder_with_provider(std::sync::Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| e.to_string())?
    .with_no_client_auth()
    .with_single_cert(certificates, key)
    .map_err(|e| e.to_string())?;
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(RustlsConfig::from_config(std::sync::Arc::new(config)))
}

#[cfg(test)]
#[path = "../../tests/unit/http_transport/mod.rs"]
mod tests;
