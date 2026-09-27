use async_graphql::{Context, Object};
use std::sync::Arc;

use super::super::context::AppCtx;

#[derive(Default)]
pub struct DiscoverMutation;

#[Object]
impl DiscoverMutation {
    /// Mirrors plain-app's `startDiscovery` mutation: kicks off the
    /// background scan loop that pushes discovered devices over the
    /// local server WS as `WS_NEARBY_DEVICE_FOUND`. Returns `true` if
    /// the loop was started, `false` if it was already running.
    async fn start_discovery(&self, ctx: &Context<'_>) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.discover_manager.start_discovery()
    }

    /// Mirrors plain-app's `stopDiscovery` mutation: tears down the
    /// background scan loop started by `startDiscovery`. Returns
    /// `true` if a loop was cancelled, `false` if none was active.
    async fn stop_discovery(&self, ctx: &Context<'_>) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.discover_manager.stop_discovery()
    }
}

#[derive(Default)]
pub struct DiscoverQuery;

#[Object]
impl DiscoverQuery {
    /// Mirrors plain-app's `isDiscovering` query: returns `true`
    /// while the background scan loop is running.
    async fn is_discovering(&self, ctx: &Context<'_>) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.discover_manager.is_discovering()
    }
}
