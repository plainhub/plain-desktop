use super::host::Host;
use axum::{
    body::Body,
    extract::ws::{Message, WebSocket},
    http::StatusCode,
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot, watch};

#[derive(Clone)]
pub(super) struct RemoteHost(pub String);

struct Pending {
    attached: oneshot::Sender<()>,
    outgoing: mpsc::Receiver<Message>,
    incoming: mpsc::Sender<Message>,
    stop: watch::Receiver<bool>,
}
pub struct NativeResources {
    host: Arc<Host>,
    pending: Mutex<HashMap<String, Pending>>,
    capacity: Arc<Semaphore>,
}
struct Exchange {
    outgoing: mpsc::Sender<Message>,
    incoming: mpsc::Receiver<Message>,
    _permit: OwnedSemaphorePermit,
}
struct Registration<'a> {
    bridge: &'a NativeResources,
    id: String,
}
impl Drop for Registration<'_> {
    fn drop(&mut self) {
        self.bridge.pending.lock().unwrap().remove(&self.id);
    }
}
impl NativeResources {
    pub fn new(host: Arc<Host>) -> Self {
        Self {
            host,
            pending: Mutex::new(HashMap::new()),
            capacity: Arc::new(Semaphore::new(32)),
        }
    }
    pub async fn attach(&self, id: &str, mut socket: WebSocket) {
        let pending = self.pending.lock().unwrap().remove(id);
        let Some(mut pending) = pending else {
            let _ = tokio::time::timeout(Duration::from_secs(1), socket.close()).await;
            return;
        };
        let _ = pending.attached.send(());
        loop {
            tokio::select! {
                _ = pending.stop.changed() => break,
                message = pending.outgoing.recv() => {
                    let Some(message) = message else { break; };
                    if tokio::select! { _=pending.stop.changed()=>true, result=socket.send(message)=>result.is_err() } { break; }
                }
                message = socket.next() => {
                    match message {
                        Some(Ok(Message::Ping(_)|Message::Pong(_))) => {},
                        Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                        Some(Ok(Message::Binary(ref bytes))) if bytes.len()>64*1024=>break,
                        Some(Ok(message)) => {
                            if tokio::select! {
                                _ = pending.stop.changed() => true,
                                result = pending.incoming.send(message) => result.is_err(),
                            } { break; }
                        }
                    }
                }
            }
        }
        let _ = tokio::time::timeout(Duration::from_secs(1), socket.close()).await;
    }
    pub(super) async fn resource(
        &self,
        path: &str,
        stop: watch::Receiver<bool>,
    ) -> Result<Body, StatusCode> {
        let exchange = self.open(json!({"path":path}), stop.clone()).await?;
        let stream = futures_util::stream::try_unfold(
            (exchange, stop),
            |(mut exchange, mut stop)| async move {
                let message = tokio::select! { _=stop.changed()=>None,message=exchange.incoming.recv()=>message };
                match message {
                    Some(Message::Binary(bytes)) => {
                        Ok::<_, std::io::Error>(Some((bytes, (exchange, stop))))
                    }
                    Some(Message::Text(value))
                        if serde_json::from_str::<Value>(&value)
                            .ok()
                            .is_some_and(|value| value["kind"] == "end") =>
                    {
                        Ok(None)
                    }
                    _ => Err(std::io::Error::new(
                        std::io::ErrorKind::ConnectionAborted,
                        "Native resource interrupted",
                    )),
                }
            },
        );
        Ok(Body::from_stream(stream))
    }
    async fn open(
        &self,
        metadata: Value,
        mut stop: watch::Receiver<bool>,
    ) -> Result<Exchange, StatusCode> {
        let permit = self
            .capacity
            .clone()
            .try_acquire_owned()
            .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;
        let id = uuid::Uuid::new_v4().to_string();
        let (attached_tx, attached_rx) = oneshot::channel();
        let (outgoing, receiver) = mpsc::channel(4);
        let (sender, incoming) = mpsc::channel(4);
        self.pending.lock().unwrap().insert(
            id.clone(),
            Pending {
                attached: attached_tx,
                outgoing: receiver,
                incoming: sender,
                stop: stop.clone(),
            },
        );
        let _registration = Registration {
            bridge: self,
            id: id.clone(),
        };
        tokio::select! {
            _ = stop.changed() => return Err(StatusCode::SERVICE_UNAVAILABLE),
            result = self.host.call("fileResourceStream", json!({"id":id})) => {
                result.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
            }
        }
        tokio::select! {
            _ = stop.changed() => return Err(StatusCode::SERVICE_UNAVAILABLE),
            result = tokio::time::timeout(Duration::from_secs(15), attached_rx) => {
                result.map_err(|_| StatusCode::GATEWAY_TIMEOUT)?.map_err(|_| StatusCode::BAD_GATEWAY)?;
            }
        }
        let send = outgoing.send(Message::Text(metadata.to_string()));
        tokio::select! {
            _ = stop.changed() => return Err(StatusCode::SERVICE_UNAVAILABLE),
            result = tokio::time::timeout(Duration::from_secs(15), send) => {
                result.map_err(|_| StatusCode::GATEWAY_TIMEOUT)?.map_err(|_| StatusCode::BAD_GATEWAY)?;
            }
        }
        Ok(Exchange {
            outgoing,
            incoming,
            _permit: permit,
        })
    }
}
