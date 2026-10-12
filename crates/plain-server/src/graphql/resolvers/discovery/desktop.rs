use async_graphql::{Context, Object, SimpleObject};
use std::sync::Arc;

use crate::assembly::context::AppCtx;

#[derive(SimpleObject, Clone, Debug)]
pub struct NearbyDevice {
    pub id: String,
    pub name: String,
    #[graphql(default)]
    pub ips: Vec<String>,
    pub port: i32,
    pub device_type: String,
    pub version: String,
    pub platform: String,
    pub last_seen: String,
    #[graphql(default)]
    pub discovery_methods: Vec<String>,
    /// Server-local best IP for this device, chosen from `ips` against the
    /// device's own interfaces. Useful when several LAN addresses are
    /// advertised and we want to dial the most reachable one.
    #[graphql(default)]
    pub best_ip: String,
    /// `PAIRED` if this device is in our `peers` table as a paired peer,
    /// `UNPAIRED` otherwise. Lets the UI sort unpaired devices to the
    /// bottom without an extra round trip.
    pub status: String,
}

impl NearbyDevice {
    pub fn from_snapshot_device(
        device: plain_rs::discovery::nearby_devices::Device,
    ) -> Self {
        let interfaces = plain_rs::transport::lan::local_interfaces();
        let best_ip = plain_rs::transport::lan::best(&device.ips, &interfaces);
        Self {
            id: device.id,
            name: device.name,
            ips: device.ips,
            port: device.port as i32,
            device_type: device.device_type,
            version: device.version,
            platform: device.platform,
            last_seen: device.last_seen,
            discovery_methods: device.discovery_methods,
            best_ip,
            // Pairing status is filled in by `load` via the paired set so the
            // resolver can decorate plain-rs devices with our local state.
            status: "UNPAIRED".to_string(),
        }
    }
}

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
        let snapshot = c
            .app
            .nearby_devices
            .snapshot();
        let paired: std::collections::HashSet<String> = plain_rs::db::chat_store::peers::all(&c.app.db)
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
        let paired: std::collections::HashSet<String> = plain_rs::db::chat_store::peers::all(&c.app.db)
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