use super::utils::extract_pt;

#[test]
fn extract_pt_no_query() {
    let (p, pt) = extract_pt("/fs");
    assert_eq!(p, "/fs");
    assert_eq!(pt, "");
}

#[test]
fn extract_pt_only_pt() {
    let (p, pt) = extract_pt("/fs?_pt=https%3A%2F%2F192.168.1.1%3A8443");
    assert_eq!(p, "/fs");
    assert_eq!(pt, "https://192.168.1.1:8443");
}

#[test]
fn extract_pt_pt_with_other_params() {
    let (p, pt) = extract_pt("/fs?id=abc&_pt=https%3A%2F%2F192.168.1.1%3A8443&w=50");
    assert_eq!(p, "/fs?id=abc&w=50");
    assert_eq!(pt, "https://192.168.1.1:8443");
}

#[test]
fn extract_pt_no_pt_param() {
    let (p, pt) = extract_pt("/fs?id=abc&w=50");
    assert_eq!(p, "/fs?id=abc&w=50");
    assert_eq!(pt, "");
}

#[test]
fn extract_pt_pt_first() {
    let (p, pt) = extract_pt("/fs?_pt=https%3A%2F%2F10.0.0.1%3A443&id=xyz");
    assert_eq!(p, "/fs?id=xyz");
    assert_eq!(pt, "https://10.0.0.1:443");
}

#[test]
fn extract_pt_pt_middle() {
    let (p, pt) = extract_pt("/fs?id=abc&_pt=https%3A%2F%2F10.0.0.1%3A443&w=50");
    assert_eq!(p, "/fs?id=abc&w=50");
    assert_eq!(pt, "https://10.0.0.1:443");
}

#[test]
fn extract_pt_pt_last() {
    let (p, pt) = extract_pt("/fs?id=abc&w=50&_pt=https%3A%2F%2F10.0.0.1%3A443");
    assert_eq!(p, "/fs?id=abc&w=50");
    assert_eq!(pt, "https://10.0.0.1:443");
}

#[test]
fn extract_pt_preserves_empty_segments() {
    // Empty segments (consecutive `&`) are skipped on re-join, matching
    // the standard `parse_query` semantics used elsewhere in the server.
    let (p, pt) = extract_pt("/fs?&&id=abc&&&_pt=x&&");
    assert_eq!(p, "/fs?id=abc");
    assert_eq!(pt, "x");
}

#[test]
fn extract_pt_preserves_base64_plus_and_slash_and_equals() {
    // The `id` is base64-encrypted ciphertext from `bitArrayToBase64`.
    // encodeURIComponent on the frontend turns `+` -> `%2B`, `/` -> `%2F`,
    // `=` -> `%3D`. The proxy must pass these percent-triplets through
    // untouched; decoding here would turn `%2B` into a space and break
    // base64 decoding on the device side, surfacing as a 403.
    let raw = "/fs?id=abc%2Bdef%2Fghi%3D%3D&w=512&h=512&_pt=https%3A%2F%2F192.168.123.23%3A8643";
    let (p, pt) = extract_pt(raw);
    assert_eq!(p, "/fs?id=abc%2Bdef%2Fghi%3D%3D&w=512&h=512");
    assert_eq!(pt, "https://192.168.123.23:8643");
}

#[test]
fn extract_pt_preserves_plain_plus_in_id() {
    // If a caller ever sends an *unencoded* `+` in `id` (defensive case),
    // the proxy must not turn it into a space either.
    let (p, pt) = extract_pt("/fs?id=abc+def&_pt=x");
    assert_eq!(p, "/fs?id=abc+def");
    assert_eq!(pt, "x");
}

#[test]
fn extract_pt_preserves_percent_triplets_in_values() {
    let (p, pt) = extract_pt("/fs?id=hello%20world%26more&_pt=x");
    assert_eq!(p, "/fs?id=hello%20world%26more");
    assert_eq!(pt, "x");
}

#[test]
fn extract_pt_valueless_param_kept_verbatim() {
    let (p, pt) = extract_pt("/fs?flag&_pt=x");
    assert_eq!(p, "/fs?flag");
    assert_eq!(pt, "x");
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
        super::handle(server_side, client_http).await;
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
    assert!(echoed_head.contains("c-id: client-1"), "custom header must be forwarded");
    assert!(
        !echoed_head.to_lowercase().contains("\r\norigin:"),
        "origin header must be stripped before forwarding, got: {echoed_head}"
    );
    echo.await.unwrap();
}
