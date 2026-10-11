use crate::assembly::server::ServerState;
use axum::{Router, routing::post};
pub(crate) fn routes(state: ServerState) -> Router {
    Router::new()
        .route("/system/sms-state", post(crate::mobile::sms::state::call))
        .route("/system/sms", post(crate::mobile::sms::query::call))
        .with_state(state)
}
