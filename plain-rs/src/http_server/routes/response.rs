//! Shared axum response helper. CORS headers come from the router-level
//! `tower-http` CorsLayer (`super::cors`), not from individual
//! responses.

use axum::body::Body;
use axum::http::StatusCode;
use axum::response::Response;

pub const APP_ID: &str = "com.ismartcoding.plain.desktop";

/// Build a plain response: status + content-type. The body is fully
/// owned, so hyper sets `content-length` itself.
pub fn respond(status: u16, body: Vec<u8>, content_type: &str) -> Response {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    Response::builder()
        .status(status)
        .header("content-type", content_type)
        .body(Body::from(body))
        .expect("static response")
}
