//! Shared axum response helper: every response carries the three CORS
//! headers, matching the old hand-rolled `respond` framing.

use axum::body::Body;
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::Response;

pub const APP_ID: &str = "com.ismartcoding.plain.desktop";

/// The CORS headers added to every local API response. Values identical to
/// [`crate::utils::http::CORS`] (kept as raw bytes for the legacy
/// plain-desktop main-checkout copy).
const CORS_HEADERS: [(&[u8], &[u8]); 3] = [
    (b"access-control-allow-origin", b"*"),
    (
        b"access-control-allow-methods",
        b"GET, POST, PUT, DELETE, OPTIONS",
    ),
    (b"access-control-allow-headers", b"*"),
];

/// Build a plain response: status + content-type + the CORS set. The body
/// is fully owned, so hyper sets `content-length` itself.
pub fn respond(status: u16, body: Vec<u8>, content_type: &str) -> Response {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut builder = Response::builder()
        .status(status)
        .header("content-type", content_type);
    for (name, value) in CORS_HEADERS {
        let name = HeaderName::from_bytes(name).expect("valid cors header name");
        let value = HeaderValue::from_bytes(value).expect("valid cors header value");
        builder = builder.header(name, value);
    }
    builder.body(Body::from(body)).expect("static response")
}

/// The CORS header set as `(name, value)` string pairs — for handlers that
/// build their own `Response` (file serving, proxying) and must keep the
/// exact header shape the hand-rolled server sent.
pub fn cors_header_pairs() -> Vec<(&'static str, &'static str)> {
    CORS_HEADERS
        .iter()
        .map(|(k, v)| {
            (
                std::str::from_utf8(k).expect("utf-8 cors name"),
                std::str::from_utf8(v).expect("utf-8 cors value"),
            )
        })
        .collect()
}
