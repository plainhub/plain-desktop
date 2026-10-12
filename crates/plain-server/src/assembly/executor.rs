use serde_json::{Value, json};
use std::sync::Arc;

use crate::assembly::context::AppCtx;
use crate::graphql::schemas::desktop::ApiSchema;

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
    let response = crate::graphql::executor::execute_graphql(
        schema,
        request.data(cid),
        crate::graphql::executor::RequestContext::desktop(
            ctx,
            crate::graphql::executor::RequestOrigin::Remote,
        ),
    )
    .await;

    serde_json::to_value(response).unwrap_or_else(|_| json!({ "data": null }))
}

#[cfg(all(test, feature = "system"))]
#[path = "../../tests/unit/assembly/executor.rs"]
mod tests;
