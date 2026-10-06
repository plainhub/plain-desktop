//! The web-facing permission gate, shared by the host provider route and
//! the public `/graphql` schema.
//!
//! "Enabled" means the user handed this client the API permission in the
//! `api_permissions` pref — a per-app opt-in, independent of the Android
//! runtime grant. The runtime grant is only re-checked when the caller asks
//! for it (`require_granted`), which plain-app's
//! `Permission.enabledAndIsGrantedAsync` does and `checkEnabledAsync` does not.

use crate::{content_api::host::Host, prefs::Prefs};
use async_graphql::Context;
use serde_json::{Value, json};
use std::{collections::HashSet, sync::Arc};

/// `WRITE_CONTACTS` implies `READ_CONTACTS` and `WRITE_CALL_LOG` implies
/// `READ_CALL_LOG`: the web UI only ever offers the wider permission, so
/// asking for the narrower one must not fail.
pub(super) fn api_enabled(permissions: &[String], configured: &HashSet<String>) -> bool {
    permissions.iter().all(|name| {
        configured.contains(name)
            || (name == "READ_CONTACTS" && configured.contains("WRITE_CONTACTS"))
            || (name == "READ_CALL_LOG" && configured.contains("WRITE_CALL_LOG"))
    })
}

fn configured(prefs: &Prefs) -> Vec<String> {
    prefs.get_or("api_permissions", Vec::new())
}

pub(super) async fn granted(host: &Host, permissions: &[String]) -> anyhow::Result<bool> {
    let facts = host
        .call(
            "systemPermissionFacts",
            json!({ "permissions": permissions }),
        )
        .await
        .map_err(anyhow::Error::msg)?;
    let grants = facts["granted"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("invalid permission facts"))?;
    Ok(permissions
        .iter()
        .all(|name| grants.get(name).and_then(Value::as_bool) == Some(true)))
}

pub(super) fn is_enabled(prefs: &Prefs, permissions: &[&str]) -> bool {
    let names: Vec<String> = permissions.iter().map(|name| name.to_string()).collect();
    let configured: HashSet<String> = configured(prefs).into_iter().collect();
    api_enabled(&names, &configured)
}

/// plain-app's `checkEnabledAsync`: error out unless every named permission
/// was handed to the web client.
pub(super) fn require(prefs: &Prefs, permissions: &[&str]) -> async_graphql::Result<()> {
    if is_enabled(prefs, permissions) {
        Ok(())
    } else {
        Err(async_graphql::Error::new("no_permission"))
    }
}

/// plain-app's `Permission.enabledAndIsGrantedAsync`: the pref opt-in *and*
/// the platform grant. Used where plain-app degrades to an empty result
/// instead of erroring (`packageCount`).
pub(super) async fn require_granted(
    ctx: &Context<'_>,
    permissions: &[&str],
) -> anyhow::Result<bool> {
    if !is_enabled(ctx.data_unchecked::<Arc<Prefs>>(), permissions) {
        return Ok(false);
    }
    granted(
        ctx.data_unchecked::<Arc<Host>>(),
        &permissions
            .iter()
            .map(|name| name.to_string())
            .collect::<Vec<_>>(),
    )
    .await
}
