use super::{
    pairing::{self, Device},
    pairing_runtime::{event, failed, success},
    server::ServerState,
};
use crate::chat::{
    events::WS_PAIRING_STARTED,
    nearby_wire::Message,
    pairing::sessions::{Target, Ticket},
};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Mutex, time::Duration};
#[derive(Default)]
pub(super) struct Pairings(Mutex<HashMap<String, Ticket>>);
enum Cause {
    Stopped,
    Failed(String),
}
pub(super) fn start(
    state: &ServerState,
    mut target: Target,
    device: Device,
) -> anyhow::Result<Value> {
    let mut jobs = state.ble_pairing.0.lock().unwrap();
    if let Some(ticket) = jobs.get(&target.device_id) {
        if state
            .pairing
            .remaining_ms(&ticket.target.device_id, &ticket.generation)
            .is_some()
        {
            return Ok(json!({"ticket":ticket}));
        }
        failed(state, &target, "Previous BLE connection is closing", None);
        return Ok(Value::Null);
    }
    if jobs.len() >= 2 {
        failed(state, &target, "BLE pairing capacity exceeded", None);
        return Ok(Value::Null);
    }
    target.device_ip.clear();
    let value = pairing::start(&state.prefs, &state.pairing, target, device)?;
    let ticket: Ticket = serde_json::from_value(value["ticket"].clone())?;
    let request = serde_json::from_value(value["request"].clone())?;
    let body = Message::PairRequest(request).wire()?;
    jobs.insert(ticket.target.device_id.clone(), ticket.clone());
    drop(jobs);
    let state = state.clone();
    let returned = ticket.clone();
    tokio::spawn(async move {
        let result = run(&state, &ticket, &body).await;
        if let Err(Cause::Failed(error)) = result {
            if state
                .pairing
                .remaining_ms(&ticket.target.device_id, &ticket.generation)
                .is_some()
                && state
                    .pairing
                    .cancel(&ticket.target.device_id, Some(&ticket.generation))
                    .is_some()
            {
                failed(&state, &ticket.target, &error, Some(&ticket.generation));
            }
        }
        let _ = state
            .host
            .call("blePairClose", json!({"generation":ticket.generation}))
            .await;
        let mut jobs = state.ble_pairing.0.lock().unwrap();
        if jobs
            .get(&ticket.target.device_id)
            .is_some_and(|t| t.generation == ticket.generation)
        {
            jobs.remove(&ticket.target.device_id);
        }
    });
    Ok(json!({"ticket":returned}))
}
async fn host(
    state: &ServerState,
    ticket: &Ticket,
    method: &str,
    params: Value,
) -> Result<Value, Cause> {
    let call = state.host.call(method, params);
    tokio::pin!(call);
    let mut stop = state.stop.clone();
    let mut timer = tokio::time::interval(Duration::from_millis(100));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _=stop.changed()=>return Err(Cause::Stopped),
            _=timer.tick()=>{if *stop.borrow() || state.pairing.remaining_ms(&ticket.target.device_id,&ticket.generation).is_none() {return Err(Cause::Stopped);}},
            value=&mut call=>return value.map_err(Cause::Failed)
        }
    }
}
async fn run(state: &ServerState, ticket: &Ticket, body: &str) -> Result<(), Cause> {
    let facts = json!({"id":ticket.target.device_id,"generation":ticket.generation});
    if host(state, ticket, "blePairConnect", facts.clone()).await? != Value::Bool(true) {
        return Err(Cause::Failed("Connection failed".into()));
    }
    let mut send = facts.clone();
    send["body"] = json!(body);
    if host(state, ticket, "blePairSend", send).await? != Value::Bool(true) {
        return Err(Cause::Failed("Failed to send pairing request".into()));
    }
    if !state
        .pairing
        .mark_sent(&ticket.target.device_id, &ticket.generation)
    {
        return Err(Cause::Stopped);
    }
    event(
        state,
        WS_PAIRING_STARTED,
        json!({"deviceId":ticket.target.device_id,"deviceName":ticket.target.device_name,"generation":ticket.generation}),
    );
    loop {
        let Some(remaining) = state
            .pairing
            .remaining_ms(&ticket.target.device_id, &ticket.generation)
        else {
            return Err(Cause::Stopped);
        };
        let mut wait = facts.clone();
        wait["timeoutMs"] = json!(remaining.min(1000));
        let response = host(state, ticket, "blePairWait", wait).await?;
        if response.get("connected").and_then(Value::as_bool) != Some(true) {
            return Err(Cause::Failed("Connection lost".into()));
        }
        let Some(body) = response.get("notification").and_then(Value::as_str) else {
            continue;
        };
        if body.len() > 65536 {
            continue;
        }
        let Ok(Message::PairResponse(response)) = Message::parse(body) else {
            continue;
        };
        if response.from_id != ticket.target.device_id {
            continue;
        }
        let result = pairing::complete_checked(
            &state.db,
            &state.prefs,
            &state.pairing,
            response,
            "",
            Some(&ticket.generation),
        )
        .map_err(|e| Cause::Failed(e.to_string()))?;
        if result.is_null() {
            continue;
        }
        if result["peer"].is_null() {
            failed(
                state,
                &ticket.target,
                result["error"].as_str().unwrap_or("Pairing failed"),
                Some(&ticket.generation),
            );
        } else {
            success(state, &result["peer"], "");
        }
        return Ok(());
    }
}
#[cfg(all(test, feature = "http_transport"))]
#[path = "../../tests/unit/content_api/ble_pairing.rs"]
mod tests;
