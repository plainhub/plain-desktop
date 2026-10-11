use crate::assembly::server::ServerState;
use crate::host::query::Provider;
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    provider: Provider,
    ids: Vec<String>,
}
fn delete_permission(provider: &Provider) -> Option<&'static str> {
    match provider {
        Provider::Call => Some("WRITE_CALL_LOG"),
        Provider::Contact => Some("WRITE_CONTACTS"),
        _ => None,
    }
}
async fn execute(state: &ServerState, request: Request) -> anyhow::Result<Value> {
    if let Some(permission) = delete_permission(&request.provider) {
        let configured: Vec<String> = state
            .prefs
            .get_or(plain_rs::prefs::system::keys::API_PERMISSIONS, Vec::new());
        anyhow::ensure!(
            configured.iter().any(|name| name == permission),
            "no_permission"
        );
    }
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
        Provider::Call => plain_rs::enums::DataType::Call.kind(),
        Provider::Contact => plain_rs::enums::DataType::Contact.kind(),
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
#[cfg(test)]
#[path = "../../tests/unit/mobile/deletes.rs"]
mod tests;
