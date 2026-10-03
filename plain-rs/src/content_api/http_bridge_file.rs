use axum::{
    http::{HeaderMap, StatusCode},
    response::Response,
};
pub async fn serve(path: &str, headers: &HeaderMap, packet: &serde_json::Value) -> Response {
    let mime = packet["contentType"]
        .as_str()
        .unwrap_or("application/octet-stream");
    let mut response =
        super::server::files::stream_file(std::path::Path::new(path), mime, headers).await;
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        "no-store".parse().unwrap(),
    );
    if packet["dlna"].as_bool() == Some(true) && response.status() == StatusCode::OK {
        *response.status_mut() = StatusCode::PARTIAL_CONTENT;
    }
    response
}
