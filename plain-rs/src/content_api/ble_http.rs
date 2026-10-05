use super::server::ServerState;
use axum::{
    Json,
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    body: String,
    remote_host: String,
    headers: std::collections::BTreeMap<String, String>,
}
#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "m", default = "method")]
    method: String,
    #[serde(rename = "p")]
    path: String,
    #[serde(rename = "q", default)]
    query: std::collections::BTreeMap<String, Vec<String>>,
    #[serde(rename = "b", default)]
    body: String,
    #[serde(rename = "bb", default)]
    binary: bool,
}
fn method() -> String {
    "GET".into()
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let result: anyhow::Result<Value> = async {
        let envelope: Envelope = serde_json::from_str(&request.body)?;
        anyhow::ensure!(
            envelope.path.starts_with('/') && !envelope.path.starts_with("//"),
            "Invalid BLE HTTP path"
        );
        let mut url = reqwest::Url::parse("http://localhost/")?;
        url.set_path(&envelope.path);
        for (key, values) in envelope.query {
            for value in values {
                url.query_pairs_mut().append_pair(&key, &value);
            }
        }
        let path = format!(
            "{}{}",
            url.path(),
            url.query().map(|q| format!("?{q}")).unwrap_or_default()
        );
        let body = if envelope.binary {
            super::peer_sdk::decode_bytes(&envelope.body)?
        } else {
            envelope.body.into_bytes()
        };
        let mut builder = axum::http::Request::builder()
            .method(
                if envelope.method.trim().is_empty() {
                    "GET".to_string()
                } else {
                    envelope.method.to_uppercase()
                }
                .as_str(),
            )
            .uri(path);
        for (name, value) in request.headers {
            builder = builder.header(name, value);
        }
        let mut req = builder.body(Body::from(body))?;
        req.extensions_mut()
            .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                [127, 0, 0, 1],
                0,
            ))));
        #[cfg(feature = "http_transport")]
        req.extensions_mut()
            .insert(super::http_bridge::RemoteHost(request.remote_host));
        #[cfg(feature = "http_transport")]
        req.extensions_mut()
            .insert(crate::http_transport::ConnectionScheme("https"));
        use tower::ServiceExt;
        let response = super::server::peer_router(state).oneshot(req).await?;
        let status = response.status().as_u16();
        let headers: std::collections::BTreeMap<_, _> = response
            .headers()
            .iter()
            .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_owned())))
            .collect();
        let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024).await?;
        Ok(json!({"s":status,"h":headers,"b":crate::base64_encode(&bytes)}))
    }
    .await;
    match result {
        Ok(value) => Json(json!({"result":value.to_string()})).into_response(),
        Err(error) => Json(json!({"result":json!({"s":400,"h":{"content-type":"text/plain"},"b":crate::base64_encode(error.to_string().as_bytes())}).to_string()})).into_response(),
    }
}
