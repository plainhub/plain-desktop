use super::{nearby_devices::Context, server::ServerState};
use crate::mdns::{host_responder, service_browser::MdnsServiceBrowser};
use anyhow::{Result, ensure};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex, RwLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
const WS_MDNS_UPDATED: i32 = 10001;
static OWNER: tokio::sync::Mutex<Option<String>> = tokio::sync::Mutex::const_new(None);
struct Session {
    browser: MdnsServiceBrowser,
    hostname: Arc<RwLock<String>>,
    scanning: Arc<AtomicBool>,
    context: Context,
    published: bool,
}
pub(super) struct Runtime {
    id: String,
    session: Mutex<Option<Session>>,
    operation: tokio::sync::Mutex<()>,
    revision: AtomicU64,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            session: Mutex::new(None),
            operation: tokio::sync::Mutex::new(()),
            revision: AtomicU64::new(0),
        }
    }
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Request {
    Receiver {},
    Start {},
    Stop {},
    Browse {},
    Restart {},
    Publish {},
    Update {},
    Unpublish {},
    Snapshot {},
}
impl Runtime {
    fn emit(&self, context: &Context, peer: Option<&str>) {
        let revision = self.revision.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = context.events.send(crate::ws_event::WsEvent::broadcast(
            WS_MDNS_UPDATED,
            json!({"revision":revision,"peerId":peer}).to_string(),
        ));
    }
    pub(super) fn snapshot(&self) -> Value {
        let guard = self.session.lock().unwrap();
        let session = guard.as_ref();
        json!({"revision":self.revision.load(Ordering::SeqCst),"receiver":session.is_some(),"running":session.is_some() && host_responder::is_running(),"scanning":session.is_some_and(|s|s.scanning.load(Ordering::SeqCst)),"published":session.is_some_and(|s|s.published),"services":session.map(|s|s.browser.snapshot()).unwrap_or_default()})
    }
    async fn initialize(self: &Arc<Self>, state: &ServerState) -> Result<()> {
        let mut owner = OWNER.lock().await;
        ensure!(
            owner.as_ref().is_none_or(|id| id == &self.id),
            "mDNS is owned by another runtime"
        );
        if self.session.lock().unwrap().is_some() {
            ensure!(
                state
                    .host
                    .call("mdnsMulticast", json!({"acquire":true}))
                    .await
                    .map_err(anyhow::Error::msg)?
                    .as_bool()
                    == Some(true),
                "Multicast permission unavailable"
            );
            return Ok(());
        }
        let id = state.prefs.get::<String>("client_id")?.unwrap_or_default();
        ensure!(!id.is_empty(), "Missing client identity");
        let hostname = Arc::new(RwLock::new(
            state
                .prefs
                .get::<String>("mdns_hostname")?
                .unwrap_or_else(|| "plainapp.local".into()),
        ));
        let scanning = Arc::new(AtomicBool::new(false));
        let context = Context::from(state);
        let weak = Arc::downgrade(self);
        let observer = context.clone();
        let visible = scanning.clone();
        let browser = MdnsServiceBrowser::new(id, hostname.clone(), move |found| {
            let Some(runtime) = weak.upgrade() else {
                return;
            };
            let guard = runtime.session.lock().unwrap();
            if guard.is_none() {
                return;
            }
            let id = found.id.clone();
            let device = crate::chat::nearby_devices::Device {
                id: found.id,
                name: found.name,
                ips: found.ips.into_iter().chain(found.ipv6).collect(),
                port: found.port,
                device_type: serde_json::from_value(json!(found.device_type))
                    .unwrap_or(crate::chat::enums::DeviceType::Other)
                    .as_str()
                    .into(),
                version: found.version,
                platform: found.platform,
                last_seen: crate::db::now_iso(),
                discovery_methods: vec!["LAN".into()],
            };
            match observer.seen(device, visible.load(Ordering::SeqCst), true) {
                Ok(true) => runtime.emit(&observer, Some(&id)),
                Ok(false) => {}
                Err(error) => log::warn!("mDNS observation: {error}"),
            }
        });
        let addrs = crate::db::chat_store::peers::all(&state.db)?
            .into_iter()
            .flat_map(|p| p.ip.split(',').map(str::to_owned).collect::<Vec<_>>())
            .take(512)
            .collect::<Vec<_>>();
        *owner = Some(self.id.clone());
        ensure!(
            state
                .host
                .call("mdnsMulticast", json!({"acquire":true}))
                .await
                .map_err(anyhow::Error::msg)?
                .as_bool()
                == Some(true),
            "Multicast permission unavailable"
        );
        browser.seed_known_addrs(&addrs);
        browser.install_listener();
        *self.session.lock().unwrap() = Some(Session {
            browser,
            hostname: hostname.clone(),
            scanning,
            context,
            published: false,
        });
        *owner = Some(self.id.clone());
        host_responder::ensure_started(&hostname.read().unwrap());
        Ok(())
    }
    pub(super) async fn execute(
        self: &Arc<Self>,
        state: &ServerState,
        request: Request,
    ) -> Result<Value> {
        if matches!(request, Request::Snapshot {}) {
            return Ok(self.snapshot());
        }
        let _operation = self.operation.lock().await;
        if matches!(request, Request::Stop {} | Request::Unpublish {})
            && self.session.lock().unwrap().is_none()
        {
            return Ok(self.snapshot());
        }
        self.initialize(state).await?;
        let service = if matches!(request, Request::Publish {} | Request::Update {}) {
            Some(serde_json::from_value::<
                crate::mdns::service_info::MdnsServiceInfo,
            >(
                super::discovery_advertisement::execute(
                    state,
                    super::discovery_advertisement::Request::Mdns {},
                )
                .await?,
            )?)
        } else {
            None
        };
        {
            let mut guard = self.session.lock().unwrap();
            let session = guard.as_mut().unwrap();
            match request {
                Request::Receiver {} => {}
                Request::Start {} => {
                    session.context.devices.scanning_modes(
                        Some(true),
                        None,
                        crate::db::chat_store::nearby::all(&state.db)?,
                    )?;
                    if !session.scanning.swap(true, Ordering::SeqCst) {
                        session.browser.start();
                        let _ = session
                            .context
                            .events
                            .send(crate::ws_event::WsEvent::broadcast(29, "{}".into()));
                    }
                }
                Request::Stop {} => {
                    session
                        .context
                        .devices
                        .scanning_modes(Some(false), None, vec![])?;
                    if session.scanning.swap(false, Ordering::SeqCst) {
                        session.browser.stop();
                        let _ = session
                            .context
                            .events
                            .send(crate::ws_event::WsEvent::broadcast(30, "{}".into()));
                    }
                }
                Request::Browse {} => session.browser.send_ptr_query(),
                Request::Restart {} => {
                    host_responder::restart_socket();
                    host_responder::ensure_ipv6_started();
                }
                Request::Publish {} => {
                    let service = service.unwrap();
                    *session.hostname.write().unwrap() = service.target_hostname.clone();
                    host_responder::clear_service();
                    host_responder::start(&service.target_hostname, Some(service.clone()));
                    session.published = true;
                }
                Request::Update {} => {
                    let service = service.unwrap();
                    if session.published {
                        *session.hostname.write().unwrap() = service.target_hostname.clone();
                        host_responder::update_service(service);
                    }
                }
                Request::Unpublish {} => {
                    host_responder::clear_service();
                    session.published = false;
                }
                Request::Snapshot {} => unreachable!(),
            }
            self.emit(&session.context, None);
        }
        Ok(self.snapshot())
    }
    pub(super) async fn close(&self, host: &super::host::Host) {
        let _operation = self.operation.lock().await;
        let mut owner = OWNER.lock().await;
        if let Some(session) = self.session.lock().unwrap().take() {
            session.scanning.store(false, Ordering::SeqCst);
            let _ = session
                .context
                .devices
                .scanning_modes(Some(false), None, vec![]);
            session.browser.shutdown();
        }
        if owner.as_ref() == Some(&self.id) {
            host_responder::stop();
            let _ = host.call("mdnsMulticast", json!({"acquire":false})).await;
            *owner = None;
        }
    }
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut stop = state.stop.clone();
    if *stop.borrow() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let result = tokio::select! {_=stop.changed()=>Err(anyhow::anyhow!("Server stopped")),result=state.mdns.execute(&state,request)=>result};
    match result {
        Ok(value) => Json(json!({"result":value})).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}

#[cfg(all(test, feature = "http_transport"))]
#[path = "../../tests/unit/content_api/mdns_runtime.rs"]
mod tests;
