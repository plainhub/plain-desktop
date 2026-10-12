use crate::graphql::executor::GraphqlExecutor;
use async_graphql::{Request, Response};
use plain_rs::events::WsEvent;
use tokio::sync::{broadcast, watch};

#[derive(Clone)]
pub struct NativeGraphqlExecutor {
    executor: GraphqlExecutor,
    events: broadcast::Sender<WsEvent>,
    client_id: String,
    stop: watch::Receiver<bool>,
}

impl NativeGraphqlExecutor {
    pub(crate) fn new(
        executor: GraphqlExecutor,
        events: broadcast::Sender<WsEvent>,
        client_id: String,
        stop: watch::Receiver<bool>,
    ) -> Self {
        Self {
            executor,
            events,
            client_id,
            stop,
        }
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<WsEvent> {
        self.events.subscribe()
    }

    pub async fn wait_stopped(&self) {
        let mut stop = self.stop.clone();
        let _ = stop.wait_for(|stopped| *stopped).await;
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub async fn execute(&self, request: Request) -> Response {
        self.executor.execute(request, false).await
    }
    pub async fn execute_private(&self, request: Request) -> Response {
        self.executor.execute(request, true).await
    }
}
