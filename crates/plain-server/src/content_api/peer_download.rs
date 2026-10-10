use super::{peer_transport::Pending, server::ServerState};
use crate::chat::transport_router::{Outcome, TransportType};
use anyhow::{Result, ensure};
use futures_util::StreamExt;
use http_body_util::BodyExt;
use serde_json::json;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

pub(super) async fn run(
    state: &ServerState,
    task: &crate::chat::download_queue::Snapshot,
    path: &str,
) -> Result<()> {
    let peer = super::peer_address::current(&state.db, &task.peer.id, &task.peer)?;
    let file_id = task.file["uri"]
        .as_str()
        .and_then(|s| s.strip_prefix("fsid:"))
        .ok_or_else(|| anyhow::anyhow!("Invalid remote attachment"))?;
    let mut available = vec![TransportType::Lan];
    if state.host.connected() {
        let kinds: Vec<TransportType> = serde_json::from_value(
            state
                .host
                .call("peerTransportCapabilities", json!({}))
                .await
                .map_err(anyhow::Error::msg)?,
        )?;
        available.extend(kinds.into_iter().filter(|k| *k != TransportType::Lan));
    }
    let step = state.transport.begin(&peer, &available)?;
    let ticket = step
        .ticket
        .ok_or_else(|| anyhow::anyhow!(step.error.unwrap_or_else(|| "No peer transport".into())))?;
    let mut pending = Pending {
        state,
        router: &state.transport,
        ticket,
    };
    let permit = state.lan.capacity.clone().acquire_owned().await?;
    let actor = state
        .prefs
        .get::<String>("client_id")?
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing peer actor"))?;
    let mut file = tokio::fs::File::create(path).await?;
    let mut bytes = 0;
    loop {
        super::peer_transport::notify(state);
        super::peer_address::current(&state.db, &peer.id, &peer)?;
        let unavailable = match pending.ticket.transport {
            TransportType::Lan => {
                let url = super::peer_address::file_url(&peer, file_id)?;
                let response = tokio::time::timeout(
                    Duration::from_secs(10),
                    state.lan.client.get(url).header("c-id", &actor).send(),
                )
                .await;
                match response {
                    Ok(Ok(response)) if response.status().is_success() => {
                        state
                            .transport
                            .finish(&pending.ticket, Outcome::Connected)?;
                        let mut chunks = response.bytes_stream();
                        while let Some(chunk) =
                            tokio::time::timeout(Duration::from_secs(30), chunks.next()).await?
                        {
                            write(state, task, &mut file, &mut bytes, &chunk?).await?;
                        }
                        None
                    }
                    Ok(Ok(response)) => Some(format!("Peer HTTP {}", response.status())),
                    Ok(Err(error)) => Some(error.to_string()),
                    Err(error) => Some(error.to_string()),
                }
            }
            TransportType::Aware => {
                let url = super::peer_address::file_url(&peer, file_id)?;
                let url = reqwest::Url::parse(&url)?;
                let path = format!("{}?{}", url.path(), url.query().unwrap_or_default());
                let response = tokio::time::timeout(
                    Duration::from_secs(15),
                    super::peer_sdk::aware(state, &peer, "GET", &path, "", vec![]),
                )
                .await;
                match response {
                    Ok(Ok(response)) if (200..300).contains(&response.status) => {
                        state
                            .transport
                            .finish(&pending.ticket, Outcome::Connected)?;
                        let _lease = response.lease;
                        let mut body = response.body;
                        while let Some(frame) =
                            tokio::time::timeout(Duration::from_secs(30), body.frame()).await?
                        {
                            if let Ok(chunk) = frame?.into_data() {
                                write(state, task, &mut file, &mut bytes, &chunk).await?;
                            }
                        }
                        None
                    }
                    Ok(Ok(response)) => Some(format!("Peer HTTP {}", response.status)),
                    Ok(Err(super::peer_lan::Failure::Unavailable(error))) => Some(error),
                    Ok(Err(super::peer_lan::Failure::Fatal(error))) => {
                        return Err(anyhow::anyhow!(error));
                    }
                    Err(error) => Some(error.to_string()),
                }
            }
            TransportType::Ble => {
                let mut offset = 0u64;
                loop {
                    super::peer_address::current(&state.db, &peer.id, &peer)?;
                    let response = super::peer_sdk::ble(
                        state,
                        &peer,
                        super::ble_wire::Request::FileChunk {
                            client_id: super::peer_sdk::ble_actor(state)
                                .map_err(|e| anyhow::anyhow!("{e:?}"))?,
                            file_id: file_id.into(),
                            offset,
                            length: 8192,
                        },
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
                    let (status, chunk) = super::ble_wire::decode_response(&response)?;
                    ensure!(status == 200, "BLE file HTTP failure");
                    ensure!(chunk.len() <= 8192, "Invalid BLE chunk length");
                    write(state, task, &mut file, &mut bytes, &chunk).await?;
                    offset += chunk.len() as u64;
                    if chunk.len() < 8192 {
                        break;
                    }
                }
                state
                    .transport
                    .finish(&pending.ticket, Outcome::Connected)?;
                None
            }
        };
        match unavailable {
            None => break,
            Some(error) => {
                let step = state
                    .transport
                    .finish(&pending.ticket, Outcome::Unavailable { error })?;
                pending.ticket = step.ticket.ok_or_else(|| {
                    anyhow::anyhow!(step.error.unwrap_or_else(|| "Peer unavailable".into()))
                })?;
            }
        }
    }
    ensure!(bytes == task.total, "Attachment byte count mismatch");
    super::peer_address::current(&state.db, &peer.id, &peer)?;
    file.flush().await?;
    file.sync_all().await?;
    drop(file);
    drop(permit);
    Ok(())
}
async fn write(
    state: &ServerState,
    task: &crate::chat::download_queue::Snapshot,
    file: &mut tokio::fs::File,
    bytes: &mut u64,
    chunk: &[u8],
) -> Result<()> {
    *bytes = bytes
        .checked_add(chunk.len() as u64)
        .ok_or_else(|| anyhow::anyhow!("Attachment size overflow"))?;
    ensure!(*bytes <= task.total, "Attachment exceeds declared size");
    file.write_all(chunk).await?;
    ensure!(
        state
            .downloads
            .progress(&task.id, &task.generation, *bytes)?,
        "Attachment transfer expired"
    );
    Ok(())
}
