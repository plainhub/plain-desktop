use super::server::ServerState;
use anyhow::{Result, ensure};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Download {
    url: String,
}
pub(super) fn download_url(raw: &str) -> Result<reqwest::Url> {
    let url = reqwest::Url::parse(raw)?;
    ensure!(
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none()
            && matches!(url.path(), "/fs" | "/zip/dir"),
        "Invalid shared download URL"
    );
    let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
    ensure!(
        query.get("sid").is_some_and(|id| !id.is_empty())
            && query.get("id").is_some_and(|id| !id.is_empty()),
        "Missing shared file ID"
    );
    Ok(url)
}
pub(super) async fn file(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Download>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut stop = state.stop.clone();
    if *stop.borrow() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let body_stop = stop.clone();
    let work = async {
        let url = download_url(&request.url)?;
        let permit = state.lan.capacity.clone().acquire_owned().await?;
        let response =
            tokio::time::timeout(Duration::from_secs(10), state.lan.client.get(url).send())
                .await??;
        ensure!(
            response.status().is_success(),
            "Shared file HTTP {}",
            response.status()
        );
        super::peer_lan::streaming(response, permit, body_stop)
    };
    match tokio::select! {_=stop.changed()=>Err(anyhow::anyhow!("Core stopped")),result=work=>result}
    {
        Ok(response) => response,
        Err(error) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}
