#[cfg(feature = "host-api")]
use crate::assembly::server::ServerState;
use async_graphql::{ObjectType, Request, Response, Schema, SubscriptionType};
#[cfg(feature = "host-api")]
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
    #[cfg(feature = "host-api")]
    state: Option<ServerState>,
    #[cfg(feature = "api")]
    desktop: Option<std::sync::Arc<crate::assembly::context::AppCtx>>,
    notify_changes: bool,
    origin: RequestOrigin,
}
impl RequestContext {
    #[cfg(feature = "host-api")]
    pub(crate) fn local(state: ServerState) -> Self {
        Self {
            #[cfg(feature = "api")]
            desktop: None,
            state: Some(state),
            notify_changes: true,
            origin: RequestOrigin::Local,
        }
    }
    #[cfg(feature = "host-api")]
    pub(crate) fn remote(state: Option<ServerState>) -> Self {
        Self {
            #[cfg(feature = "api")]
            desktop: None,
            state,
            notify_changes: false,
            origin: RequestOrigin::Remote,
        }
    }
    #[cfg(feature = "api")]
    pub(crate) fn desktop(
        context: std::sync::Arc<crate::assembly::context::AppCtx>,
        origin: RequestOrigin,
    ) -> Self {
        Self {
            #[cfg(feature = "host-api")]
            state: None,
            desktop: Some(context),
            notify_changes: false,
            origin,
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
    let request = request.data(origin);
    #[cfg(feature = "host-api")]
    let request = match context.state.as_ref() {
        Some(state) => request.data(state.clone()),
        None => request,
    };
    #[cfg(feature = "api")]
    let request = match context.desktop.as_ref() {
        Some(context) => request
            .data(context.clone())
            .data(context.media.db.clone())
            .data(context.prefs.clone())
            .data(context.pomodoro.clone())
            .data(context.image_updates.clone())
            .data(context.event_tx.clone())
            .data(context.db.clone()),
        None => request,
    };
    #[cfg(feature = "api")]
    let request = match context.desktop.as_ref() {
        Some(context) if origin == RequestOrigin::Local => {
            request.data(context.identity.client_id.clone())
        }
        _ => request,
    };
    let response = schema.execute(request).await;
    if changed && response.errors.is_empty() {
        #[cfg(feature = "host-api")]
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

#[cfg(feature = "host-api")]
#[derive(Clone)]
pub struct GraphqlExecutor {
    state: ServerState,
}
#[cfg(feature = "host-api")]
impl GraphqlExecutor {
    pub(crate) fn new(state: ServerState) -> Self {
        Self { state }
    }
    pub async fn execute(&self, request: Request, host_api: bool) -> Response {
        let client_id = match self.state.native_client_id() {
            Ok(client_id) => client_id,
            Err(error) => {
                return Response::from_errors(vec![async_graphql::ServerError::new(error, None)]);
            }
        };
        let request = request.data(client_id);
        let context = RequestContext::local(self.state.clone());
        if host_api {
            execute_graphql(&self.state.schema, request, context).await
        } else {
            execute_graphql(&self.state.public, request, context).await
        }
    }
}

#[cfg(all(test, feature = "host-api"))]
#[path = "../../tests/unit/graphql/executor.rs"]
mod tests;
