use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{Notify, Semaphore, mpsc, oneshot};

type Reply = Result<Value, String>;
#[derive(Default)]
struct State {
    generation: u64,
    next_id: u64,
    peer_sockets: bool,
    sender: Option<mpsc::Sender<Value>>,
    pending: HashMap<u64, oneshot::Sender<Reply>>,
}
pub struct Host {
    state: Arc<Mutex<State>>,
    ready: Notify,
    capacity: Semaphore,
}
impl Default for Host {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(State::default())),
            ready: Notify::new(),
            capacity: Semaphore::new(32),
        }
    }
}
struct Pending {
    state: Arc<Mutex<State>>,
    id: u64,
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.state.lock().unwrap().pending.remove(&self.id);
    }
}
impl Host {
    pub(super) fn needs_socket_cleanup(&self) -> bool {
        self.state.lock().unwrap().peer_sockets
    }
    pub(super) fn connected(&self) -> bool {
        self.state.lock().unwrap().sender.is_some()
    }
    pub fn connect(&self) -> (u64, mpsc::Receiver<Value>) {
        let (sender, receiver) = mpsc::channel(32);
        let mut state = self.state.lock().unwrap();
        for (_, reply) in state.pending.drain() {
            let _ = reply.send(Err("host connection replaced".into()));
        }
        state.generation += 1;
        state.sender = Some(sender);
        state.peer_sockets = false;
        let generation = state.generation;
        drop(state);
        self.ready.notify_waiters();
        (generation, receiver)
    }
    pub fn is_current(&self, generation: u64) -> bool {
        let state = self.state.lock().unwrap();
        state.generation == generation && state.sender.is_some()
    }
    pub fn disconnect(&self, generation: u64) {
        let mut state = self.state.lock().unwrap();
        if state.generation != generation {
            return;
        }
        state.sender = None;
        for (_, reply) in state.pending.drain() {
            let _ = reply.send(Err("host disconnected".into()));
        }
    }
    pub fn reply(&self, generation: u64, message: Value) -> Result<(), String> {
        let id = message
            .get("id")
            .and_then(Value::as_u64)
            .ok_or("invalid host reply ID")?;
        let mut state = self.state.lock().unwrap();
        if state.generation != generation {
            return Ok(());
        }
        if let Some(reply) = state.pending.remove(&id) {
            let result = match message.get("error") {
                Some(Value::String(error)) => Err(error.clone()),
                Some(_) => Err("invalid host error".into()),
                None => message
                    .get("result")
                    .cloned()
                    .ok_or_else(|| "missing host result".into()),
            };
            let _ = reply.send(result);
        }
        Ok(())
    }
    pub async fn call(&self, method: &str, params: Value) -> Reply {
        self.call_inner(method, params, Some(Duration::from_secs(25)))
            .await
    }
    pub(super) async fn call_wait(&self, method: &str, params: Value) -> Reply {
        self.call_inner(method, params, None).await
    }
    async fn call_inner(&self, method: &str, params: Value, timeout: Option<Duration>) -> Reply {
        let _permit = self
            .capacity
            .try_acquire()
            .map_err(|_| "host request capacity exceeded")?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let (id, receiver) = loop {
            let notified = self.ready.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let request = {
                let mut state = self.state.lock().unwrap();
                if let Some(sender) = state.sender.clone() {
                    if method == "peerTransportSocketOpen" {
                        state.peer_sockets = true;
                    }
                    state.next_id += 1;
                    let id = state.next_id;
                    let (reply, receiver) = oneshot::channel();
                    state.pending.insert(id, reply);
                    if sender
                        .try_send(json!({"id":id,"method":method,"params":params}))
                        .is_err()
                    {
                        state.pending.remove(&id);
                        return Err("host request queue unavailable".into());
                    }
                    Some((id, receiver))
                } else {
                    None
                }
            };
            if let Some(request) = request {
                break request;
            }
            tokio::time::timeout_at(deadline, notified)
                .await
                .map_err(|_| "host unavailable")?;
        };
        let _pending = Pending {
            state: self.state.clone(),
            id,
        };
        match timeout {
            Some(timeout) => tokio::time::timeout(timeout, receiver)
                .await
                .map_err(|_| "host request timed out")?,
            None => receiver.await,
        }
        .map_err(|_| "host reply canceled")?
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/host.rs"]
mod tests;
