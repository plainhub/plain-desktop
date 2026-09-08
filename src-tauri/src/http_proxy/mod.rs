//! Local HTTP proxy for browser-initiated requests.
//!
//! Solves the self-signed cert problem:
//!   browser ──plain HTTP──► 127.0.0.1:N ──TLS (self-signed ok)──► device
//!
//! Two request types it must handle well:
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
//! Target URL is passed via:
//!   a) _pt query parameter  — browser-initiated (<img>, <video>)
//!   b) x-proxy-target header — fetch/XHR

use std::net::TcpListener as StdTcpListener;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[cfg(test)]
mod tests;
mod utils;

use plain_rs::http::CORS;
use utils::extract_pt;

// ─── Public state ─────────────────────────────────────────────────────────────

pub struct HttpProxyState {
    pub port: u16,
}

impl HttpProxyState {
    /// Build a dedicated reqwest client (no timeout — video streaming is long-lived)
    /// and start accepting connections.
    pub fn start() -> Self {
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
                tokio::spawn(handle(stream, c));
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

async fn handle(stream: TcpStream, http: reqwest::Client) {
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
        let (path, pt) = extract_pt(&raw_path);
        let target_base = if !pt.is_empty() {
            pt.trim_end_matches('/').to_owned()
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
                    continue
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
        if keep_alive
            && let Some(len) = content_length {
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
