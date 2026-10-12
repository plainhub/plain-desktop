use crate::assembly::server::ServerState;
use plain_rs::events::{HOST_BINARY_TYPES, HOST_TEXT_TYPES, WsEvent};
use serde_json::{Value, json};
use tokio::sync::{Mutex, broadcast, mpsc};

pub struct HostEvents {
    state: ServerState,
    generation: u64,
    requests: Mutex<mpsc::Receiver<Value>>,
    events: Mutex<broadcast::Receiver<WsEvent>>,
}
struct ServingSession<'a>(&'a HostEvents);

impl Drop for ServingSession<'_> {
    fn drop(&mut self) {
        self.0.state.host.disconnect(self.0.generation);
    }
}

impl HostEvents {
    pub(crate) fn new(state: ServerState) -> Self {
        let events = state.events.subscribe();
        let (generation, requests) = state.host.connect();
        tokio::spawn(
            crate::services::discovery::mdns_runtime::Runtime::host_connected(
                state.clone(),
                generation,
            ),
        );
        Self {
            state,
            generation,
            requests: Mutex::new(requests),
            events: Mutex::new(events),
        }
    }
    pub fn capabilities(&self) -> Value {
        json!({"textTypes":HOST_TEXT_TYPES,"binaryTypes":HOST_BINARY_TYPES})
    }
    pub async fn wait_disconnected(&self) {
        let mut stop = self.state.stop.clone();
        let mut connection = self.state.host.connection_changes();
        if *stop.borrow() || !self.state.host.is_current(self.generation) {
            return;
        }
        tokio::select! {
            _ = stop.changed() => {},
            _ = connection.changed() => {},
        }
    }
    pub async fn next_request(&self) -> Option<Value> {
        let mut requests = self.requests.lock().await;
        tokio::select! {
            biased;
            _ = self.wait_disconnected() => None,
            value = requests.recv() => value,
        }
    }
    pub async fn next_event(&self) -> Option<Value> {
        let mut events = self.events.lock().await;
        loop {
            let event = tokio::select! {
                biased;
                _ = self.wait_disconnected() => return None,
                value = events.recv() => value,
            };
            match event {
                Ok(event) if event.is_host_event() => continue,
                Ok(event) if event.binary_payload.is_some() => continue,
                Ok(event) => return Some(json!({"type":event.event_type,"payload":event.payload})),
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    #[cfg(feature = "http_transport")]
                    if let Some(failure) = self.state.http_failure.lock().unwrap().as_ref() {
                        return Some(
                            json!({"type":plain_rs::events::HTTP_SERVER_FAILED,"payload":serde_json::to_string(failure).unwrap(),"reconcile":true}),
                        );
                    }
                    return Some(json!({"type":"CONTENT_CHANGED","payload":"{}","reconcile":true}));
                }
                Err(_) => return None,
            }
        }
    }
    pub async fn serve<Requests, Events, Pending>(
        self: std::sync::Arc<Self>,
        requests: Requests,
        events: Events,
    ) -> Result<(), String>
    where
        Requests: Fn(String, Value) -> Pending,
        Pending: std::future::Future<Output = Result<Value, String>> + Send + 'static,
        Events: Fn(Value) -> Result<(), String>,
    {
        let _session = ServingSession(&self);
        self.serve_connected(requests, events).await
    }

    async fn serve_connected<Requests, Events, Pending>(
        &self,
        requests: Requests,
        events: Events,
    ) -> Result<(), String>
    where
        Requests: Fn(String, Value) -> Pending,
        Pending: std::future::Future<Output = Result<Value, String>> + Send + 'static,
        Events: Fn(Value) -> Result<(), String>,
    {
        let mut workers = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                biased;
                _ = self.wait_disconnected() => break,
                completed = workers.join_next(), if !workers.is_empty() => {
                    let (id, result) = completed
                        .ok_or("host worker unavailable")?
                        .map_err(|error| format!("host worker failed: {error}"))?;
                    let reply = match result {
                        Ok(result) => json!({"id": id, "result": result}),
                        Err(error) => json!({"id": id, "error": error}),
                    };
                    self.reply(reply)?;
                },
                event = self.next_event() => {
                    match event {
                        Some(event) => events(event)?,
                        None => break,
                    }
                },
                request = self.next_request(), if workers.len() < crate::host::bridge::REQUEST_CAPACITY => {
                    let Some(request) = request else { break };
                    let id = request.get("id").and_then(Value::as_u64)
                        .ok_or("invalid host request ID")?;
                    let method = request.get("method").and_then(Value::as_str)
                        .ok_or("invalid host request method")?.to_owned();
                    let params = request.get("params").cloned()
                        .ok_or("missing host request parameters")?;
                    let pending = requests(method, params);
                    workers.spawn(async move { (id, pending.await) });
                },
            }
        }
        workers.abort_all();
        while workers.join_next().await.is_some() {}
        Ok(())
    }

    pub fn reply(&self, value: Value) -> Result<(), String> {
        self.state.host.reply(self.generation, value)
    }
    pub fn attach_resource(
        &self,
        id: &str,
    ) -> Result<crate::native::resource_stream::NativeResourceStream, String> {
        self.state.bridge.attach_native(id, self.generation)
    }

    pub fn submit_thumbnail(&self, token: &str, bytes: Vec<u8>) -> Result<(), String> {
        if !self.state.host.is_current(self.generation) {
            return Err("host disconnected".into());
        }
        self.state.thumbnails.submit(token, bytes)
    }

    pub fn publish_text(&self, kind: &str, payload: String) -> Result<(), String> {
        self.publish(WsEvent::host_text(kind, payload))
    }
    pub fn publish_binary(&self, kind: &str, payload: Vec<u8>) -> Result<(), String> {
        self.publish(WsEvent::host_binary(kind, payload))
    }
    fn publish(&self, event: Option<WsEvent>) -> Result<(), String> {
        if !self.state.host.is_current(self.generation) {
            return Err("host disconnected".into());
        }
        let event = event.ok_or("host event type not allowed")?;
        let _ = self.state.events.send(event);
        Ok(())
    }
}
impl Drop for HostEvents {
    fn drop(&mut self) {
        self.state.host.disconnect(self.generation);
    }
}

#[cfg(test)]
#[path = "../../tests/unit/host/events.rs"]
mod tests;

#[cfg(test)]
pub(crate) struct TestHost(HostEvents);
#[cfg(test)]
impl TestHost {
    pub(crate) fn new(server: &crate::assembly::server::ContentServer) -> Self {
        Self(server.host_events())
    }
    pub(crate) async fn next(
        &mut self,
    ) -> Option<Result<tokio_tungstenite::tungstenite::Message, String>> {
        self.0.next_request().await.map(|v| {
            Ok(tokio_tungstenite::tungstenite::Message::Text(
                v.to_string().into(),
            ))
        })
    }
    pub(crate) async fn send(
        &mut self,
        value: tokio_tungstenite::tungstenite::Message,
    ) -> Result<(), String> {
        self.0.reply(
            serde_json::from_str(value.to_text().map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?,
        )
    }
}

#[cfg(test)]
#[path = "../../tests/unit/host/events_runtime.rs"]
mod runtime_tests;
