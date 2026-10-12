use crate::assembly::context::AppCtx;
use crate::graphql::executor::{RequestContext, RequestOrigin, execute_graphql};
use crate::graphql::schemas::desktop::ApiSchema;
use async_graphql::{Request, Response};
use std::sync::Arc;

#[derive(Clone)]
pub struct DesktopGraphqlExecutor {
    schema: Arc<ApiSchema>,
    context: Arc<AppCtx>,
}

impl DesktopGraphqlExecutor {
    pub fn new(schema: Arc<ApiSchema>, context: Arc<AppCtx>) -> Self {
        Self { schema, context }
    }

    pub fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<plain_rs::events::WsEvent> {
        self.context.event_tx.subscribe()
    }

    pub fn client_id(&self) -> &str {
        &self.context.identity.client_id
    }

    pub async fn execute(&self, request: Request) -> Response {
        execute_graphql(
            self.schema.as_ref(),
            request.data(self.context.identity.client_id.clone()),
            RequestContext::desktop(self.context.clone(), RequestOrigin::Local),
        )
        .await
    }
}
