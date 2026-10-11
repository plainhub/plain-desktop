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
    pub async fn next_request(&self) -> Option<Value> {
        let mut requests = self.requests.lock().await;
        let mut stop = self.state.stop.clone();
        if *stop.borrow() || !self.state.host.is_current(self.generation) {
            return None;
        }
        tokio::select! { _ = stop.changed() => None, value = requests.recv() => value }
    }
    pub async fn next_event(&self) -> Option<Value> {
        let mut events = self.events.lock().await;
        let mut stop = self.state.stop.clone();
        loop {
            if *stop.borrow() || !self.state.host.is_current(self.generation) {
                return None;
            }
            let event = tokio::select! { _ = stop.changed() => return None, value = events.recv() => value };
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
    pub fn reply(&self, value: Value) -> Result<(), String> {
        self.state.host.reply(self.generation, value)
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
