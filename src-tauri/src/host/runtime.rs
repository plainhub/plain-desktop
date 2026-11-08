use std::sync::Arc;

use plain_server::server::ContentServer;
use serde_json::Value;
use tauri::{Emitter, Manager};

pub struct Runtime {
    core: Arc<ContentServer>,
    app: tauri::AppHandle,
    capabilities: Vec<String>,
    multicast: std::sync::Mutex<plain_rs::infrastructure::host::multicast::MulticastLeases>,
}

impl Runtime {
    pub fn new(core: Arc<ContentServer>, app: tauri::AppHandle, capabilities: Vec<String>) -> Self {
        Self {
            core,
            app,
            capabilities,
            multicast: Default::default(),
        }
    }

    async fn dispatch(&self, method: &str, params: Value) -> Result<Value, String> {
        match method {
            "systemWebLoginRequested"
            | "systemWebLoginClosed"
            | "systemWebLoginSnapshot"
            | "peerNotificationsChanged"
            | "peerClipboardChanged"
            | "networkInterfacesChanged"
            | "httpServerStatusChanged"
            | "onlineClientsChanged" => {
                self.app
                    .state::<crate::commands::native::callbacks::HostCallbacks>()
                    .deliver(method, &params)?;
                Ok(Value::Null)
            }
            "systemWebLoginCompleted"
            | "clipboardHistoryRefresh"
            | "dlnaRendererChanged"
            | "dlnaRendererRefresh"
            | "sharedFolderDownloadsRefresh"
            | "peerTransportsChanged"
            | "appUpdateDownloadChanged"
            | "dlnaSenderChanged"
            | "feedSyncChanged"
            | "audioPlaybackRefresh"
            | "audioLibraryRefresh"
            | "pairingSucceeded" => Ok(Value::Null),
            "prefsChanged" => {
                self.app
                    .emit("prefsChanged", ())
                    .map_err(|error| error.to_string())?;
                Ok(Value::Null)
            }
            "systemCapabilities" => {
                serde_json::to_value(&self.capabilities).map_err(|error| error.to_string())
            }
            "peerTransportCapabilities" => Ok(serde_json::json!([])),
            "systemSetClipboard" => {
                let request: ClipboardText =
                    serde_json::from_value(params).map_err(|error| error.to_string())?;
                let app = self.app.clone();
                tokio::task::spawn_blocking(move || super::clipboard::write_text(&app, &request.text))
                    .await
                    .map_err(|error| error.to_string())??;
                Ok(Value::Bool(true))
            }
            "systemRequestAccess" => Ok(serde_json::json!(false)),
            "systemAccessFacts" => plain_rs::infrastructure::host::access_facts::facts(
                &plain_rs::features::access::AccessFeature::FILE_FEATURES,
                params,
            ),
            method if plain_server::native::audio::handles(method) => {
                self.core.native_audio()?.handle(method, params).await
            }

            "mdnsMulticast" => {
                let result = self
                    .multicast
                    .lock()
                    .map_err(|_| "multicast lease state unavailable")?
                    .without_platform_lock(params)?;
                Ok(Value::Bool(result))
            }

            _ => Err(format!("Unsupported desktop host method: {method}")),
        }
    }

    pub fn start(self: Arc<Self>) -> Result<tokio::task::JoinHandle<Result<(), String>>, String> {
        let session = Arc::new(self.core.host_events()?);
        let changes = Arc::new(tokio::sync::Notify::new());
        let receiver = changes.clone();
        let observer = session.clone();
        self.core
            .shared_preferences()?
            .set_change_hook(Some(Arc::new({
                let changes = changes.clone();
                move || changes.notify_one()
            })));
        changes.notify_one();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    _ = observer.wait_disconnected() => return,
                    _ = receiver.notified() => {
                        if let Err(error) = observer.prefs_changed().await {
                            log::error!("Host preferences update failed: {error}");
                        }
                    },
                }
            }
        });
        Ok(tokio::spawn(self.run(session)))
    }

    async fn run(
        self: Arc<Self>,
        session: Arc<plain_server::host::events::HostEvents>,
    ) -> Result<(), String> {
        let _multicast =
            plain_rs::infrastructure::host::multicast::MulticastSession::new(&self.multicast);
        let (running, stopped) = tokio::sync::watch::channel(true);
        let requests = self.clone();
        let serving_session = session.clone();
        let serving = async {
            let result = serving_session
                .serve(move |method, params| {
                    let requests = requests.clone();
                    async move { requests.dispatch(&method, params).await }
                })
                .await;
            running.send_replace(false);
            result
        };
        let observing = super::clipboard::observe(
            stopped,
            || {
                let app = self.app.clone();
                let prefs = self.core.shared_preferences();
                let supported = self.capabilities.iter().any(|name| name == "CLIPBOARD");
                async move {
                    let permissions: Vec<String> = prefs?.get_or(
                        plain_rs::features::prefs::pref_keys::API_PERMISSIONS,
                        Vec::new(),
                    );
                    if !supported || !permissions.iter().any(|name| name == "CLIPBOARD") {
                        return Ok(None);
                    }
                    tokio::task::spawn_blocking(move || {
                        match super::clipboard::read(&app) {
                            Ok(sample) => Ok(sample),
                            Err(error) => {
                                log::debug!("clipboard text unavailable: {error}");
                                Ok(None)
                            }
                        }
                    })
                    .await
                    .map_err(|error| error.to_string())?
                }
            },
            |sample| {
                let session = session.clone();
                async move {
                    session
                        .publish_text(
                            "CLIPBOARD_REPORTED",
                            serde_json::to_string(&sample).map_err(|error| error.to_string())?,
                        )
                        .await
                }
            },
        );
        let (result, ()) = tokio::join!(serving, observing);
        result
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ClipboardText {
    text: String,
}
