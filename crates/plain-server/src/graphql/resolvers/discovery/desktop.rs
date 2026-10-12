use async_graphql::{Context, Object};
use std::sync::Arc;

use crate::assembly::context::AppCtx;

pub use super::types::NearbyDevice;

#[derive(Default)]
pub struct DiscoverMutation;

#[Object]
impl DiscoverMutation {
    /// Mirrors plain-app's `startDiscovery` mutation: kicks off the
    /// background scan loop that pushes discovered devices over the
    /// local server WS as `WS_NEARBY_DEVICE_FOUND`. Returns the cached
    /// device list immediately so the caller can populate the UI without
    /// waiting for the first event. The list reflects the current
    /// `nearby_devices` cache; subsequent changes are broadcast on the
    /// events stream.
    async fn start_discovery(&self, ctx: &Context<'_>) -> Vec<NearbyDevice> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.discover_manager.start_discovery();
        let snapshot = c.app.nearby_devices.snapshot();
        let paired: std::collections::HashSet<String> =
            plain_rs::db::chat_store::peers::all(&c.app.db)
                .ok()
                .into_iter()
                .flatten()
                .filter(|p| p.is_paired())
                .map(|p| p.id)
                .collect();
        snapshot
            .devices
            .into_iter()
            .map(|device| {
                let mut decorated = NearbyDevice::from_snapshot_device(device);
                if paired.contains(&decorated.id) {
                    decorated.status = "PAIRED".to_string();
                }
                decorated
            })
            .collect()
    }

    /// Mirrors plain-app's `stopDiscovery` mutation: tears down the
    /// background scan loop started by `startDiscovery`. Returns `true`
    /// if a loop was cancelled, `false` if none was active.
    async fn stop_discovery(&self, ctx: &Context<'_>) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.discover_manager.stop_discovery()
    }
}

#[derive(Default)]
pub struct DiscoverQuery;

#[Object]
impl DiscoverQuery {
    /// Mirrors plain-app's `isDiscovering` query: returns `true` while
    /// the background scan loop is running.
    async fn is_discovering(&self, ctx: &Context<'_>) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.discover_manager.is_discovering()
    }

    /// Read-only mirror of the cached `nearby_devices` snapshot. Callers
    /// that only want the current list without restarting the scan loop
    /// use this; the events stream still drives incremental updates.
    async fn nearby_devices(&self, ctx: &Context<'_>) -> Vec<NearbyDevice> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let snapshot = c.app.nearby_devices.snapshot();
        let paired: std::collections::HashSet<String> =
            plain_rs::db::chat_store::peers::all(&c.app.db)
                .ok()
                .into_iter()
                .flatten()
                .filter(|p| p.is_paired())
                .map(|p| p.id)
                .collect();
        snapshot
            .devices
            .into_iter()
            .map(|device| {
                let mut decorated = NearbyDevice::from_snapshot_device(device);
                if paired.contains(&decorated.id) {
                    decorated.status = "PAIRED".to_string();
                }
                decorated
            })
            .collect()
    }
}
