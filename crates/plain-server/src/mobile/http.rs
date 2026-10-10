use crate::assembly::server::ServerState;
use axum::{Router, routing::post};
pub(crate) fn routes(state: ServerState) -> Router {
    Router::new()
        .route("/system/sms-state", post(crate::mobile::sms::state::call))
        .route("/system/sms", post(crate::mobile::sms::query::call))
        .route("/system/sms-send", post(crate::mobile::sms::send::call))
        .route(
            "/system/contact-write",
            post(crate::mobile::contacts::write::call),
        )
        .route("/system/mms-runtime", post(crate::mobile::sms::mms::call))
        .route(
            "/system/provider-delete",
            post(crate::mobile::deletes::call),
        )
        .with_state(state)
}
