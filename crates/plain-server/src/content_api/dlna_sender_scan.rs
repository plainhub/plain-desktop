use super::{dlna_sender_runtime::Device, server::ServerState};
use crate::dlna_sender::{desc, ssdp};
use std::{collections::HashSet, time::Duration};

pub(super) fn start(state: &ServerState) {
    let runtime = state.cast.clone();
    let mut slot = runtime.scan.lock().unwrap();
    if slot.as_ref().is_some_and(|task| !task.is_finished()) {
        return;
    }
    let selected = runtime.snapshot().current_device.map(|device| device.id);
    runtime
        .devices
        .lock()
        .unwrap()
        .retain(|id, _| selected.as_ref() == Some(id));
    runtime.snapshot.lock().unwrap().devices.clear();
    let state = state.clone();
    *slot = Some(tokio::spawn(async move {
        scan(&state).await;
        let _guard = state.cast.operations.lock().await;
        super::dlna_sender_runtime::release_permission(&state, &state.cast.scan_lease).await;
    }));
}
async fn scan(state: &ServerState) {
    let local = ssdp::local_ipv4_addrs()
        .keys()
        .copied()
        .collect::<HashSet<_>>();
    let Ok(socket) = tokio::net::UdpSocket::bind("0.0.0.0:0").await else {
        return;
    };
    let query = ssdp::build_m_search("ssdp:all");
    let mut buffer = vec![0u8; 65536];
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    else {
        return;
    };
    let _ = socket
        .send_to(query.as_bytes(), "239.255.255.250:1900")
        .await;
    loop {
        let packet =
            tokio::time::timeout(Duration::from_secs(5), socket.recv_from(&mut buffer)).await;
        let (size, source) = match packet {
            Ok(Ok(packet)) => packet,
            Ok(Err(_)) => break,
            Err(_) => {
                let _ = socket
                    .send_to(query.as_bytes(), "239.255.255.250:1900")
                    .await;
                continue;
            }
        };
        if matches!(source.ip(),std::net::IpAddr::V4(ip) if local.contains(&ip)) {
            continue;
        }
        let host = source.ip().to_string();
        if state
            .cast
            .snapshot()
            .devices
            .iter()
            .any(|d| d.host_address == host)
        {
            continue;
        }
        let Ok(packet) = std::str::from_utf8(&buffer[..size]) else {
            continue;
        };
        let prefix = packet
            .chars()
            .take(20)
            .collect::<String>()
            .to_ascii_uppercase();
        if !prefix.starts_with("HTTP/1.1 200") && !prefix.starts_with("NOTIFY * HTTP") {
            continue;
        }
        let headers = ssdp::parse_ssdp_headers(packet);
        let Some(location) = headers.get("location") else {
            continue;
        };
        let Ok(mut response) = client.get(location).send().await else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }
        let mut body = Vec::new();
        let mut failed = false;
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    if body.len() + chunk.len() > 2 * 1024 * 1024 {
                        failed = true;
                        break;
                    }
                    body.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(_) => {
                    failed = true;
                    break;
                }
            }
        }
        if failed {
            continue;
        }
        let mut parsed = desc::parse_device_desc(&body);
        if !parsed.has_av_transport {
            continue;
        }
        parsed.location = location.clone();
        if parsed.udn.is_empty() {
            parsed.udn = ssdp::parse_udn_from_usn(
                headers.get("usn").map(String::as_str).unwrap_or_default(),
            );
        }
        if parsed.udn.is_empty() {
            parsed.udn = host.clone();
        }
        let device = Device {
            id: parsed.udn.clone(),
            host_address: host.clone(),
            name: if parsed.friendly_name.is_empty() {
                host
            } else {
                parsed.friendly_name.clone()
            },
            location: location.clone(),
        };
        state
            .cast
            .devices
            .lock()
            .unwrap()
            .insert(device.id.clone(), parsed);
        state.cast.snapshot.lock().unwrap().devices.push(device);
        state.cast.publish(&state);
    }
}
