use super::types::NearbyDevice;
use crate::assembly::server::ServerState;
use async_graphql::{Context, Object};
use std::collections::HashSet;

#[derive(Default)]
pub struct DiscoveryHostQuery;

#[Object]
impl DiscoveryHostQuery {
    async fn nearby_devices(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<NearbyDevice>> {
        let state = ctx.data::<ServerState>()?;
        nearby_snapshot(state)
    }

    /// Whether the manual discovery scan loop is running. The nearby page
    /// polls this and restarts the scan when another client stopped it.
    async fn is_discovering(&self, ctx: &Context<'_>) -> bool {
        ctx.data::<ServerState>()
            .is_ok_and(|state| state.mdns.manual_scanning())
    }
}

#[derive(Default)]
pub struct DiscoveryHostMutation;

#[Object]
impl DiscoveryHostMutation {
    async fn start_discovery(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<NearbyDevice>> {
        let state = ctx.data::<ServerState>()?;
        state
            .mdns
            .execute(
                state,
                crate::services::discovery::mdns_runtime::Request::Start {},
            )
            .await?;
        nearby_snapshot(state)
    }

    async fn stop_discovery(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        let state = ctx.data::<ServerState>()?;
        let active = state.mdns.manual_scanning();
        state
            .mdns
            .execute(
                state,
                crate::services::discovery::mdns_runtime::Request::Stop {},
            )
            .await?;
        Ok(active)
    }
}

fn nearby_snapshot(state: &ServerState) -> async_graphql::Result<Vec<NearbyDevice>> {
    let paired: HashSet<String> = plain_rs::db::chat_store::peers::all(&state.db)?
        .into_iter()
        .filter(|peer| peer.is_paired())
        .map(|peer| peer.id)
        .collect();
    Ok(state
        .nearby_devices
        .snapshot()
        .devices
        .into_iter()
        .map(|device| {
            let mut model = NearbyDevice::from_snapshot_device(device);
            if paired.contains(&model.id) {
                model.status = "PAIRED".to_owned();
            }
            model
        })
        .collect())
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphql/resolvers/discovery/host.rs"]
mod tests;
