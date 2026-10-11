use crate::assembly::server::ServerState;
use async_graphql::{ObjectType, Request, Response, Schema, SubscriptionType};
use plain_rs::events::WsEvent;

/// Where a GraphQL request came from, injected by the adapter that received
/// it — never by the document or its variables.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RequestOrigin {
    /// This device, over the native bridge. Holds real OS grants and file
    /// authorization; must not be judged by the web API permission list.
    Local,
    /// Another device, over HTTP. Answerable only where the user handed the
    /// web client the matching `api_permissions` entry.
    Remote,
}

pub struct RequestContext {
    state: Option<ServerState>,
    notify_changes: bool,
    origin: RequestOrigin,
}
impl RequestContext {
    pub(crate) fn local(state: ServerState) -> Self {
        Self {
            state: Some(state),
            notify_changes: true,
            origin: RequestOrigin::Local,
        }
    }
    pub(crate) fn remote(state: Option<ServerState>) -> Self {
        Self {
            state,
            notify_changes: false,
            origin: RequestOrigin::Remote,
        }
    }
    pub(crate) fn origin(&self) -> RequestOrigin {
        self.origin
    }
}
pub async fn execute_graphql<
    Q: ObjectType + 'static,
    M: ObjectType + 'static,
    S: SubscriptionType + 'static,
>(
    schema: &Schema<Q, M, S>,
    request: Request,
    context: RequestContext,
) -> Response {
    let changed = context.notify_changes && content_changes(&request);
    let origin = context.origin();
    let request = match context.state.as_ref() {
        Some(state) => request.data(state.clone()).data(origin),
        None => request.data(origin),
    };
    let response = schema.execute(request).await;
    if changed && response.errors.is_empty() {
        if let Some(state) = context.state {
            let _ = state
                .events
                .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()));
        }
    }
    response
}

pub(crate) fn content_changes(request: &Request) -> bool {
    use async_graphql::parser::{
        parse_query,
        types::{OperationType, Selection},
    };
    let Ok(document) = parse_query(&request.query) else {
        return false;
    };
    document
        .operations
        .iter()
        .filter(|(name, _)| {
            request
                .operation_name
                .as_deref()
                .is_none_or(|selected| name.is_some_and(|n| n.as_str() == selected))
        })
        .any(|(_, operation)| {
            operation.node.ty == OperationType::Mutation
                && operation
                    .node
                    .selection_set
                    .node
                    .items
                    .iter()
                    .any(|selection| match &selection.node {
                        Selection::Field(field) => !matches!(
                            field.node.name.node.as_str(),
                            "broadcastImageEditorUpdate"
                                | "configurePomodoroDay"
                                | "tickPomodoro"
                                | "skipPomodoro"
                                | "adjustPomodoro"
                                | "startPomodoro"
                                | "pausePomodoro"
                                | "stopPomodoro"
                        ),
                        _ => true,
                    })
        })
}

#[derive(Clone)]
pub struct GraphqlExecutor {
    state: ServerState,
}
impl GraphqlExecutor {
    pub(crate) fn new(state: ServerState) -> Self {
        Self { state }
    }
    pub async fn execute(&self, request: Request, host_api: bool) -> Response {
        let context = RequestContext::local(self.state.clone());
        if host_api {
            execute_graphql(&self.state.schema, request, context).await
        } else {
            execute_graphql(&self.state.public, request, context).await
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/graphql/executor.rs"]
mod tests;
