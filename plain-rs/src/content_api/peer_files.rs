use super::server::ServerState;
use axum::{
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::net::{Ipv4Addr, SocketAddr};
pub(super) async fn file(State(state): State<ServerState>, mut request: Request) -> Response {
    let query = request.uri().to_string();
    let id = crate::utils::query::query_get(&query, "id").unwrap_or_default();
    let decoded = crate::xchacha_decrypt(
        &crate::prefs::ensure_url_token(&state.prefs),
        &crate::base64_decode(&id),
    )
    .and_then(|v| String::from_utf8(v).ok());
    let Some(uri) = decoded else {
        return forward(state, request).await;
    };
    let Some(suffix) = uri.strip_prefix("fid:") else {
        return forward(state, request).await;
    };
    if !state.prefs.get_user_or("service", false) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let store = crate::app_files::FileStore::new(state.db.clone(), state.directory.clone());
    let Ok(path) = store.resolve(suffix) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(record) = state
        .db
        .get_app_file(suffix.split('.').next().unwrap_or_default())
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if crate::utils::query::query_get(&query, "w").is_some()
        && crate::utils::query::query_get(&query, "h").is_some()
    {
        return forward(state, request).await;
    }
    let offset = crate::utils::query::query_get(&query, "offset");
    let length = crate::utils::query::query_get(&query, "length");
    if let (Some(offset), Some(length)) = (offset, length) {
        let (Ok(offset), Ok(length)) = (offset.parse::<u64>(), length.parse::<u64>()) else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        if offset >= record.size as u64 {
            return (StatusCode::OK, Vec::<u8>::new()).into_response();
        }
        if length == 0 {
            return (StatusCode::OK, Vec::<u8>::new()).into_response();
        }
        let Some(end) = offset.checked_add(length - 1) else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        let Ok(range) = format!("bytes={offset}-{end}").parse() else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        request.headers_mut().insert("range", range);
        let mut response =
            super::server::files::stream_file(&path, &record.mime_type, request.headers()).await;
        if response.status() == StatusCode::PARTIAL_CONTENT {
            *response.status_mut() = StatusCode::OK;
        }
        response
    } else {
        super::server::files::stream_file(&path, &record.mime_type, request.headers()).await
    }
}
async fn forward(state: ServerState, request: Request) -> Response {
    #[cfg(feature = "http_transport")]
    {
        super::http_bridge::handle(
            State(super::http_bridge::HttpBridgeState {
                bridge: state.bridge.clone(),
                stop: state.stop.clone(),
                prefs: state.prefs.clone(),
            }),
            request
                .extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .cloned()
                .unwrap_or(ConnectInfo(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))),
            axum::Extension(
                request
                    .extensions()
                    .get::<crate::http_transport::ConnectionScheme>()
                    .cloned()
                    .unwrap_or(crate::http_transport::ConnectionScheme("https")),
            ),
            None,
            request,
        )
        .await
    }
    #[cfg(not(feature = "http_transport"))]
    {
        let _ = (state, request);
        StatusCode::NOT_FOUND.into_response()
    }
}
