use super::{
    peer_status::{ConnectionGuard, emit},
    server::ServerState,
};
use crate::{
    chat::peer_status::handshake,
    db::{DPeer, chat_store::peers},
};
use anyhow::{Result, ensure};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::task::JoinHandle;

pub(super) struct Outgoing {
    state: Mutex<Work>,
    setup: Arc<tokio::sync::Semaphore>,
}
#[derive(Default)]
struct Work {
    epoch: u64,
    started: bool,
    coordinator: Option<JoinHandle<()>>,
    peers: HashMap<String, (String, JoinHandle<()>)>,
}
impl Outgoing {
    pub(super) fn new() -> Self {
        Self {
            state: Mutex::new(Work::default()),
            setup: Arc::new(tokio::sync::Semaphore::new(4)),
        }
    }
    pub(super) fn start(self: &Arc<Self>, state: ServerState) {
        let mut work = self.state.lock().unwrap();
        if work.started {
            return;
        }
        work.started = true;
        work.epoch = work.epoch.wrapping_add(1);
        let epoch = work.epoch;
        let this = self.clone();
        work.coordinator = Some(tokio::spawn(async move {
            let mut stop = state.stop.clone();
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            loop {
                tokio::select! {
                    _=stop.changed()=>break,
                    _=tick.tick()=>{ if !this.current(epoch) { break; } if let Err(error)=this.reconcile(&state,epoch) { log::warn!("Peer status reconcile: {error}"); } }
                }
            }
            drop(this.stop_jobs(Some(epoch)));
        }));
    }
    fn stop_jobs(&self, epoch: Option<u64>) -> Vec<JoinHandle<()>> {
        let mut work = self.state.lock().unwrap();
        if epoch.is_some_and(|epoch| epoch != work.epoch) {
            return vec![];
        }
        work.started = false;
        work.epoch = work.epoch.wrapping_add(1);
        let mut jobs: Vec<_> = work.peers.drain().map(|(_, (_, job))| job).collect();
        if let Some(job) = work.coordinator.take() {
            jobs.push(job);
        }
        for job in &jobs {
            job.abort();
        }
        jobs
    }
    pub(super) async fn stop(&self) {
        for job in self.stop_jobs(None) {
            let _ = job.await;
        }
    }
    fn current(&self, epoch: u64) -> bool {
        let work = self.state.lock().unwrap();
        work.started && work.epoch == epoch
    }
    pub(super) fn reconcile(self: &Arc<Self>, state: &ServerState, epoch: u64) -> Result<()> {
        let actor = state.prefs.get::<String>("client_id")?.unwrap_or_default();
        let wanted: HashMap<String, DPeer> = peers::all(&state.db)?
            .into_iter()
            .filter(|_| state.prefs.get_user_or("service", false))
            .filter(|p| p.is_paired() && !p.key.is_empty() && !actor.is_empty() && actor < p.id)
            .map(|p| (p.id.clone(), p))
            .collect();
        let mut work = self.state.lock().unwrap();
        if !work.started || work.epoch != epoch {
            return Ok(());
        }
        let removed: Vec<_> = work
            .peers
            .iter()
            .filter(|(id, (identity, task))| {
                task.is_finished()
                    || wanted
                        .get(*id)
                        .is_none_or(|peer| fingerprint(peer) != *identity)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in removed {
            if let Some((_, task)) = work.peers.remove(&id) {
                task.abort();
            }
        }
        for (id, peer) in wanted {
            if !work.peers.contains_key(&id) {
                let identity = fingerprint(&peer);
                let this = self.clone();
                let context = state.clone();
                work.peers.insert(
                    id.clone(),
                    (
                        identity,
                        tokio::spawn(async move {
                            this.worker(context, id, epoch).await;
                        }),
                    ),
                );
            }
        }
        Ok(())
    }
    pub(super) fn snapshot(&self) -> serde_json::Value {
        let work = self.state.lock().unwrap();
        json!({"started":work.started,"workers":work.peers.len(),"epoch":work.epoch})
    }
    pub(super) fn reconnect(self: &Arc<Self>, state: &ServerState) -> Result<()> {
        let active = state.peer_status.connections.active_peers(&state.db);
        let epoch = {
            let mut work = self.state.lock().unwrap();
            let reset: Vec<_> = work
                .peers
                .iter()
                .filter(|(id, (_, job))| job.is_finished() || !active.contains(*id))
                .map(|(id, _)| id.clone())
                .collect();
            for id in reset {
                if let Some((_, job)) = work.peers.remove(&id) {
                    job.abort();
                }
            }
            work.epoch
        };
        self.reconcile(state, epoch)
    }
    async fn worker(self: Arc<Self>, state: ServerState, id: String, epoch: u64) {
        let mut attempts = 0u32;
        while self.current(epoch) && state.prefs.get_user_or("service", false) {
            if attempts > 0 {
                tokio::time::sleep(Duration::from_millis(
                    (1000u64 << attempts.saturating_sub(1).min(6)).min(60_000),
                ))
                .await;
                state.mdns.browse_resident();
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            if !self.current(epoch) {
                break;
            }
            let Some(peer) = peers::get(&state.db, &id).ok().flatten() else {
                break;
            };
            let Ok(actor) = state.prefs.get::<String>("client_id") else {
                break;
            };
            if !peer.is_paired() || actor.as_ref().is_none_or(|actor| actor >= &id) {
                break;
            }
            match self.attempt(&state, &peer, epoch).await {
                Ok(()) => attempts = 0,
                Err(error) => {
                    log::debug!("Peer status connection: {error}");
                    attempts = attempts.saturating_add(1);
                }
            }
        }
    }
    async fn attempt(&self, state: &ServerState, peer: &DPeer, epoch: u64) -> Result<()> {
        ensure!(!peer.ip.is_empty() && peer.port > 0, "No peer address");
        let permit = self.setup.clone().acquire_owned().await?;
        ensure!(self.current(epoch), "Status runtime stopped");
        let actor = state
            .prefs
            .get::<String>("client_id")?
            .filter(|id| !id.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Missing identity"))?;
        let url = crate::utils::build_url::build_url(
            "wss",
            peer.best_ip(),
            peer.port,
            &format!("/status?cid={actor}"),
        );
        let mut socket =
            tokio::time::timeout(Duration::from_secs(10), super::status_socket::connect(&url))
                .await??;
        let current =
            peers::get(&state.db, &peer.id)?.ok_or_else(|| anyhow::anyhow!("Peer removed"))?;
        ensure!(
            current.is_paired()
                && fingerprint(&current) == fingerprint(peer)
                && current.ip == peer.ip
                && current.port == peer.port,
            "Peer changed"
        );
        ensure!(
            state.prefs.get::<String>("client_id")?.as_deref() == Some(&actor),
            "Identity changed"
        );
        let keypair = super::peer_wire::signing_keypair(&state.prefs)?;
        let body = handshake(&crate::base64_decode(&peer.key), &keypair, &actor)?;
        tokio::time::timeout(
            Duration::from_secs(5),
            socket.send(tokio_tungstenite::tungstenite::Message::Binary(body.into())),
        )
        .await??;
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                match socket.next().await {
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text)))
                        if text == "ok" =>
                    {
                        return Ok(());
                    }
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Binary(body)))
                        if body.as_ref() == b"ok" =>
                    {
                        return Ok(());
                    }
                    Some(Ok(
                        tokio_tungstenite::tungstenite::Message::Ping(_)
                        | tokio_tungstenite::tungstenite::Message::Pong(_),
                    )) => {}
                    _ => return Err(anyhow::anyhow!("Status acknowledgement missing")),
                }
            }
        })
        .await??;
        ensure!(
            self.current(epoch) && state.prefs.get_user_or("service", false),
            "Status runtime stopped"
        );
        let (lease, changed) = state
            .peer_status
            .connections
            .admit(&state.db, peer.clone())?;
        let _guard = ConnectionGuard {
            state: state.clone(),
            lease: lease.clone(),
        };
        drop(permit);
        if changed {
            emit(state, &peer.id, true);
        }
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _=tick.tick()=>ensure!(state.prefs.get_user_or("service",false) && state.peer_status.connections.valid(&state.db,&lease) && state.prefs.get::<String>("client_id")?.as_deref()==Some(&actor),"Peer changed"),
                frame=socket.next()=>match frame { Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_)))|None|Some(Err(_))=>return Err(anyhow::anyhow!("Status connection closed")),_=>{} }
            }
        }
    }
}
fn fingerprint(peer: &DPeer) -> String {
    format!("{}:{}:{}:{}", peer.key, peer.public_key, peer.ip, peer.port)
}
pub(super) async fn ensure_aware(state: &ServerState) -> Result<()> {
    for peer in peers::all(&state.db)?.into_iter().filter(|p| p.is_paired()) {
        state
            .host
            .call("peerStartAware", json!({"peer":peer}))
            .await
            .map_err(anyhow::Error::msg)?;
    }
    Ok(())
}
