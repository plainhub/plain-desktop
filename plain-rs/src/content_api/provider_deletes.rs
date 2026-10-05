use super::{provider_plan::Provider, server::ServerState};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    provider: Provider,
    ids: Vec<String>,
}
async fn execute(state: &ServerState, request: Request) -> anyhow::Result<Value> {
    let requested = request
        .ids
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    if requested.is_empty() {
        return Ok(json!({"count":0}));
    }
    let receipt = state
        .host
        .call(
            "systemDeleteRecords",
            json!({"provider":request.provider,"ids":requested}),
        )
        .await
        .map_err(anyhow::Error::msg)?;
    let completed: Vec<String> = serde_json::from_value(receipt)?;
    let completed = completed
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    anyhow::ensure!(
        completed.is_subset(&requested),
        "invalid provider deletion receipt"
    );
    let kind = match request.provider {
        Provider::Call => crate::enums::DataType::Call.kind(),
        Provider::Contact => crate::enums::DataType::Contact.kind(),
        _ => anyhow::bail!("unsupported delete provider"),
    };
    state.db.with_conn(|c| -> rusqlite::Result<()> {
        let tx = c.unchecked_transaction()?;
        for id in &completed {
            tx.execute(
                "DELETE FROM tag_relations WHERE type=?1 AND key=?2",
                rusqlite::params![kind, id],
            )?;
        }
        tx.commit()
    })?;
    Ok(json!({"count":completed.len()}))
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match execute(&state, request).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}
