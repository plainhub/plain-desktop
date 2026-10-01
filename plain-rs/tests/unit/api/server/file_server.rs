use super::*;

fn temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("plain-file-server-test-{name}"))
}

#[tokio::test]
async fn unsatisfiable_range_responds_416_with_content_range() {
    let resp = unsatisfiable_range_response(10);
    assert_eq!(resp.status(), axum::http::StatusCode::RANGE_NOT_SATISFIABLE);
    let headers = resp.headers();
    assert_eq!(headers.get("content-length").unwrap(), "0");
    assert_eq!(headers.get("content-range").unwrap(), "bytes */10");
    assert_eq!(headers.get("accept-ranges").unwrap(), "bytes");
}

#[tokio::test]
async fn partial_range_serves_206_with_requested_bytes() {
    let path = temp_path("206");
    tokio::fs::write(&path, b"0123456789").await.unwrap();
    let resp = partial_response(&path, 2, 4, 10, "video/mp4", "inline").await;
    assert_eq!(resp.status(), axum::http::StatusCode::PARTIAL_CONTENT);
    let headers = resp.headers();
    assert_eq!(headers.get("content-length").unwrap(), "3");
    assert_eq!(headers.get("content-range").unwrap(), "bytes 2-4/10");
    assert_eq!(headers.get("accept-ranges").unwrap(), "bytes");
    assert_eq!(
        headers.get("access-control-expose-headers").unwrap(),
        "content-disposition, accept-ranges, content-range"
    );
    let body = collect_body(resp).await;
    assert_eq!(body, b"234");
    let _ = tokio::fs::remove_file(&path).await;
}

#[tokio::test]
async fn full_response_streams_whole_file() {
    let path = temp_path("200");
    tokio::fs::write(&path, b"hello world").await.unwrap();
    let resp = full_response(&path, 11, "text/plain", "inline").await;
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let headers = resp.headers();
    assert_eq!(headers.get("content-length").unwrap(), "11");
    assert_eq!(headers.get("content-type").unwrap(), "text/plain");
    assert_eq!(headers.get("accept-ranges").unwrap(), "bytes");
    let body = collect_body(resp).await;
    assert_eq!(body, b"hello world");
    let _ = tokio::fs::remove_file(&path).await;
}

/// The router-level `tower-http` CorsLayer adds the CORS set (mirroring
/// the request origin) to normal responses whenever the request carries
/// an `Origin` header — replacing the per-response hardcoded headers.
#[tokio::test]
async fn cors_layer_adds_allow_origin_to_streaming_responses() {
    use axum::body::Body;
    use tower::ServiceExt;

    let svc = tower::ServiceBuilder::new()
        .layer(crate::server::cors::layer(
            &crate::server::cors::CorsPolicy::permissive_default(),
        ))
        .service(tower::service_fn(|_: axum::extract::Request| async {
            Ok::<_, std::convert::Infallible>(unsatisfiable_range_response(10))
        }));
    let resp = svc
        .oneshot(
            axum::extract::Request::get("/fs")
                .header("origin", "https://app.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(
        resp.headers().get("access-control-allow-origin").unwrap(),
        "https://app.example"
    );
    assert!(
        resp.headers()
            .get("access-control-allow-credentials")
            .is_some()
    );
}

async fn collect_body(resp: axum::response::Response) -> Vec<u8> {
    use futures_util::StreamExt;
    let mut out = Vec::new();
    let mut stream = resp.into_body().into_data_stream();
    while let Some(chunk) = stream.next().await {
        out.extend_from_slice(&chunk.expect("body chunk"));
    }
    out
}
