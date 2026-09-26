use serde_json::{Value, json};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite};

use super::super::graphql::{AppCtx, LocalSchema, execute_graphql};
use super::super::peer_graphql::{self, PeerSchema};
use super::file_server::serve_file;
use super::proxy_file::proxy_file;
use super::response::{APP_ID, respond};
use super::upload;
use crate::local::dlna;
use plain_rs::{base64_decode, base64_encode, xchacha_decrypt, xchacha_encrypt};

pub(super) async fn handle<R, W>(
    rd: R,
    mut wr: W,
    schema: &Arc<LocalSchema>,
    peer_schema: &Arc<PeerSchema>,
    ctx: &Arc<AppCtx>,
    data_dir: &std::path::Path,
    remote_ip: &str,
) where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let _ = data_dir; // /fs now uses ctx.data_dir via the file_server handler.
    let mut reader = tokio::io::BufReader::new(rd);

    let mut req_line = String::new();
    if reader.read_line(&mut req_line).await.is_err() {
        return;
    }
    let parts: Vec<&str> = req_line
        .trim_end_matches(['\r', '\n'])
        .splitn(3, ' ')
        .collect();
    if parts.len() < 2 {
        return;
    }
    let method = parts[0].to_owned();
    let raw_path = parts[1].to_owned();
    let (path, query_str) = raw_path
        .split_once('?')
        .map(|(p, q)| (p.to_owned(), q.to_owned()))
        .unwrap_or_else(|| (raw_path.clone(), String::new()));

    let mut content_type = String::new();
    let mut content_length: usize = 0;
    let mut header_client_id = String::new();
    let mut header_channel_id = String::new();
    let mut header_range = String::new();
    let mut header_soap_action = String::new();
    let mut header_sender_name = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await.is_err() {
            return;
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            let k = k.trim();
            let v = v.trim();
            if k.eq_ignore_ascii_case("content-type") {
                content_type = v.to_string();
            } else if k.eq_ignore_ascii_case("content-length") {
                content_length = v.parse().unwrap_or(0);
            } else if k.eq_ignore_ascii_case("c-id") {
                header_client_id = v.to_string();
            } else if k.eq_ignore_ascii_case("c-cid") {
                header_channel_id = v.to_string();
            } else if k.eq_ignore_ascii_case("range") {
                header_range = v.to_string();
            } else if k.eq_ignore_ascii_case("soapaction") {
                header_soap_action = v.to_string();
            } else if k.eq_ignore_ascii_case("c-name") {
                header_sender_name = v.to_string();
            }
        }
    }

    // DLNA MediaRenderer receiver routes — served plain (no token) so remote
    // control points can reach them. Gated by the DLNA toggle + running engine,
    // mirroring plain-app's `handleDlnaReceiver` (returns 404 when disabled).
    if dlna::is_receiver_path(&method, &path) {
        if !crate::prefs::get_dlna_enabled(&ctx.handle) || !ctx.dlna_engine.is_running() {
            respond(&mut wr, 404, "Not Found", b"", "text/plain").await;
            return;
        }
        let body = if content_length > 0 {
            let mut buf = vec![0u8; content_length];
            if tokio::io::AsyncReadExt::read_exact(&mut reader, &mut buf)
                .await
                .is_err()
            {
                return;
            }
            String::from_utf8_lossy(&buf).to_string()
        } else {
            String::new()
        };
        let mut headers = std::collections::HashMap::new();
        if !header_soap_action.is_empty() {
            headers.insert("soapaction".to_string(), header_soap_action);
        }
        if !header_sender_name.is_empty() {
            headers.insert("c-name".to_string(), header_sender_name);
        }
        let local_ip = crate::commands::discover::discover_local_ipv4_strs()
            .into_iter()
            .next()
            .unwrap_or_else(|| "127.0.0.1".to_string());
        let device_name = ctx.device_name.read().unwrap().clone();
        let allowed = crate::prefs::get_dlna_allowed_senders(&ctx.handle);
        let denied = crate::prefs::get_dlna_denied_senders(&ctx.handle);
        let Some(command_tx) = ctx.dlna_engine.command_sender() else {
            respond(&mut wr, 404, "Not Found", b"", "text/plain").await;
            return;
        };
        let resp = dlna::http_router::route(
            &ctx.dlna_engine.state,
            &method,
            &path,
            &headers,
            &body,
            ctx.dlna_engine.device_uuid(),
            &device_name,
            &local_ip,
            &command_tx,
            &allowed,
            &denied,
        )
        .await;
        respond_dlna(&mut wr, &resp).await;
        return;
    }

    if method == "OPTIONS" {
        respond(&mut wr, 200, "OK", b"", "text/plain").await;
        return;
    }

    match (method.as_str(), path.as_str()) {
        ("GET", "/health") => respond(&mut wr, 200, "OK", APP_ID.as_bytes(), "text/plain").await,
        ("POST", "/init") => {
            // Mirrors Kotlin SystemRoutes `/init`:
            //   1. If body encrypted with token → client is authenticated →
            //      return empty body (frontend: token + empty body → auto-login)
            //   2. Otherwise return InitResponse(signaturePublicKey) as JSON
            //      so the frontend can proceed with the handshake.
            //
            // The Tauri desktop app has no password management. The
            // signaturePublicKey is the Ed25519 verifying key (last 32
            // bytes of the 64-byte keypair).
            let mut authenticated = false;
            if content_length > 0 && !ctx.token.is_empty() {
                let mut body = vec![0u8; content_length];
                if reader.read_exact(&mut body).await.is_ok() {
                    authenticated = plain_rs::xchacha_decrypt(&ctx.token, &body).is_some();
                }
            }

            if authenticated {
                // Frontend: `r.status === 200 && token && !bodyText` → finishLoginSuccess()
                respond(&mut wr, 200, "OK", b"", "text/plain").await;
            } else {
                let kp_bytes = base64_decode(&ctx.identity.ed25519_keypair);
                let signature_public_key = if kp_bytes.len() == 64 {
                    base64_encode(&kp_bytes[32..])
                } else {
                    String::new()
                };
                let json = json!({ "signaturePublicKey": signature_public_key });
                respond(
                    &mut wr,
                    200,
                    "OK",
                    json.to_string().as_bytes(),
                    "application/json",
                )
                .await;
            }
        }
        ("GET", "/fs") => {
            serve_file(&mut wr, &query_str, &header_range, ctx).await;
        }
        ("GET", "/proxyfs") => {
            proxy_file(&mut wr, &query_str, &header_range, ctx).await;
        }
        ("POST", "/upload") => {
            if header_client_id.is_empty() || header_client_id != ctx.identity.client_id {
                respond(&mut wr, 401, "Unauthorized", b"", "text/plain").await;
                return;
            }
            upload::handle_upload(reader, wr, ctx, &content_type, content_length).await;
        }
        ("POST", "/upload_chunk") => {
            if header_client_id.is_empty() || header_client_id != ctx.identity.client_id {
                respond(&mut wr, 401, "Unauthorized", b"", "text/plain").await;
                return;
            }
            upload::handle_upload_chunk(reader, wr, ctx, &content_type, content_length).await;
        }
        ("POST", "/graphql") => {
            let mut body = vec![0u8; content_length];
            if content_length > 0
                && tokio::io::AsyncReadExt::read_exact(&mut reader, &mut body)
                    .await
                    .is_err()
            {
                return;
            }
            let Some(plaintext) = xchacha_decrypt(&ctx.token, &body) else {
                respond(&mut wr, 401, "Unauthorized", b"", "text/plain").await;
                return;
            };
            let json_bytes = strip_replay_prefix(&plaintext);
            let request: Value = serde_json::from_slice(json_bytes).unwrap_or_else(|_| json!({}));
            let response_json = execute_graphql(schema, request, ctx.clone()).await;
            let response_text = response_json.to_string();
            match xchacha_encrypt(&ctx.token, response_text.as_bytes()) {
                Some(encrypted) => {
                    respond(&mut wr, 200, "OK", &encrypted, "application/octet-stream").await
                }
                None => respond(&mut wr, 500, "Internal Server Error", b"", "text/plain").await,
            }
        }
        ("POST", "/peer_graphql") => {
            let mut body = vec![0u8; content_length];
            if content_length > 0
                && tokio::io::AsyncReadExt::read_exact(&mut reader, &mut body)
                    .await
                    .is_err()
            {
                return;
            }
            peer_graphql::handle(
                &mut wr,
                &body,
                &header_client_id,
                &header_channel_id,
                ctx,
                peer_schema,
            )
            .await;
        }
        // `POST /nearby` — LAN transport for pairing messages. The request
        // body is the same prefix-prefixed wire format the BLE nearby
        // service uses ("PAIR_REQUEST:{…}"). Mirrors plain-app `NearbyRoutes`.
        ("POST", "/nearby") => {
            let mut body = vec![0u8; content_length];
            if content_length > 0
                && tokio::io::AsyncReadExt::read_exact(&mut reader, &mut body)
                    .await
                    .is_err()
            {
                return;
            }
            let text = String::from_utf8_lossy(&body).to_string();
            let known = ctx.chat.pairing.handle_nearby_post(&text, remote_ip);
            if known {
                respond(&mut wr, 200, "OK", b"1", "text/plain").await;
            } else {
                log::error!(
                    "NearbyRoutes: unknown message type, body={}",
                    &text.chars().take(50).collect::<String>()
                );
                respond(
                    &mut wr,
                    400,
                    "Bad Request",
                    b"unknown message type",
                    "text/plain",
                )
                .await;
            }
        }
        _ => respond(&mut wr, 404, "Not Found", b"", "text/plain").await,
    }
}

/// Strip the `"TIMESTAMP|NONCE|"` replay-protection prefix from the decrypted payload.
fn strip_replay_prefix(payload: &[u8]) -> &[u8] {
    let mut pipe_count = 0u8;
    for (i, &b) in payload.iter().enumerate() {
        if b == b'|' {
            pipe_count += 1;
            if pipe_count == 2 {
                return &payload[i + 1..];
            }
        }
    }
    payload
}

/// Write a DLNA HTTP response, including the custom headers needed for GENA
/// SUBSCRIBE (SID / TIMEOUT). Uses `AsyncWrite` directly because DLNA responses
/// bypass the shared `respond` framing.
async fn respond_dlna<W: AsyncWrite + Unpin>(
    wr: &mut W,
    resp: &dlna::http_router::DlnaHttpResponse,
) {
    use tokio::io::AsyncWriteExt;
    let mut head = format!(
        "HTTP/1.1 {} {}\r\ncontent-length: {}\r\nconnection: close\r\n",
        resp.status,
        reason(resp.status),
        resp.body.len()
    );
    if let Some(ct) = &resp.content_type {
        head.push_str(&format!("content-type: {ct}\r\n"));
    }
    for (k, v) in &resp.headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    let _ = wr.write_all(head.as_bytes()).await;
    let _ = wr.write_all(resp.body.as_bytes()).await;
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        404 => "Not Found",
        _ => "Internal Server Error",
    }
}
