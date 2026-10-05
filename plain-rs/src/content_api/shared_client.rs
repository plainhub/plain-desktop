use super::server::ServerState;
use crate::{
    chat::share_card::{self, Card},
    shares::client::{Info, Link},
};
use anyhow::{Result, ensure};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::HashSet, time::Duration};
fn add(
    links: &mut Vec<Link>,
    seen: &mut HashSet<(String, u16)>,
    host: &str,
    port: u16,
    card: &Card,
) {
    if let Ok(link) = Link::new(host, port, &card.share_id, &card.url_token) {
        if seen.insert((link.host.clone(), port)) {
            links.push(link);
        }
    }
}
fn add_addresses(
    links: &mut Vec<Link>,
    seen: &mut HashSet<(String, u16)>,
    ips: &str,
    port: u16,
    card: &Card,
) {
    let mut ips: Vec<String> = ips
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    let best = crate::chat::lan_ip::best(&ips, &crate::chat::lan_ip::local_interfaces());
    if !best.is_empty() {
        add(links, seen, &best, port, card);
    }
    for ip in ips.drain(..) {
        add(links, seen, &ip, port, card);
    }
}
fn candidates(state: &ServerState, card: &Card, discovered: bool) -> Result<Vec<Link>> {
    let mut links = vec![];
    let mut seen = HashSet::new();
    if state.prefs.get::<String>("client_id")?.as_deref() == Some(&card.peer_info.id) {
        let port = state.prefs.get_user::<u16>("https_port")?.unwrap_or(8443);
        add(&mut links, &mut seen, "127.0.0.1", port, card);
        for local in crate::chat::lan_ip::local_interfaces() {
            add(&mut links, &mut seen, &local.ip.to_string(), port, card);
        }
    }
    add_addresses(
        &mut links,
        &mut seen,
        &card.peer_info.ip,
        card.peer_info.port,
        card,
    );
    if let Some(peer) = crate::db::chat_store::peers::get(&state.db, &card.peer_info.id)? {
        add_addresses(&mut links, &mut seen, &peer.ip, peer.port, card);
    }
    if discovered {
        discovery_candidates(&mut links, &mut seen, &state.mdns.snapshot(), card);
    }
    Ok(links)
}
fn discovery_candidates(
    links: &mut Vec<Link>,
    seen: &mut HashSet<(String, u16)>,
    snapshot: &Value,
    card: &Card,
) {
    for service in snapshot["services"].as_array().into_iter().flatten() {
        if service["complete"] != true
            || !service["txtRecords"].as_array().is_some_and(|txt| {
                txt.iter()
                    .any(|entry| entry.as_str() == Some(&format!("id={}", card.peer_info.id)))
            })
        {
            continue;
        }
        let Some(port) = service["port"]
            .as_u64()
            .and_then(|port| u16::try_from(port).ok())
        else {
            continue;
        };
        for key in ["ips", "ipv6"] {
            for host in service[key]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                add(links, seen, host, port, card);
            }
        }
    }
}
pub(super) async fn fetch(state: &ServerState, link: &Link, path: Option<&str>) -> Result<Info> {
    let _permit = state.lan.capacity.clone().acquire_owned().await?;
    let response = state
        .lan
        .client
        .post(link.url("/guest_graphql")?)
        .timeout(Duration::from_secs(10))
        .header("content-type", "application/octet-stream")
        .header("c-id", &link.shared_id)
        .body(link.request(path)?)
        .send()
        .await?;
    ensure!(
        response.status().is_success(),
        "Shared HTTP {}",
        response.status()
    );
    let bytes = super::peer_lan::limited(response, 4 * 1024 * 1024).await?;
    link.response(&bytes)
}
async fn browse(
    state: &ServerState,
    id: &str,
    expected: &Card,
    path: Option<&str>,
) -> Result<Value> {
    let original = share_card::current(&state.db, id, expected)?;
    Link::new("127.0.0.1", 443, &expected.share_id, &expected.url_token)?;
    for round in 0..=12 {
        ensure!(
            share_card::current(&state.db, id, expected)? == original,
            "Share card changed"
        );
        if round > 0 {
            if (round - 1) % 4 == 0 {
                state.mdns.browse_resident();
            }
            tokio::time::sleep(Duration::from_millis(700)).await;
        }
        for link in candidates(state, expected, round > 0)? {
            ensure!(
                share_card::current(&state.db, id, expected)? == original,
                "Share card changed"
            );
            if let Ok(info) = fetch(state, &link, path).await {
                ensure!(
                    share_card::current(&state.db, id, expected)? == original,
                    "Share card changed"
                );
                let (card, updated) = share_card::refresh(
                    &state.db,
                    id,
                    expected,
                    &original,
                    &link.host,
                    link.port,
                    &info.name,
                    info.expires_at,
                )?;
                if let Some(row) = updated {
                    let _ = state.events.send(crate::ws_event::WsEvent::broadcast(
                        crate::chat::events::WS_MESSAGE_UPDATED,
                        json!([crate::chat::service::chat_to_json(&row)]).to_string(),
                    ));
                }
                return Ok(json!({"info":info,"link":link,"card":card}));
            }
        }
    }
    Ok(Value::Null)
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Request {
    Plan {
        kind: crate::shares::batch_plan::Kind,
        link: Link,
        entries: Vec<crate::shares::client::File>,
        target_dir: String,
        downloads_base: String,
    },
    OwnLink {
        id: String,
        host: Option<String>,
    },
    Fetch {
        link: Link,
        virtual_path: Option<String>,
    },
    Browse {
        message_id: String,
        expected: Card,
        virtual_path: Option<String>,
    },
    InitialLink {
        message_id: String,
        expected: Card,
    },
    FileUrl {
        link: Link,
        url_token: String,
        virtual_path: String,
        zip: bool,
    },
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
    let work = async {
        match request {
            Request::Plan {
                kind,
                link,
                entries,
                target_dir,
                downloads_base,
            } => {
                link.url("/guest_graphql")?;
                let mut walker = crate::shares::batch_plan::Walker::new(
                    kind,
                    entries,
                    &target_dir,
                    &downloads_base,
                )?;
                while let Some(path) = walker.next_directory()? {
                    let info = fetch(
                        &state,
                        &link,
                        if path.is_empty() { None } else { Some(&path) },
                    )
                    .await?;
                    walker.supply(info.entries)?;
                }
                Ok(serde_json::to_value(walker.finish()?)?)
            }
            Request::OwnLink { id, host } => {
                let service = crate::shares::Service::new(state.db.clone(), state.prefs.clone());
                let token = service.token(&id)?;
                let host = host.unwrap_or_else(|| {
                    crate::chat::lan_ip::local_interfaces()
                        .first()
                        .map(|iface| iface.ip.to_string())
                        .unwrap_or_else(|| "127.0.0.1".into())
                });
                let link = Link::new(
                    &host,
                    state.prefs.get_user::<u16>("https_port")?.unwrap_or(8443),
                    &id,
                    &token,
                )?;
                Ok(serde_json::to_value(link)?)
            }
            Request::Fetch { link, virtual_path } => Ok(serde_json::to_value(
                fetch(&state, &link, virtual_path.as_deref()).await?,
            )?),
            Request::Browse {
                message_id,
                expected,
                virtual_path,
            } => browse(&state, &message_id, &expected, virtual_path.as_deref()).await,
            Request::InitialLink {
                message_id,
                expected,
            } => {
                share_card::current(&state.db, &message_id, &expected)?;
                let link = candidates(&state, &expected, false)?
                    .into_iter()
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("Share address unavailable"))?;
                Ok(serde_json::to_value(link)?)
            }
            Request::FileUrl {
                link,
                url_token,
                virtual_path,
                zip,
            } => Ok(json!(link.file_url(&url_token, &virtual_path, zip)?)),
        }
    };
    let result = tokio::select! { _=stop.changed()=>Err(anyhow::anyhow!("Core stopped")), result=tokio::time::timeout(Duration::from_secs(120),work)=>result.unwrap_or_else(|_|Err(anyhow::anyhow!("Share request timed out"))) };
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
#[path = "../../tests/unit/content_api/shared_client.rs"]
mod tests;
