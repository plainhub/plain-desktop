use super::server::ServerState;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const RECEIPTS_KEY: &str = "sms_send_receipts";
const RECEIPT_TTL_MS: u64 = 24 * 60 * 60 * 1000;
const MAX_RECEIPTS: usize = 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Receipt {
    key: String,
    payload_hash: String,
    created_at_ms: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
    number: String,
    body: String,
    subscription_id: Option<i32>,
    client_id: Option<String>,
    client_request_id: Option<String>,
}

fn permission_allowed(configured: &[String]) -> bool {
    configured.iter().any(|name| name == "SEND_SMS")
}

fn digest(bytes: &[u8]) -> String {
    crate::utils::hex::bytes_to_hex(&Sha256::digest(bytes))
}

fn claim(
    receipts: &mut Vec<Receipt>,
    key: &str,
    payload_hash: &str,
    now_ms: u64,
) -> anyhow::Result<bool> {
    receipts.retain(|receipt| now_ms.saturating_sub(receipt.created_at_ms) < RECEIPT_TTL_MS);
    if let Some(existing) = receipts.iter().find(|receipt| receipt.key == key) {
        anyhow::ensure!(
            existing.payload_hash == payload_hash,
            "requestId was already used for a different SMS"
        );
        return Ok(false);
    }
    if receipts.len() >= MAX_RECEIPTS {
        let remove_count = receipts.len() - MAX_RECEIPTS + 1;
        receipts.drain(..remove_count);
    }
    receipts.push(Receipt {
        key: key.to_owned(),
        payload_hash: payload_hash.to_owned(),
        created_at_ms: now_ms,
    });
    Ok(true)
}

async fn execute(state: &ServerState, request: Request) -> anyhow::Result<Value> {
    anyhow::ensure!(!request.number.trim().is_empty(), "phone number is empty");
    let configured: Vec<String> = state.prefs.get_or("api_permissions", Vec::new());
    anyhow::ensure!(permission_allowed(&configured), "no_permission");
    let request_id = request
        .client_request_id
        .as_deref()
        .filter(|value| !value.trim().is_empty());
    let receipt_key = request_id.map(|request_id| {
        digest(
            format!(
                "{}\0{}",
                request.client_id.as_deref().unwrap_or_default(),
                request_id
            )
            .as_bytes(),
        )
    });
    if let Some(key) = &receipt_key {
        let _guard = state.sms_send_lock.lock().await;
        let payload_hash = digest(
            serde_json::to_vec(&json!({
                "number": &request.number,
                "body": &request.body,
                "subscriptionId": request.subscription_id,
            }))?
            .as_slice(),
        );
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as u64;
        let mut receipts: Vec<Receipt> = state.prefs.get_or(RECEIPTS_KEY, Vec::new());
        if !claim(&mut receipts, key, &payload_hash, now_ms)? {
            return Ok(json!({"sent":true,"duplicate":true}));
        }
        // Persist the idempotency claim before asking the platform to dispatch.
        // This makes an ambiguous timeout/retry at-most-once across restarts.
        state.prefs.set(RECEIPTS_KEY, &receipts)?;
    }
    state
        .host
        .call(
            "systemSendSms",
            json!({
                "number": request.number,
                "body": request.body,
                "subscriptionId": request.subscription_id,
                "clientId": request.client_id,
                "clientRequestId": request.client_request_id,
            }),
        )
        .await
        .map_err(anyhow::Error::msg)?;
    Ok(json!({"sent":true}))
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

#[cfg(test)]
mod tests {
    use super::{Receipt, claim, digest, permission_allowed};

    #[test]
    fn sms_send_requires_explicit_api_permission() {
        assert!(!permission_allowed(&[]));
        assert!(!permission_allowed(&["READ_SMS".into()]));
        assert!(permission_allowed(&["SEND_SMS".into()]));
    }

    #[test]
    fn request_id_claim_deduplicates_matching_payload_and_rejects_reuse() {
        let mut receipts = Vec::<Receipt>::new();
        assert!(claim(&mut receipts, "client-request", "payload-a", 100).unwrap());
        assert!(!claim(&mut receipts, "client-request", "payload-a", 101).unwrap());
        assert!(claim(&mut receipts, "client-request", "payload-b", 102).is_err());
    }

    #[test]
    fn idempotency_digests_do_not_store_sms_content() {
        assert_ne!(digest(b"number\0request"), digest(b"number\0other"));
        assert_ne!(digest(b"body-a"), digest(b"body-b"));
    }
}
