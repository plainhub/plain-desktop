use crate::assembly::server::ServerState;
use crate::services::peers::status_outgoing;
use serde::Deserialize;
use serde_json::{Value, json};

pub struct Controller {
    state: ServerState,
}

impl Controller {
    pub(crate) fn new(state: ServerState) -> Self {
        Self { state }
    }

    /// One entry point for the mobile bridge. Snapshot reads are not exposed
    /// here — the public `peers { id online }` GraphQL query covers them.
    /// This controller is just the runtime-control surface for the app.
    pub async fn execute_json(&self, command: &str) -> Result<String, String> {
        let request: Request =
            serde_json::from_str(command).map_err(|e| e.to_string())?;
        let mut stop = self.state.stop.clone();
        if *stop.borrow() {
            return Err("Core stopped".into());
        }
        let result = tokio::select! {
            _ = stop.changed() => Err("Core stopped".to_string()),
            result = execute(&self.state, request) => result,
        }?;
        serde_json::to_string(&result).map_err(|e| e.to_string())
    }
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
enum Request {
    Start,
    Stop,
    Reconnect,
    EnsureAware,
}

fn emit(state: &ServerState, id: &str, online: bool) {
    if id.is_empty() {
        return;
    }
    let _ = state.events.send(plain_rs::events::WsEvent::broadcast(
        plain_rs::chat::events::WS_PEER_STATUS_UPDATED,
        json!({"id": id, "online": online}).to_string(),
    ));
    let _ = state.events.send(plain_rs::events::WsEvent::broadcast(
        "PEER_CONNECTIONS_UPDATED",
        json!({"id": id, "online": online}).to_string(),
    ));
}

async fn execute(state: &ServerState, request: Request) -> Result<Value, String> {
    if matches!(request, Request::EnsureAware) {
        status_outgoing::ensure_aware(state)
            .await
            .map_err(|e| e.to_string())?;
        return Ok(Value::Null);
    }
    let _control = state.peer_status.control.lock().await;
    match request {
        Request::Start => {
            if state
                .peer_status
                .public_active
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                state.peer_status.outgoing.start(state.clone());
                Ok(Value::Null)
            } else {
                Err("Public server inactive".to_string())
            }
        }
        Request::Stop => {
            state.peer_status.outgoing.stop().await;
            let previous = state.peer_status.connections.snapshot(&state.db);
            state.peer_status.connections.clear_hints();
            let current = state.peer_status.connections.snapshot(&state.db);
            if let Some(previous_ids) = previous["online"].as_array() {
                for id in previous_ids.iter().filter_map(|v| v.as_str()) {
                    let still_online = current["online"]
                        .as_array()
                        .is_some_and(|ids| ids.iter().any(|c| c.as_str() == Some(id)));
                    if !still_online {
                        emit(state, id, false);
                    }
                }
            }
            Ok(Value::Null)
        }
        Request::Reconnect => {
            state
                .peer_status
                .outgoing
                .reconnect(state)
                .map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        Request::EnsureAware => unreachable!(),
    }
}