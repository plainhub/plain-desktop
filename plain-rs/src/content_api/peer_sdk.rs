use super::peer_lan::Failure;
use super::{host::Host, server::ServerState};
use anyhow::{Result, ensure};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

pub(super) struct Lease {
    host: Arc<Host>,
    token: String,
    jobs: Vec<tokio::task::JoinHandle<()>>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        for job in &self.jobs {
            job.abort();
        }
        let host = self.host.clone();
        let token = self.token.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = host
                    .call("peerTransportSocketClose", json!({"token":token}))
                    .await;
            });
        }
    }
}
async fn socket(state: &ServerState, peer: &crate::db::DPeer) -> Result<(DuplexStream, Lease)> {
    let token = crate::utils::short_uuid::short_uuid();
    let mut lease = Lease {
        host: state.host.clone(),
        token: token.clone(),
        jobs: vec![],
    };
    let value = state
        .host
        .call(
            "peerTransportSocketOpen",
            json!({"token":token,"peer":super::peer_address::view(peer)}),
        )
        .await
        .map_err(anyhow::Error::msg)?;
    ensure!(value == json!(true), "Aware socket unavailable");
    let (io, pump) = tokio::io::duplex(64 * 1024);
    let (mut reader, mut writer) = tokio::io::split(pump);
    let host = state.host.clone();
    let read_token = token.clone();
    let mut stop = state.stop.clone();
    lease.jobs.push(tokio::spawn(async move {
        loop {
            let response=tokio::select! {_=stop.changed()=>break,response=host.call("peerTransportSocketRead",json!({"token":read_token,"length":32768}))=>response};
            let Ok(response)=response else {break};
            if response["eof"]==true {break;}
            if let Some(encoded)=response["bytes"].as_str() {
                let bytes=crate::base64_decode(encoded);
                if bytes.len()>32768 || writer.write_all(&bytes).await.is_err() {break;}
            }
        }
    }));
    let host = state.host.clone();
    let mut stop = state.stop.clone();
    lease.jobs.push(tokio::spawn(async move {
        let mut bytes = vec![0; 32768];
        loop {
            let n = tokio::select! {_=stop.changed()=>break,n=reader.read(&mut bytes)=>n};
            let Ok(n) = n else { break };
            if n == 0 {
                break;
            }
            let result = host
                .call(
                    "peerTransportSocketWrite",
                    json!({"token":token,"bytes":crate::base64_encode(&bytes[..n])}),
                )
                .await;
            if !matches!(result,Ok(value) if value==json!(n)) {
                break;
            }
        }
    }));
    Ok((io, lease))
}
pub(super) struct Response {
    pub status: u16,
    pub body: hyper::body::Incoming,
    pub lease: Lease,
}
pub(super) async fn aware(
    state: &ServerState,
    peer: &crate::db::DPeer,
    method: &str,
    path: &str,
    channel: &str,
    body: Vec<u8>,
) -> Result<Response, Failure> {
    let (socket, mut lease) = socket(state, peer)
        .await
        .map_err(|e| Failure::Unavailable(e.to_string()))?;
    let tls = tokio_rustls::TlsConnector::from(Arc::new(
        super::status_socket::config().map_err(|e| Failure::Fatal(e.to_string()))?,
    ))
    .connect(
        rustls::pki_types::ServerName::try_from("plain-aware-peer")
            .map_err(|e| Failure::Fatal(e.to_string()))?,
        socket,
    )
    .await
    .map_err(|e| Failure::Unavailable(e.to_string()))?;
    let (mut sender, connection) =
        hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(tls))
            .await
            .map_err(|e| Failure::Unavailable(e.to_string()))?;
    lease.jobs.push(tokio::spawn(async move {
        let _ = connection.await;
    }));
    let actor = state
        .prefs
        .get::<String>("client_id")
        .map_err(|e| Failure::Fatal(e.to_string()))?
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Failure::Fatal("Missing peer actor".into()))?;
    let request = hyper::Request::builder()
        .method(method)
        .uri(path)
        .header("host", "plain-aware-peer")
        .header("c-id", actor)
        .header("c-cid", channel)
        .header("content-type", "application/octet-stream")
        .body(Full::new(Bytes::from(body)))
        .map_err(|e| Failure::Fatal(e.to_string()))?;
    let response = sender
        .send_request(request)
        .await
        .map_err(|e| Failure::Fatal(e.to_string()))?;
    Ok(Response {
        status: response.status().as_u16(),
        body: response.into_body(),
        lease,
    })
}
pub(super) async fn aware_send(
    state: &ServerState,
    peer: &crate::db::DPeer,
    channel: &str,
    key: &[u8],
    body: &str,
) -> Result<Value, Failure> {
    let encrypted = crate::xchacha_encrypt_raw(key, body.as_bytes())
        .ok_or_else(|| Failure::Fatal("Invalid peer key".into()))?;
    let response = aware(state, peer, "POST", "/peer_graphql", channel, encrypted).await?;
    async {
        let status = response.status;
        let _lease = response.lease;
        let mut body = response.body;
        let mut bytes = vec![];
        while let Some(frame) = body.frame().await {
            if let Ok(data) = frame?.into_data() {
                ensure!(
                    bytes.len() + data.len() <= 4 * 1024 * 1024,
                    "Peer response exceeds limit"
                );
                bytes.extend_from_slice(&data);
            }
        }
        decode_response(status, key, &bytes)
    }
    .await
    .map_err(|e: anyhow::Error| Failure::Fatal(e.to_string()))
}
fn decode_response(status: u16, key: &[u8], bytes: &[u8]) -> Result<Value> {
    let plain = crate::xchacha_decrypt_raw(key, bytes)
        .ok_or_else(|| anyhow::anyhow!("Failed to authenticate peer response"))?;
    if status != 200 {
        return Ok(json!({"data":null,"errors":[{"message":format!("Peer HTTP {status}")}]}));
    }
    let value: Value = serde_json::from_slice(&plain)?;
    ensure!(value.is_object(), "Invalid peer response");
    Ok(value)
}
pub(super) async fn ble(
    state: &ServerState,
    peer: &crate::db::DPeer,
    request: Value,
    channel: &str,
) -> Result<Value, Failure> {
    let actor = state
        .prefs
        .get::<String>("client_id")
        .map_err(|e| Failure::Fatal(e.to_string()))?
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Failure::Fatal("Missing peer actor".into()))?;
    let short = crate::chat::nearby_wire::short_id(&peer.id);
    let value=state.host.call("peerTransportBleExchange",json!({"peer":super::peer_address::view(peer),"shortId":short,"headers":{"c-id":actor,"c-cid":channel},"body":request.to_string()})).await.map_err(Failure::Fatal)?;
    let raw = value
        .as_str()
        .ok_or_else(|| Failure::Fatal("Missing BLE response".into()))?;
    if raw.len() > 6 * 1024 * 1024 {
        return Err(Failure::Fatal("BLE response exceeds limit".into()));
    }
    serde_json::from_str(raw).map_err(|e| Failure::Fatal(e.to_string()))
}
pub(super) async fn ble_send(
    state: &ServerState,
    peer: &crate::db::DPeer,
    channel: &str,
    key: &[u8],
    body: &str,
) -> Result<Value, Failure> {
    let encrypted = crate::xchacha_encrypt_raw(key, body.as_bytes())
        .ok_or_else(|| Failure::Fatal("Invalid peer key".into()))?;
    let response = ble(
        state,
        peer,
        json!({"m":"POST","p":"/peer_graphql","b":crate::base64_encode(&encrypted),"bb":true}),
        channel,
    )
    .await?;
    let bytes = decode_bytes(
        response["b"]
            .as_str()
            .ok_or_else(|| Failure::Fatal("Missing BLE body".into()))?,
    )
    .map_err(|e| Failure::Fatal(e.to_string()))?;
    decode_response(response["s"].as_u64().unwrap_or(200) as u16, key, &bytes)
        .map_err(|e| Failure::Fatal(e.to_string()))
}

pub(super) fn decode_bytes(encoded: &str) -> Result<Vec<u8>> {
    let bytes = crate::base64_decode(encoded);
    ensure!(
        crate::base64_encode(&bytes) == encoded,
        "Invalid SDK byte encoding"
    );
    Ok(bytes)
}
