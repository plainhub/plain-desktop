use plain_rs::chat::events::WS_PAIRING_FAILED;
use plain_rs::events::WsEvent;
use plain_rs::peers::pairing::sessions::Sessions;
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::{broadcast, watch},
    task::JoinHandle,
    time::MissedTickBehavior,
};

pub(crate) fn start(
    sessions: Arc<Sessions>,
    events: broadcast::Sender<WsEvent>,
    mut stop: watch::Receiver<bool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = stop.changed() => break,
                _ = interval.tick() => {
                    for ticket in sessions.expire_all() {
                        let _ = events.send(WsEvent::broadcast(WS_PAIRING_FAILED, json!({
                            "deviceId":ticket.target.device_id,"deviceName":ticket.target.device_name,
                            "generation":ticket.generation,"error":"Pairing timed out"
                        }).to_string()));
                    }
                }
            }
        }
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/services/discovery/pairing_timeout.rs"]
mod tests;
