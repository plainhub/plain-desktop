use super::server::ServerState;
use axum::{
    extract::{Request, State},
    http::{Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

pub(super) async fn apply(
    State(state): State<ServerState>,
    request: Request,
    next: Next,
) -> Response {
    let origin = request.headers().get(header::ORIGIN).cloned();
    let mut allow_headers = None;
    if let Some(origin) = &origin {
        let scheme = request
            .extensions()
            .get::<crate::http_transport::ConnectionScheme>()
            .map_or("http", |scheme| scheme.0);
        let host = request
            .headers()
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        let same_origin = origin
            .to_str()
            .ok()
            .and_then(|origin| reqwest::Url::parse(origin).ok())
            .zip(reqwest::Url::parse(&format!("{scheme}://{host}")).ok())
            .is_some_and(|(origin, authority)| {
                origin.scheme() == authority.scheme()
                    && origin.host_str() == authority.host_str()
                    && origin.port_or_known_default() == authority.port_or_known_default()
            });
        if !same_origin
            && !state.prefs.get_user_or("allow_any_host", false)
            && !state.build_debug.load(std::sync::atomic::Ordering::Relaxed)
        {
            return StatusCode::FORBIDDEN.into_response();
        }
        allow_headers = request
            .headers()
            .get(header::ACCESS_CONTROL_REQUEST_HEADERS)
            .and_then(|v| v.to_str().ok())
            .map(|value| {
                value
                    .split(',')
                    .map(|value| value.trim().to_ascii_lowercase())
                    .filter(|value| {
                        matches!(value.as_str(), "content-type" | "authorization" | "accept")
                            || value.starts_with("c-")
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            });
    }
    let options = origin.is_some() && request.method() == Method::OPTIONS;
    let mut response = if options {
        StatusCode::OK.into_response()
    } else {
        next.run(request).await
    };
    let headers = response.headers_mut();
    headers.insert(
        "x-server-time",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|v| v.as_millis())
            .unwrap_or_default()
            .to_string()
            .parse()
            .unwrap(),
    );
    headers.insert("cross-origin-opener-policy", "same-origin".parse().unwrap());
    headers.insert(
        "cross-origin-embedder-policy",
        "credentialless".parse().unwrap(),
    );
    if let Some(origin) = origin {
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
        headers.append(header::VARY, "Origin".parse().unwrap());
        if options {
            headers.insert(
                header::ACCESS_CONTROL_ALLOW_METHODS,
                "GET, POST, PUT, DELETE, OPTIONS, HEAD".parse().unwrap(),
            );
            headers.insert(header::ACCESS_CONTROL_MAX_AGE, "86400".parse().unwrap());
            if let Some(value) = allow_headers.filter(|v| !v.is_empty()) {
                if let Ok(value) = value.parse() {
                    headers.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, value);
                }
            }
        }
    }
    response
}
