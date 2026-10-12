use crate::assembly::server::ServerState;
use crate::services::discovery::nearby_devices::Context;
use anyhow::{Result, ensure};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use plain_rs::protocols::mdns::host_responder;
use plain_rs::protocols::mdns::service_browser::MdnsServiceBrowser;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex, RwLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
const WS_MDNS_UPDATED: &'static str = "MDNS_UPDATED";
static OWNER: tokio::sync::Mutex<Option<String>> = tokio::sync::Mutex::const_new(None);
struct Session {
    browser: MdnsServiceBrowser,
    /// macOS only. The system DNS-SD browser sees devices the sandboxed
    /// browser cannot, which is what nearby discovery is for.
    #[cfg(target_os = "macos")]
    system_browser: crate::services::discovery::local::macos_dns_sd::MacDnsSdBrowser,
    hostname: Arc<RwLock<String>>,
    scanning: Arc<AtomicBool>,
    context: Context,
    published: bool,
    manual_scanning: bool,
    manual_capture: bool,
    debug_sessions: std::collections::HashSet<uuid::Uuid>,
}
impl Session {
    fn set_scanning(&self, active: bool) -> Result<bool> {
        if self.scanning.load(Ordering::SeqCst) == active {
            return Ok(false);
        }
        self.context.devices.scanning_modes(
            Some(active),
            None,
            if active {
                plain_rs::db::chat_store::nearby::all(&self.context.db)?
            } else {
                vec![]
            },
        )?;
        let previous = self.scanning.swap(active, Ordering::SeqCst);
        if previous == active {
            return Ok(false);
        }
        if active {
            self.browser.start();
        } else {
            self.browser.stop();
        }
        let _ = self
            .context
            .events
            .send(plain_rs::events::WsEvent::broadcast(
                if active {
                    "NEARBY_DISCOVERY_STARTED"
                } else {
                    "NEARBY_DISCOVERY_STOPPED"
                },
                "{}".into(),
            ));
        Ok(active)
    }
}
pub(crate) struct Runtime {
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
pub(crate) enum Request {
    Receiver {},
    Start {},
    Stop {},
    Browse {},
    Restart {},
    Publish {},
    Update {},
    Unpublish {},
    Snapshot {},
    Capture { enabled: bool },
    DebugStart { token: String },
    DebugStop { token: String },
}
impl Runtime {
    pub(crate) async fn update_hostname(self: &Arc<Self>, state: &ServerState) -> Result<()> {
        let hostname = state
            .prefs
            .get::<String>(plain_rs::prefs::system::keys::MDNS_HOSTNAME)?
            .ok_or_else(|| anyhow::anyhow!("Missing hostname"))?;
        {
            let guard = self.session.lock().unwrap();
            let Some(session) = guard.as_ref() else {
                return Ok(());
            };
            let mut current = session.hostname.write().unwrap();
            if *current != hostname {
                *current = hostname;
                session.browser.clear_instances();
                session.browser.send_ptr_query();
            }
        }
        self.execute(state, Request::Update {}).await?;
        Ok(())
    }

    fn emit(&self, context: &Context, peer: Option<&str>, online: bool) {
        let revision = self.revision.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = context.events.send(plain_rs::events::WsEvent::broadcast(
            WS_MDNS_UPDATED,
            json!({"runtimeId":self.id,"revision":revision,"peerId":peer,"online":online})
                .to_string(),
        ));
    }
    pub(crate) fn browse_resident(&self) {
        if let Some(session) = self.session.lock().unwrap().as_ref() {
            session.browser.send_ptr_query();
        }
    }
    /// Typed browser snapshot for the shell's mDNS debug surface. The JSON
    /// snapshot carries the same rows, but a shell rendering a service table
    /// should not have to re-project them out of a dynamic value.
    /// The union the shell's mDNS debug page renders: the sandboxed browser
    /// plus, on macOS, the system one. A device either browser saw has to
    /// appear here or the page under-reports what discovery is doing.
    pub(crate) fn browser_snapshot(
        &self,
    ) -> Vec<plain_rs::protocols::mdns::service_browser::MdnsServiceSnapshot> {
        let guard = self.session.lock().unwrap();
        let Some(session) = guard.as_ref() else {
            return Vec::new();
        };
        let mut values = session.browser.snapshot();
        #[cfg(target_os = "macos")]
        values.extend(session.system_browser.snapshot());
        values
    }

    pub(crate) fn browser_activity(&self) -> Vec<crate::services::discovery::local::MdnsActivity> {
        let guard = self.session.lock().unwrap();
        let Some(session) = guard.as_ref() else {
            return Vec::new();
        };
        #[cfg(target_os = "macos")]
        {
            session.system_browser.activity()
        }
        #[cfg(not(target_os = "macos"))]
        {
            Vec::new()
        }
    }

    /// Whether the manual scan loop the discovery UI drives is running. This
    /// is the value behind the `isDiscovering` query, not the host
    /// responder's own running flag.
    pub(crate) fn manual_scanning(&self) -> bool {
        self.session
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|session| session.manual_scanning)
    }

    pub(crate) fn snapshot(&self) -> Value {
        let guard = self.session.lock().unwrap();
        let session = guard.as_ref();
        let (packets_in, packets_out) = if session.is_some() {
            plain_rs::protocols::mdns::packet_capture::snapshot()
        } else {
            (vec![], vec![])
        };
        json!({"runtimeId":self.id,"packetsIn":packets_in,"packetsOut":packets_out,"revision":self.revision.load(Ordering::SeqCst),"receiver":session.is_some(),"running":session.is_some() && host_responder::is_running(),"scanning":session.is_some_and(|s|s.scanning.load(Ordering::SeqCst)),"published":session.is_some_and(|s|s.published),"manualScanning":session.is_some_and(|s|s.manual_scanning),"debugSessionCount":session.map(|s|s.debug_sessions.len()).unwrap_or(0),"capturing":session.is_some_and(|s|s.manual_capture || !s.debug_sessions.is_empty()),"services":session.map(|s|s.browser.snapshot()).unwrap_or_default()})
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
                    .call("mdnsMulticast", json!({"acquire":true,"lease":self.id}))
                    .await
                    .map_err(anyhow::Error::msg)?
                    .as_bool()
                    == Some(true),
                "Multicast permission unavailable"
            );
            return Ok(());
        }
        let id = state
            .prefs
            .get::<String>(plain_rs::prefs::system::keys::CLIENT_ID)?
            .unwrap_or_default();
        ensure!(!id.is_empty(), "Missing client identity");
        let hostname = Arc::new(RwLock::new(
            state
                .prefs
                .get::<String>(plain_rs::prefs::system::keys::MDNS_HOSTNAME)?
                .unwrap_or_else(|| "plainapp.local".into()),
        ));
        let scanning = Arc::new(AtomicBool::new(false));
        let context = Context::from(state);
        let weak = Arc::downgrade(self);
        let observer = context.clone();
        let status = state.peer_status.clone();
        let status_db = state.db.clone();
        let status_events = state.events.clone();
        let visible = scanning.clone();
        let system_observer = observer.clone();
        let system_visible = visible.clone();
        let system_weak = weak.clone();
        let system_events = status_events.clone();
        let system_status = status.clone();
        let system_db = status_db.clone();
        let browser = MdnsServiceBrowser::new(id, hostname.clone(), move |found| {
            let Some(runtime) = weak.upgrade() else {
                return;
            };
            let guard = runtime.session.lock().unwrap();
            if guard.is_none() {
                return;
            }
            let id = found.id.clone();
            let device = plain_rs::discovery::nearby_devices::Device {
                id: found.id,
                name: found.name,
                ips: found.ips.into_iter().chain(found.ipv6).collect(),
                port: found.port,
                device_type: serde_json::from_value(json!(found.device_type))
                    .unwrap_or(plain_rs::chat::enums::DeviceType::Other)
                    .as_str()
                    .into(),
                version: found.version,
                platform: found.platform,
                last_seen: plain_rs::db::now_iso(),
                discovery_methods: vec!["LAN".into()],
            };
            match observer.seen(device, visible.load(Ordering::SeqCst), true) {
                Ok(true) => {
                    if visible.load(Ordering::SeqCst)
                        && status.connections.hint(&status_db, &id).unwrap_or(false)
                    {
                        let _ = status_events.send(plain_rs::events::WsEvent::broadcast(
                            plain_rs::chat::events::WS_PEER_STATUS_UPDATED,
                            json!({"id":id,"online":true}).to_string(),
                        ));
                        let _=status_events.send(plain_rs::events::WsEvent::broadcast("PEER_CONNECTIONS_UPDATED",json!({"runtimeId":status.connections.snapshot(&status_db)["runtimeId"]}).to_string()));
                    }
                    runtime.emit(&observer, Some(&id), visible.load(Ordering::SeqCst));
                }
                Ok(false) => {}
                Err(error) => log::warn!("mDNS observation: {error}"),
            }
        });
        let addrs = plain_rs::db::chat_store::peers::all(&state.db)?
            .into_iter()
            .flat_map(|p| p.ip.split(',').map(str::to_owned).collect::<Vec<_>>())
            .take(512)
            .collect::<Vec<_>>();
        *owner = Some(self.id.clone());
        ensure!(
            state
                .host
                .call("mdnsMulticast", json!({"acquire":true,"lease":self.id}))
                .await
                .map_err(anyhow::Error::msg)?
                .as_bool()
                == Some(true),
            "Multicast permission unavailable"
        );
        browser.seed_known_addrs(&addrs);
        browser.install_listener();
        // The system browser reports the same device shape as the sandboxed
        // one, so it feeds the same observer and the same event stream.
        #[cfg(target_os = "macos")]
        let system_browser = {
            let system_browser =
                crate::services::discovery::local::macos_dns_sd::MacDnsSdBrowser::new(
                    move |found| {
                        let Some(runtime) = system_weak.upgrade() else {
                            return;
                        };
                        if runtime.session.lock().unwrap().is_none() {
                            return;
                        }
                        let id = found.id.clone();
                        let device = plain_rs::discovery::nearby_devices::Device {
                            id: found.id,
                            name: found.name,
                            ips: found.ips,
                            port: found.port,
                            device_type: serde_json::from_value(json!(found.device_type))
                                .unwrap_or(plain_rs::chat::enums::DeviceType::Other)
                                .as_str()
                                .into(),
                            version: found.version,
                            platform: found.platform,
                            last_seen: plain_rs::db::now_iso(),
                            discovery_methods: vec!["DNS_SD".into()],
                        };
                        match system_observer.seen(
                            device,
                            system_visible.load(Ordering::SeqCst),
                            true,
                        ) {
                            Ok(true) => {
                                if system_visible.load(Ordering::SeqCst)
                                    && system_status
                                        .connections
                                        .hint(&system_db, &id)
                                        .unwrap_or(false)
                                {
                                    let _ =
                                        system_events.send(plain_rs::events::WsEvent::broadcast(
                                            plain_rs::chat::events::WS_PEER_STATUS_UPDATED,
                                            json!({"id":id,"online":true}).to_string(),
                                        ));
                                }
                                runtime.emit(
                                    &system_observer,
                                    Some(&id),
                                    system_visible.load(Ordering::SeqCst),
                                );
                            }
                            Ok(false) => {}
                            Err(error) => log::warn!("system DNS-SD observation: {error}"),
                        }
                    },
                );
            system_browser.start();
            system_browser
        };
        *self.session.lock().unwrap() = Some(Session {
            browser,
            #[cfg(target_os = "macos")]
            system_browser,
            hostname: hostname.clone(),
            scanning,
            context,
            published: false,
            manual_scanning: false,
            manual_capture: false,
            debug_sessions: std::collections::HashSet::new(),
        });
        *owner = Some(self.id.clone());
        host_responder::ensure_started(&hostname.read().unwrap());
        Ok(())
    }
    pub(crate) async fn execute(
        self: &Arc<Self>,
        state: &ServerState,
        request: Request,
    ) -> Result<Value> {
        if matches!(request, Request::Snapshot {}) {
            return Ok(self.snapshot());
        }
        let _operation = self.operation.lock().await;
        if matches!(
            request,
            Request::Stop {}
                | Request::Unpublish {}
                | Request::Restart {}
                | Request::Update {}
                | Request::Capture { enabled: false }
                | Request::DebugStop { .. }
        ) && self.session.lock().unwrap().is_none()
        {
            return Ok(self.snapshot());
        }
        if matches!(request, Request::Update {})
            && !self
                .session
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|session| session.published)
        {
            return Ok(self.snapshot());
        }
        if !matches!(
            request,
            Request::Stop {}
                | Request::Unpublish {}
                | Request::DebugStop { .. }
                | Request::Capture { enabled: false }
        ) {
            self.initialize(state).await?;
        }
        let service = if matches!(request, Request::Publish {} | Request::Update {}) {
            Some(serde_json::from_value::<
                plain_rs::protocols::mdns::service_info::MdnsServiceInfo,
            >(
                crate::services::discovery::discovery_advertisement::execute(
                    state,
                    crate::services::discovery::discovery_advertisement::Request::Mdns {},
                )
                .await?,
            )?)
        } else {
            None
        };
        let mut started = false;
        {
            let mut guard = self.session.lock().unwrap();
            let session = guard.as_mut().unwrap();
            match request {
                Request::Receiver {} => {}
                Request::Capture { enabled } => {
                    session.manual_capture = enabled;
                    plain_rs::protocols::mdns::packet_capture::set_enabled(
                        enabled || !session.debug_sessions.is_empty(),
                    );
                }
                Request::DebugStart { token } => {
                    let token = uuid::Uuid::parse_str(&token)?;
                    ensure!(
                        session.debug_sessions.contains(&token)
                            || session.debug_sessions.len() < 16,
                        "Too many mDNS debug sessions"
                    );
                    started = session.set_scanning(true)?;
                    session.debug_sessions.insert(token);
                    plain_rs::protocols::mdns::packet_capture::set_enabled(true);
                }
                Request::DebugStop { token } => {
                    let token = uuid::Uuid::parse_str(&token)?;
                    session.debug_sessions.remove(&token);
                    plain_rs::protocols::mdns::packet_capture::set_enabled(
                        session.manual_capture || !session.debug_sessions.is_empty(),
                    );
                    session.set_scanning(
                        session.manual_scanning || !session.debug_sessions.is_empty(),
                    )?;
                }
                Request::Start {} => {
                    started = session.set_scanning(true)?;
                    session.manual_scanning = true;
                }
                Request::Stop {} => {
                    session.manual_scanning = false;
                    session.set_scanning(!session.debug_sessions.is_empty())?;
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
            self.emit(&session.context, None, false);
        }
        let mut snapshot = self.snapshot();
        snapshot["started"] = json!(started);
        Ok(snapshot)
    }
    pub(crate) async fn host_connected(state: ServerState, generation: u64) {
        if state.mdns.session.lock().unwrap().is_none() {
            return;
        }
        let mut stop = state.stop.clone();
        for _ in 0..8 {
            if *stop.borrow() || !state.host.is_current(generation) {
                return;
            }
            let result = tokio::select! {
                _ = stop.changed() => return,
                result = state.mdns.execute(&state, Request::Receiver {}) => result,
            };
            match result {
                Ok(_) => return,
                Err(error) if error.to_string().contains("capacity exceeded") => {
                    tokio::select! { _ = stop.changed() => return, _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {} }
                }
                Err(error) => {
                    log::warn!("mDNS multicast reacquire: {error}");
                    return;
                }
            }
        }
    }
    pub(crate) async fn close(&self, host: &crate::host::bridge::Host) {
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
            plain_rs::protocols::mdns::packet_capture::set_enabled(false);
            let _ = host
                .call("mdnsMulticast", json!({"acquire":false,"lease":self.id}))
                .await;
            *owner = None;
        }
    }
}
pub(crate) async fn call(
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
#[path = "../../../tests/unit/services/discovery/mdns_runtime.rs"]
mod tests;
