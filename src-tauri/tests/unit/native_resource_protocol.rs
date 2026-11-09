use super::*;

#[test]
fn resource_urls_use_the_webviews_registered_transport_and_preserve_the_handle() {
    let url = resource_url("opaque-handle");
    #[cfg(target_os = "windows")]
    assert_eq!(url, "http://plain-resource.file/opaque-handle");
    #[cfg(not(target_os = "windows"))]
    assert_eq!(url, "plain-resource://file/opaque-handle");
    let request = Request::builder().uri(url).body(Vec::new()).unwrap();
    assert_eq!(requested_id(&request).as_deref(), Some("opaque-handle"));
}

#[test]
fn absent_or_traversing_resource_handles_are_rejected() {
    for uri in ["plain-resource://file/", "plain-resource://file/.."] {
        let request = Request::builder().uri(uri).body(Vec::new()).unwrap();
        assert!(requested_id(&request).is_none());
    }
}

struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new(bytes: &[u8]) -> Self {
        let path = std::env::temp_dir().join(format!(
            "plain-resource-{}",
            plain_rs::features::record_id::generate()
        ));
        std::fs::write(&path, bytes).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn request(url: &str, method: Method, range: Option<&str>) -> Request<Vec<u8>> {
    let mut builder = Request::builder().uri(url).method(method);
    if let Some(range) = range {
        builder = builder.header(header::RANGE, range);
    }
    builder.body(Vec::new()).unwrap()
}

#[tokio::test]
async fn responses_preserve_exact_range_bytes_and_reject_foreign_or_closed_scopes() {
    let fixture = Fixture::new(b"0123456789");
    let registry = ResourceRegistry::default();
    let scope = NativeRequestScope::default();
    let opened = registry.open(&scope, &fixture.0).await.unwrap();
    let response = resource_response(
        &registry,
        &scope,
        request(&opened.url, Method::GET, Some("bytes=2-5")),
    )
    .await;
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes 2-5/10");
    assert_eq!(response.headers()[header::CONTENT_LENGTH], "4");
    assert_eq!(response.body(), b"2345");
    let other = NativeRequestScope::default();
    let denied =
        resource_response(&registry, &other, request(&opened.url, Method::GET, None)).await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    assert!(denied.body().is_empty());
    scope.close();
    assert_eq!(
        resource_response(&registry, &scope, request(&opened.url, Method::GET, None))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn large_resources_expose_head_and_bounded_ranges_without_reading_a_full_response() {
    let fixture = Fixture::new(b"");
    let size = MAX_RESPONSE_BYTES + 1;
    std::fs::OpenOptions::new()
        .write(true)
        .open(&fixture.0)
        .unwrap()
        .set_len(size)
        .unwrap();
    let registry = ResourceRegistry::default();
    let scope = NativeRequestScope::default();
    let opened = registry.open(&scope, &fixture.0).await.unwrap();
    let head = resource_response(
        &registry,
        &scope,
        request(&opened.url, Method::HEAD, Some("bytes=0-4")),
    )
    .await;
    assert_eq!(head.status(), StatusCode::OK);
    assert_eq!(head.headers()[header::CONTENT_LENGTH], size.to_string());
    assert!(head.body().is_empty());
    assert!(!head.headers().contains_key(header::CONTENT_RANGE));
    let full = resource_response(&registry, &scope, request(&opened.url, Method::GET, None)).await;
    assert_eq!(full.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert!(full.body().is_empty());
    let bounded = resource_response(
        &registry,
        &scope,
        request(&opened.url, Method::GET, Some("bytes=0-65535")),
    )
    .await;
    assert_eq!(bounded.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(bounded.body(), &vec![0; 65536]);
    let invalid = resource_response(
        &registry,
        &scope,
        request(&opened.url, Method::GET, Some(&format!("bytes={size}-"))),
    )
    .await;
    assert_eq!(invalid.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(
        invalid.headers()[header::CONTENT_RANGE],
        format!("bytes */{size}")
    );
}

#[tokio::test]
async fn empty_resources_and_unsupported_methods_do_not_invent_bytes() {
    let fixture = Fixture::new(b"");
    let registry = ResourceRegistry::default();
    let scope = NativeRequestScope::default();
    let opened = registry.open(&scope, &fixture.0).await.unwrap();
    let empty = resource_response(&registry, &scope, request(&opened.url, Method::GET, None)).await;
    assert_eq!(empty.status(), StatusCode::OK);
    assert_eq!(empty.headers()[header::CONTENT_LENGTH], "0");
    assert!(empty.body().is_empty());
    assert_eq!(
        resource_response(&registry, &scope, request(&opened.url, Method::POST, None))
            .await
            .status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
}
