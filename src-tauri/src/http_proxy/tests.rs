use super::utils::extract_proxy_params;
use super::PeerResolver;
use std::sync::Arc;

fn no_peers() -> PeerResolver {
    Arc::new(|_id: &str| None)
}

#[test]
fn extract_no_query() {
    let p = extract_proxy_params("/fs");
    assert_eq!(p.path, "/fs");
    assert_eq!(p.pt, "");
    assert_eq!(p.cid, "");
}

#[test]
fn extract_only_pt() {
    let p = extract_proxy_params("/fs?_pt=https%3A%2F%2F192.0.2.1%3A8443");
    assert_eq!(p.path, "/fs");
    assert_eq!(p.pt, "https://192.0.2.1:8443");
    assert_eq!(p.cid, "");
}

#[test]
fn extract_pt_with_other_params() {
    let p = extract_proxy_params("/fs?id=abc&_pt=https%3A%2F%2F192.0.2.1%3A8443&w=50");
    assert_eq!(p.path, "/fs?id=abc&w=50");
    assert_eq!(p.pt, "https://192.0.2.1:8443");
}

#[test]
fn extract_no_pt_param() {
    let p = extract_proxy_params("/fs?id=abc&w=50");
    assert_eq!(p.path, "/fs?id=abc&w=50");
    assert_eq!(p.pt, "");
}

#[test]
fn extract_pt_first() {
    let p = extract_proxy_params("/fs?_pt=https%3A%2F%2F198.51.100.7%3A443&id=xyz");
    assert_eq!(p.path, "/fs?id=xyz");
    assert_eq!(p.pt, "https://198.51.100.7:443");
}

#[test]
fn extract_pt_middle() {
    let p = extract_proxy_params("/fs?id=abc&_pt=https%3A%2F%2F198.51.100.7%3A443&w=50");
    assert_eq!(p.path, "/fs?id=abc&w=50");
    assert_eq!(p.pt, "https://198.51.100.7:443");
}

#[test]
fn extract_pt_last() {
    let p = extract_proxy_params("/fs?id=abc&w=50&_pt=https%3A%2F%2F198.51.100.7%3A443");
    assert_eq!(p.path, "/fs?id=abc&w=50");
    assert_eq!(p.pt, "https://198.51.100.7:443");
}

#[test]
fn extract_cid_stripped_and_decoded() {
    let p = extract_proxy_params("/?cid=abc&_pt=wss%3A%2F%2F203.0.113.5%3A8443&_cid=peer%2D1");
    assert_eq!(p.path, "/?cid=abc");
    assert_eq!(p.pt, "wss://203.0.113.5:8443");
    assert_eq!(p.cid, "peer-1");
}

#[test]
fn extract_preserves_empty_segments() {
    // Empty segments (consecutive `&`) are skipped on re-join, matching
    // the standard `parse_query` semantics used elsewhere in the server.
    let p = extract_proxy_params("/fs?&&id=abc&&&_pt=x&&");
    assert_eq!(p.path, "/fs?id=abc");
    assert_eq!(p.pt, "x");
}

#[test]
fn extract_preserves_base64_plus_and_slash_and_equals() {
    // The `id` is base64-encrypted ciphertext from `bitArrayToBase64`.
    // encodeURIComponent on the frontend turns `+` -> `%2B`, `/` -> `%2F`,
    // `=` -> `%3D`. The proxy must pass these percent-triplets through
    // untouched; decoding here would turn `%2B` into a space and break
    // base64 decoding on the device side, surfacing as a 403.
    let raw = "/fs?id=abc%2Bdef%2Fghi%3D%3D&w=512&h=512&_pt=https%3A%2F%2F203.0.113.9%3A8643";
    let p = extract_proxy_params(raw);
    assert_eq!(p.path, "/fs?id=abc%2Bdef%2Fghi%3D%3D&w=512&h=512");
    assert_eq!(p.pt, "https://203.0.113.9:8643");
}

#[test]
fn extract_preserves_plain_plus_in_id() {
    // If a caller ever sends an *unencoded* `+` in `id` (defensive case),
    // the proxy must not turn it into a space either.
    let p = extract_proxy_params("/fs?id=abc+def&_pt=x");
    assert_eq!(p.path, "/fs?id=abc+def");
    assert_eq!(p.pt, "x");
}

#[test]
fn extract_preserves_percent_triplets_in_values() {
    let p = extract_proxy_params("/fs?id=hello%20world%26more&_pt=x");
    assert_eq!(p.path, "/fs?id=hello%20world%26more");
    assert_eq!(p.pt, "x");
}

#[test]
fn extract_valueless_param_kept_verbatim() {
    let p = extract_proxy_params("/fs?flag&_pt=x");
    assert_eq!(p.path, "/fs?flag");
    assert_eq!(p.pt, "x");
}

#[test]
fn replace_authority_swaps_host_between_scheme_and_path() {
    assert_eq!(
        super::replace_authority("wss://192.0.2.5:8443", "198.51.100.9:9999"),
        "wss://198.51.100.9:9999"
    );
    assert_eq!(
        super::replace_authority("https://192.0.2.5:8443/some/path?a=1", "198.51.100.9:9999"),
        "https://198.51.100.9:9999/some/path?a=1"
    );
    assert_eq!(super::replace_authority("ws://host", "h2:1"), "ws://h2:1");
    assert_eq!(super::replace_authority("not-a-url", "h2:1"), "not-a-url");
}

#[tokio::test]
async fn handle_strips_origin_but_forwards_custom_headers() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    // Mock upstream: read one full request (headers + content-length body),
    // then echo the received request head back as the response body so the
    // test can inspect exactly what the proxy forwarded.
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    let echo = tokio::spawn(async move {
        let (mut sock, _) = upstream.accept().await.unwrap();
        let mut buf = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            let head_end = buf.windows(4).position(|w| w == b"\r\n\r\n");
            if let Some(head_end) = head_end {
                let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
                let content_length = head
                    .lines()
                    .find_map(|l| {
                        let (k, v) = l.split_once(':')?;
                        (k.trim().eq_ignore_ascii_case("content-length"))
                            .then(|| v.trim().parse::<usize>().ok())?
                    })
                    .unwrap_or(0);
                if buf.len() >= head_end + 4 + content_length {
                    break;
                }
            }
            let n = sock.read(&mut chunk).await.unwrap();
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        let head_end = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        let head = &buf[..head_end];
        let resp = format!(
            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            head.len()
        );
        sock.write_all(resp.as_bytes()).await.unwrap();
        sock.write_all(head).await.unwrap();
        sock.shutdown().await.unwrap();
    });

    // Client-side socket feeding handle() directly.
    let relay = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = relay.local_addr().unwrap();
    let mut client = TcpStream::connect(relay_addr).await.unwrap();
    let (server_side, _) = relay.accept().await.unwrap();

    let client_http = reqwest::Client::new();
    tokio::spawn(async move {
        super::handle(server_side, client_http, no_peers()).await;
    });

    let pt = format!("http%3A%2F%2F{}", upstream_addr);
    let request = format!(
        "POST /upload?_pt={pt} HTTP/1.1\r\nhost: {relay_addr}\r\norigin: http://tauri.localhost\r\nc-id: client-1\r\ncontent-type: text/plain\r\ncontent-length: 2\r\nconnection: close\r\n\r\nhi"
    );
    client.write_all(request.as_bytes()).await.unwrap();
    client.shutdown().await.unwrap();

    let mut response = Vec::new();
    client.read_to_end(&mut response).await.unwrap();
    let echoed_head = String::from_utf8_lossy(&response);
    assert!(
        echoed_head.contains("c-id: client-1"),
        "custom header must be forwarded"
    );
    assert!(
        !echoed_head.to_lowercase().contains("\r\norigin:"),
        "origin header must be stripped before forwarding, got: {echoed_head}"
    );
    echo.await.unwrap();
}

#[tokio::test]
async fn websocket_upgrade_relays_handshake_and_frames() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    // Mock device: read the upgrade request head, answer 101, then echo
    // every byte. Reports the received head back to the test for asserts.
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    let (head_tx, head_rx) = tokio::sync::oneshot::channel::<String>();
    let device = tokio::spawn(async move {
        let (sock, _) = upstream.accept().await.unwrap();
        let mut sock = sock;
        let mut buf = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                let _ = head_tx.send(String::from_utf8_lossy(&buf[..end]).to_string());
                break;
            }
            let n = sock.read(&mut chunk).await.unwrap();
            if n == 0 {
                return;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        sock.write_all(
            b"HTTP/1.1 101 Switching Protocols\r\nupgrade: websocket\r\nconnection: Upgrade\r\nsec-websocket-accept: test-accept\r\n\r\n",
        )
        .await
        .unwrap();
        let (mut r, mut w) = sock.split();
        let _ = tokio::io::copy(&mut r, &mut w).await;
    });

    // The peer table resolves peer-1 to the mock device; _pt deliberately
    // points at an unroutable address to prove the resolution wins.
    let peers: PeerResolver = Arc::new(move |_id: &str| Some(upstream_addr.to_string()));

    let relay = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = relay.local_addr().unwrap();
    let mut client = TcpStream::connect(relay_addr).await.unwrap();
    let (server_side, _) = relay.accept().await.unwrap();

    let client_http = reqwest::Client::new();
    tokio::spawn(async move {
        super::handle(server_side, client_http, peers).await;
    });

    let pt = "ws%3A%2F%2F127.0.0.1%3A1";
    let request = format!(
        "GET /?cid=abc&_pt={pt}&_cid=peer%2D1 HTTP/1.1\r\nhost: {relay_addr}\r\nupgrade: websocket\r\nconnection: Upgrade\r\nsec-websocket-key: dGhlIHNhbXBsZSBub25jZQ==\r\nsec-websocket-version: 13\r\n\r\n"
    );
    client.write_all(request.as_bytes()).await.unwrap();

    // 101 head comes back with the device's accept header forwarded verbatim.
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        client.read_exact(&mut byte).await.unwrap();
        head.push(byte[0]);
        if head.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let head_str = String::from_utf8_lossy(&head).to_string();
    assert!(head_str.starts_with("HTTP/1.1 101 "), "got: {head_str}");
    assert!(
        head_str.contains("sec-websocket-accept: test-accept"),
        "got: {head_str}"
    );
    assert!(
        head_str.to_lowercase().contains("upgrade: websocket"),
        "got: {head_str}"
    );

    // Frames relay opaquely in both directions.
    client.write_all(b"frame-bytes").await.unwrap();
    let mut echoed = [0u8; 11];
    client.read_exact(&mut echoed).await.unwrap();
    assert_eq!(&echoed, b"frame-bytes");

    let device_head = head_rx.await.unwrap();
    assert!(
        device_head.starts_with("GET /?cid=abc "),
        "_pt/_cid stripped, path preserved; got: {device_head}"
    );
    assert!(
        device_head.contains("sec-websocket-key: dGhlIHNhbXBsZSBub25jZQ=="),
        "client key forwarded; got: {device_head}"
    );
    assert!(
        device_head.to_lowercase().contains("upgrade: websocket"),
        "got: {device_head}"
    );
    device.abort();
}

#[tokio::test]
async fn websocket_upgrade_refused_by_device_forwards_status() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    let device = tokio::spawn(async move {
        let (mut sock, _) = upstream.accept().await.unwrap();
        let mut buf = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
            let n = sock.read(&mut chunk).await.unwrap();
            if n == 0 {
                return;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        sock.write_all(b"HTTP/1.1 401 Unauthorized\r\ncontent-length: 0\r\n\r\n")
            .await
            .unwrap();
    });

    let relay = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = relay.local_addr().unwrap();
    let mut client = TcpStream::connect(relay_addr).await.unwrap();
    let (server_side, _) = relay.accept().await.unwrap();

    let client_http = reqwest::Client::new();
    tokio::spawn(async move {
        super::handle(server_side, client_http, no_peers()).await;
    });

    let pt = format!("ws%3A%2F%2F{}", upstream_addr);
    let request = format!(
        "GET /?_pt={pt} HTTP/1.1\r\nhost: {relay_addr}\r\nupgrade: websocket\r\nconnection: Upgrade\r\nsec-websocket-key: dGhlIHNhbXBsZSBub25jZQ==\r\nsec-websocket-version: 13\r\n\r\n"
    );
    client.write_all(request.as_bytes()).await.unwrap();

    let mut response = Vec::new();
    client.read_to_end(&mut response).await.unwrap();
    let text = String::from_utf8_lossy(&response).to_string();
    assert!(text.starts_with("HTTP/1.1 401 "), "got: {text}");
    device.abort();
}
