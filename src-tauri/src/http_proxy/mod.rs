//! Local HTTP/WebSocket proxy — the webview's only network transport.
//!
//! Solves the self-signed cert problem:
//!   webview ──plain HTTP/WS──► 127.0.0.1:N ──TLS (self-signed ok)──► device
//!
//! Three request types it must handle well:
//!
//! 1. Thumbnails (<img> tags, many concurrent, small responses)
//!    → reqwest connection pool reuses TLS sessions → no per-request handshake overhead
//!
//! 2. Video/audio (<video> tag, Range requests, large response)
//!    → resp.chunk() streams bytes as they arrive, never buffers full body
//!    → when browser closes connection (video switched), write fails,
//!    loop exits, resp is dropped, reqwest closes upstream (body was
//!    abandoned mid-stream so the connection is NOT returned to the pool)
//!    → no stale upstream connections
//!
//! 3. WebSocket upgrades (app socket, peer sync sockets, login handshake)
//!    → dialed through reqwest's connection upgrade, then the TCP streams
//!    are spliced byte-for-byte — frames relay without parsing, and the
//!    handshake is forwarded verbatim so accept/extension checks stay
//!    end-to-end between webview and device
//!
//! Target URL is passed via:
//!   a) _pt query parameter  — every browser-initiated call (WS can set no headers)
//!   b) x-proxy-target header — fetch/XHR
//! A `_cid` query parameter (WS dials) names a paired peer; its current
//! `ip:port` is re-resolved from the peers table right before dialing.

use std::net::TcpListener as StdTcpListener;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[cfg(test)]
mod tests;
mod utils;

use plain_rs::http::CORS;
use utils::extract_proxy_params;

// ─── Public state ─────────────────────────────────────────────────────────────

/// Resolves a peer client id to its current `ip:port` (blocking — SQLite read).
pub type PeerResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

pub struct HttpProxyState {
    pub port: u16,
}

impl HttpProxyState {
    /// Build a dedicated reqwest client (no timeout — video streaming and
    /// WebSocket relays are long-lived) and start accepting connections.
    pub fn start(peers: PeerResolver) -> Self {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .danger_accept_invalid_hostnames(true)
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .pool_max_idle_per_host(20)
            .build()
            .expect("proxy reqwest client");

        let std_listener = StdTcpListener::bind("127.0.0.1:0").expect("http proxy bind");
        let port = std_listener.local_addr().expect("http proxy addr").port();
        std_listener.set_nonblocking(true).expect("set_nonblocking");

        tauri::async_runtime::spawn(async move {
            let listener =
                tokio::net::TcpListener::from_std(std_listener).expect("listener from_std");
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    continue;
                };
                let c = client.clone();
                let p = peers.clone();
                tokio::spawn(handle(stream, c, p));
            }
        });

        HttpProxyState { port }
    }
}

#[tauri::command]
pub fn http_proxy_port(state: tauri::State<'_, HttpProxyState>) -> u16 {
    state.port
}

// ─── Per-connection handler ───────────────────────────────────────────────────

async fn handle(stream: TcpStream, http: reqwest::Client, peers: PeerResolver) {
    let _ = stream.set_nodelay(true);
    let (rd, mut wr) = stream.into_split();
    let mut reader = BufReader::new(rd);

    // Serve multiple requests per connection (HTTP/1.1 keep-alive). The
    // browser reuses the loopback connection for every thumbnail / Range
    // request, and — because we fully consume each upstream body — reqwest
    // returns the TLS connection to its pool, so the next request skips the
    // TLS handshake to the device entirely. That handshake is the dominant
    // latency for video seeking on high-RTT links.
    loop {
        // 1. Request line. EOF or a blank line ends the connection.
        let mut req_line = String::new();
        match reader.read_line(&mut req_line).await {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let req_line = req_line.trim_end_matches(['\r', '\n']);
        if req_line.is_empty() {
            return;
        }
        let parts: Vec<&str> = req_line.splitn(3, ' ').collect();
        if parts.len() < 2 {
            return;
        }
        let method = parts[0].to_owned();
        let raw_path = parts[1].to_owned();

        // 2. Request headers.
        let mut req_headers: Vec<(String, String)> = Vec::new();
        let mut client_close = false;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).await.is_err() {
                return;
            }
            let t = line.trim_end_matches(['\r', '\n']);
            if t.is_empty() {
                break;
            }
            if let Some((k, v)) = t.split_once(':') {
                let key = k.trim().to_ascii_lowercase();
                if key == "connection" && v.to_ascii_lowercase().contains("close") {
                    client_close = true;
                }
                req_headers.push((key, v.trim().to_owned()));
            }
        }

        // 3. OPTIONS preflight — answer locally, close (no body reuse).
        if method == "OPTIONS" {
            let _ = wr.write_all(b"HTTP/1.1 200 OK\r\n").await;
            let _ = wr.write_all(CORS).await;
            let _ = wr
                .write_all(b"content-length: 0\r\nconnection: close\r\n\r\n")
                .await;
            return;
        }

        // 4. Proxy target.
        let params = extract_proxy_params(&raw_path);
        let path = params.path;
        let target_base = if !params.pt.is_empty() {
            params.pt.trim_end_matches('/').to_owned()
        } else {
            req_headers
                .iter()
                .find(|(k, _)| k == "x-proxy-target")
                .map(|(_, v)| v.trim_end_matches('/').to_owned())
                .unwrap_or_default()
        };
        if target_base.is_empty() {
            let _ = wr
                .write_all(b"HTTP/1.1 400 Bad Request\r\nconnection: close\r\n\r\n")
                .await;
            return;
        }

        // 4b. WebSocket upgrade — splice the connection and stop serving
        // HTTP on it. Must run before the body handling below: an upgrade
        // request has no body, and the connection never returns to the
        // keep-alive loop.
        if method == "GET"
            && req_headers
                .iter()
                .any(|(k, v)| k == "upgrade" && v.eq_ignore_ascii_case("websocket"))
        {
            relay_websocket(
                reader,
                wr,
                http,
                &req_headers,
                target_base,
                path,
                params.cid,
                peers,
            )
            .await;
            return;
        }

        // 5. Request body.
        let body_len: usize = req_headers
            .iter()
            .find(|(k, _)| k == "content-length")
            .and_then(|(_, v)| v.parse().ok())
            .unwrap_or(0);
        let mut body = vec![0u8; body_len];
        if body_len > 0 && reader.read_exact(&mut body).await.is_err() {
            return;
        }

        // 6. Forward request via reqwest (connection pool handles TLS reuse).
        let url = format!("{}{}", target_base, path);
        let req_method: reqwest::Method = method.parse().unwrap_or(reqwest::Method::GET);
        let mut builder = http.request(req_method, &url);
        for (k, v) in &req_headers {
            match k.as_str() {
                // Strip hop-by-hop and proxy-internal headers. `origin` must
                // go too: the device's CORS gate 403s cross-origin browser
                // requests (release builds only allow any host when the user
                // opts in), while Rust-client requests without Origin pass —
                // the proxy is the device's trusted agent, not a web page.
                "host" | "connection" | "transfer-encoding" | "x-proxy-target" | "origin" => {
                    continue;
                }
                _ => {
                    if let (Ok(name), Ok(val)) = (
                        reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                        reqwest::header::HeaderValue::from_str(v),
                    ) {
                        builder = builder.header(name, val);
                    }
                }
            }
        }
        if !body.is_empty() {
            builder = builder.body(body);
        }

        let mut resp = match builder.send().await {
            Ok(r) => r,
            Err(_) => {
                let _ = wr
                    .write_all(b"HTTP/1.1 502 Bad Gateway\r\nconnection: close\r\n\r\n")
                    .await;
                return;
            }
        };

        // 7. Keep-alive is only safe when we can frame the body ourselves:
        //    the upstream must give a content-length and must not transform
        //    the bytes (content-encoding) — reqwest auto-decompresses, which
        //    would desync a forwarded length. In that case we fall back to
        //    `connection: close` framing and stream to EOF.
        let resp_hdrs = resp.headers().clone();
        let content_length = resp_hdrs
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());
        let compressed = resp_hdrs
            .get(reqwest::header::CONTENT_ENCODING)
            .is_some_and(|v| v != "identity");
        let keep_alive = !client_close && content_length.is_some() && !compressed;

        // 8. Forward status + headers in a single write (fewer syscalls).
        let status = resp.status();
        let mut head: Vec<u8> = Vec::with_capacity(512);
        head.extend_from_slice(
            format!(
                "HTTP/1.1 {} {}\r\n",
                status.as_u16(),
                status.canonical_reason().unwrap_or("")
            )
            .as_bytes(),
        );
        head.extend_from_slice(CORS);
        head.extend_from_slice(if keep_alive {
            b"connection: keep-alive\r\n"
        } else {
            b"connection: close\r\n"
        });
        for (k, v) in &resp_hdrs {
            match k.as_str() {
                // Skip hop-by-hop, framing, and CORS headers we inject ourselves.
                "connection"
                | "keep-alive"
                | "transfer-encoding"
                | "content-length"
                | "content-encoding"
                | "access-control-allow-origin"
                | "access-control-allow-methods"
                | "access-control-allow-headers" => continue,
                _ => {}
            }
            if let Ok(vs) = v.to_str() {
                head.extend_from_slice(format!("{}: {}\r\n", k.as_str(), vs).as_bytes());
            }
        }
        if keep_alive && let Some(len) = content_length {
            head.extend_from_slice(format!("content-length: {}\r\n", len).as_bytes());
        }
        head.extend_from_slice(b"\r\n");
        if wr.write_all(&head).await.is_err() {
            return;
        }

        // 9. Stream the response body — never buffer the full body.
        //
        //    resp.chunk() returns the next piece of data as the device sends it.
        //    reqwest (hyper) dechunks Transfer-Encoding: chunked automatically,
        //    so we always get raw bytes regardless of how the device encoded them.
        //
        //    Keep-alive path: consume exactly content-length bytes. Fully
        //    consuming the body returns the upstream connection to the pool,
        //    so the next request reuses the TLS session instead of handshaking
        //    again. If the client disconnects mid-stream, write_all fails and
        //    we drop the connection — the abandoned upstream connection is
        //    closed by reqwest, no stale connections.
        if keep_alive {
            if method == "HEAD" {
                continue; // no body expected; keep serving this connection
            }
            let total = content_length.unwrap_or(0);
            let mut remaining = total;
            while remaining > 0 {
                match resp.chunk().await {
                    Ok(Some(data)) => {
                        if wr.write_all(&data).await.is_err() {
                            return;
                        }
                        remaining = remaining.saturating_sub(data.len() as u64);
                    }
                    // Upstream ended early — framing broken, drop the connection.
                    Ok(None) | Err(_) => return,
                }
            }
            // Next request on the same connection.
        } else {
            while let Ok(Some(data)) = resp.chunk().await {
                if wr.write_all(&data).await.is_err() {
                    return;
                }
            }
            return;
        }
    }
}

// ─── WebSocket relay ──────────────────────────────────────────────────────────

/// Splice a webview WebSocket through to the device: dial the target with an
/// HTTP upgrade, forward the device's `101` head verbatim (the client's
/// `sec-websocket-key` was forwarded too, so its accept check validates
/// against the device's answer), then copy bytes in both directions without
/// touching the frames.
#[allow(clippy::too_many_arguments)]
async fn relay_websocket(
    reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    wr: tokio::net::tcp::OwnedWriteHalf,
    http: reqwest::Client,
    req_headers: &[(String, String)],
    target_base: String,
    path: String,
    cid: String,
    peers: PeerResolver,
) {
    // Re-resolve a named peer's current ip:port (blocking SQLite read) so a
    // device that changed address reconnects on its fresh host even when the
    // URL was built from a stale login session.
    let mut base = target_base;
    if !cid.is_empty() {
        let resolved = tokio::task::spawn_blocking(move || peers(&cid))
            .await
            .ok()
            .flatten();
        if let Some(host) = resolved {
            base = replace_authority(&base, &host);
        }
    }

    // reqwest speaks http(s) only; ws/wss map onto it for the upgrade dial.
    let dial_base = if let Some(rest) = base.strip_prefix("wss://") {
        format!("https://{rest}")
    } else if let Some(rest) = base.strip_prefix("ws://") {
        format!("http://{rest}")
    } else {
        base
    };
    let path = if path.starts_with('/') {
        path
    } else {
        format!("/{path}")
    };

    // The client cannot send frames before our 101, so the BufReader holds
    // nothing beyond the request head — reunite the halves losslessly.
    let mut tcp = match reader.into_inner().reunite(wr) {
        Ok(stream) => stream,
        Err(_) => return,
    };

    // Forward the client's handshake material; the key must be the client's
    // own so the device's accept header satisfies the client's check.
    let mut builder = http
        .get(format!("{dial_base}{path}"))
        .header("upgrade", "websocket")
        .header("connection", "Upgrade")
        .header("sec-websocket-version", "13");
    let mut have_key = false;
    for (k, v) in req_headers {
        if k == "sec-websocket-key" {
            builder = builder.header(k.as_str(), v.as_str());
            have_key = true;
        } else if k.starts_with("sec-websocket-") && k != "sec-websocket-version" {
            builder = builder.header(k.as_str(), v.as_str());
        }
    }
    if !have_key {
        let _ = tcp
            .write_all(b"HTTP/1.1 400 Bad Request\r\nconnection: close\r\n\r\n")
            .await;
        return;
    }

    let mut resp = match builder.send().await {
        Ok(r) => r,
        Err(_) => {
            let _ = tcp
                .write_all(b"HTTP/1.1 502 Bad Gateway\r\nconnection: close\r\n\r\n")
                .await;
            return;
        }
    };

    // The device refused the upgrade — relay the refusal as a plain response.
    if resp.status().as_u16() != 101 {
        let status = resp.status();
        let mut head: Vec<u8> = Vec::with_capacity(256);
        head.extend_from_slice(
            format!(
                "HTTP/1.1 {} {}\r\n",
                status.as_u16(),
                status.canonical_reason().unwrap_or("")
            )
            .as_bytes(),
        );
        head.extend_from_slice(b"connection: close\r\n");
        for (k, v) in resp.headers() {
            if matches!(
                k.as_str(),
                "connection" | "transfer-encoding" | "content-length" | "keep-alive"
            ) {
                continue;
            }
            if let Ok(vs) = v.to_str() {
                head.extend_from_slice(format!("{}: {}\r\n", k.as_str(), vs).as_bytes());
            }
        }
        head.extend_from_slice(b"\r\n");
        if tcp.write_all(&head).await.is_ok() {
            while let Ok(Some(data)) = resp.chunk().await {
                if tcp.write_all(&data).await.is_err() {
                    break;
                }
            }
        }
        return;
    }

    let resp_hdrs = resp.headers().clone();
    let mut upgraded = match resp.upgrade().await {
        Ok(u) => u,
        Err(_) => {
            let _ = tcp
                .write_all(b"HTTP/1.1 502 Bad Gateway\r\nconnection: close\r\n\r\n")
                .await;
            return;
        }
    };

    // Echo the device's 101 head. `upgrade`/`connection` are written by us
    // (the webview requires them present); the accept/protocol headers come
    // from the device untouched.
    let mut head: Vec<u8> = Vec::with_capacity(256);
    head.extend_from_slice(
        b"HTTP/1.1 101 Switching Protocols\r\nupgrade: websocket\r\nconnection: Upgrade\r\n",
    );
    for (k, v) in &resp_hdrs {
        if matches!(k.as_str(), "upgrade" | "connection" | "transfer-encoding" | "content-length")
        {
            continue;
        }
        if let Ok(vs) = v.to_str() {
            head.extend_from_slice(format!("{}: {}\r\n", k.as_str(), vs).as_bytes());
        }
    }
    head.extend_from_slice(b"\r\n");
    if tcp.write_all(&head).await.is_err() {
        return;
    }

    // Dumb pipe from here on — frames, pings and close flow opaquely.
    let _ = tokio::io::copy_bidirectional(&mut tcp, &mut upgraded).await;
}

/// Swap everything between the scheme and the first `/` for `host`.
fn replace_authority(base: &str, host: &str) -> String {
    let Some(scheme_end) = base.find("://") else {
        return base.to_owned();
    };
    let after = &base[scheme_end + 3..];
    if let Some(slash) = after.find('/') {
        format!("{}://{host}{}", &base[..scheme_end], &after[slash..])
    } else {
        format!("{}://{host}", &base[..scheme_end])
    }
}
