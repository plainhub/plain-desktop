use super::server::ServerState;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

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

async fn execute(state: &ServerState, request: Request) -> anyhow::Result<Value> {
    anyhow::ensure!(!request.number.trim().is_empty(), "phone number is empty");
    let configured: Vec<String> = state.prefs.get_or("api_permissions", Vec::new());
    anyhow::ensure!(permission_allowed(&configured), "no_permission");
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
    use super::permission_allowed;

    #[test]
    fn sms_send_requires_explicit_api_permission() {
        assert!(!permission_allowed(&[]));
        assert!(!permission_allowed(&["READ_SMS".into()]));
        assert!(permission_allowed(&["SEND_SMS".into()]));
    }
}
