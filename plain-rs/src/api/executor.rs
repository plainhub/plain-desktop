use serde_json::{json, Value};
use std::sync::Arc;

use super::context::AppCtx;
use crate::http_server::main_schemas::ApiSchema;

pub async fn execute_graphql(
    schema: &ApiSchema,
    request: Value,
    ctx: Arc<AppCtx>,
    cid: String,
) -> Value {
    let request = match serde_json::from_value::<async_graphql::Request>(request) {
        Ok(request) => request,
        Err(_) => {
            return serde_json::to_value(async_graphql::Response::from_errors(vec![
                async_graphql::ServerError::new("Bad request", None),
            ]))
            .unwrap_or_else(|_| json!({ "data": null }));
        }
    };
    let request = request
        .data(ctx.clone())
        .data(cid)
        .data(ctx.media.db.clone())
        .data(ctx.prefs.clone())
        .data(ctx.pomodoro.clone())
        .data(ctx.image_updates.clone())
        .data(ctx.event_tx.clone())
        .data(ctx.db.clone());
    let response = schema.execute(request).await;

    serde_json::to_value(response).unwrap_or_else(|_| json!({ "data": null }))
}

#[cfg(all(test, feature = "system"))]
#[path = "../../tests/unit/api/executor.rs"]
mod tests;
