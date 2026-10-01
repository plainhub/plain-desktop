//! `/proxyfs` — proxy a peer file through the local server.
//!
//! Mirrors plain-app `web/routes/FilesRoutes.kt::addFilesRoutes().get("/proxyfs")`:
//!   1. Decrypt the `id` query param with the local URL token → peer URL
//!      (e.g. `https://peer-ip:port/fs?id=…`).
//!   2. Validate the decrypted URL starts with `http`.
//!   3. Forward the request (including the `Range` header) to the peer.
//!   4. Stream the peer's response (status, headers, body) back to the
//!      caller.
//!
//! Why this exists: when a peer receives a chat message with `fsid:` URIs,
//! the web client builds a `/proxyfs` URL (see `lib/api/file.ts::getPeerProxyUrl`)
//! that wraps the peer's `/fs` URL. Routing through the local server avoids
//! the browser's mixed-content / self-signed-cert errors when fetching
//! directly from the peer's HTTPS endpoint.

use std::sync::{Arc, OnceLock};

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::HeaderValue;
use axum::response::Response;

use super::response::respond;
use crate::api::context::AppCtx;
use crate::base64_decode;
use crate::query::parse_query;
use crate::server::ServerState;
use crate::xchacha_decrypt;

pub async fn proxyfs_handler(State(state): State<ServerState>, req: Request) -> Response {
    let query_str = req.uri().query().unwrap_or("").to_string();
    let range_header = req
        .headers()
        .get("range")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    proxy_file(&query_str, &range_header, &state.ctx).await
}

pub async fn proxy_file(query_str: &str, range_header: &str, ctx: &Arc<AppCtx>) -> Response {
    // 1. Parse + decrypt the id.
    let params = parse_query(query_str);
    let id_encoded = match params.get("id") {
        Some(s) if !s.is_empty() => s.as_str(),
        _ => return respond(400, b"missing id".to_vec(), "text/plain"),
    };
    let id_bytes = base64_decode(id_encoded);
    let Some(plaintext) = xchacha_decrypt(&ctx.token, &id_bytes) else {
        return respond(401, Vec::new(), "text/plain");
    };
    let peer_url = match std::str::from_utf8(&plaintext) {
        Ok(s) => s,
        Err(_) => return respond(400, b"invalid utf-8".to_vec(), "text/plain"),
    };

    // 2. Validate the peer URL.
    if !peer_url.starts_with("http") {
        return respond(400, b"Invalid peer URL".to_vec(), "text/plain");
    }

    // 3. Forward the request to the peer, forwarding the Range header
    //    so media seeking works through the proxy.
    let mut req = proxy_client().get(peer_url);
    if !range_header.is_empty()
        && let Ok(v) = HeaderValue::from_str(range_header)
    {
        req = req.header("range", v);
    }
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => return respond(502, e.to_string().into_bytes(), "text/plain"),
    };

    // 4. Build the proxied response: same status, the peer's headers
    //    minus the hop-by-hop and CORS ones (we inject our own), and a
    //    streaming body. Non-UTF-8 header values are skipped, matching
    //    the old head-serializer behavior.
    let status = resp.status();
    let mut builder = axum::http::Response::builder().status(status);
    for (k, v) in resp.headers() {
        if is_stripped_header(k.as_str()) || v.to_str().is_err() {
            continue;
        }
        builder = builder.header(k.clone(), v.clone());
    }
    let stream = resp.bytes_stream();
    builder
        .body(Body::from_stream(stream))
        .expect("static response")
}

/// Hop-by-hop and CORS headers stripped from the peer's response — we
/// re-frame the connection ourselves; the router CORS layer adds our
/// own set.
pub fn is_stripped_header(name: &str) -> bool {
    matches!(
        name,
        "connection"
            | "keep-alive"
            | "transfer-encoding"
            | "access-control-allow-origin"
            | "access-control-allow-methods"
            | "access-control-allow-headers"
    )
}

/// Shared reqwest client for `/proxyfs` requests. Cloning is cheap —
/// `reqwest::Client` is an `Arc` internally, so all clones share one
/// connection pool. Built once with self-signed-cert tolerance so it
/// can reach peers whose local server uses a self-generated TLS cert.
fn proxy_client() -> reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .danger_accept_invalid_certs(true)
                .danger_accept_invalid_hostnames(true)
                .tcp_nodelay(true)
                .tcp_keepalive(std::time::Duration::from_secs(60))
                .pool_max_idle_per_host(20)
                .build()
                .expect("proxyfs reqwest client")
        })
        .clone()
}

#[cfg(test)]
#[path = "../../tests/unit/api/server/proxy_file.rs"]
mod tests;
