//! Public `/graphql` peer, SIM and pairing roots.
//!
//! Every one of these is a command rather than a read: pairing moves a
//! device through a handshake the app owns, and `sims` is the telephony
//! slot list. Rust holds the contract shape; the platform holds the state.

use super::host::Host;
use super::public_device::{DeviceType, device_type as parse_device_type};
use async_graphql::{Context, Enum, InputObject, Object, SimpleObject};
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum PeerStatus {
    Paired,
    Unpaired,
    Channel,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum DiscoveryMethod {
    Lan,
    Ble,
    Qr,
}

impl DiscoveryMethod {
    fn as_str(self) -> &'static str {
        match self {
            DiscoveryMethod::Lan => "LAN",
            DiscoveryMethod::Ble => "BLE",
            DiscoveryMethod::Qr => "QR",
        }
    }
}

#[derive(SimpleObject, Clone, Debug)]
pub struct Peer {
    pub id: async_graphql::ID,
    pub name: String,
    pub ip: String,
    pub status: PeerStatus,
    pub port: i32,
    #[graphql(name = "deviceType")]
    pub device_type: DeviceType,
    #[graphql(name = "createdAt")]
    pub created_at: crate::content_types::Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: crate::content_types::Instant,
    pub online: bool,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct Sim {
    pub id: async_graphql::ID,
    pub label: String,
    pub number: String,
    #[graphql(name = "subscriptionId")]
    pub subscription_id: i32,
}

#[derive(InputObject, Clone, Debug)]
pub struct PairingDeviceInput {
    pub id: async_graphql::ID,
    pub name: String,
    pub ips: Vec<String>,
    pub port: i32,
    #[graphql(name = "deviceType")]
    pub device_type: DeviceType,
    pub version: String,
    pub platform: String,
    #[graphql(name = "lastSeen")]
    pub last_seen: String,
    #[graphql(name = "discoveryMethods")]
    pub discovery_methods: Vec<DiscoveryMethod>,
}

#[derive(InputObject, Clone, Debug)]
pub struct PairingRequestInput {
    #[graphql(name = "fromId")]
    pub from_id: async_graphql::ID,
    #[graphql(name = "fromName")]
    pub from_name: String,
    pub port: i32,
    #[graphql(name = "deviceType")]
    pub device_type: DeviceType,
    #[graphql(name = "ecdhPublicKey")]
    pub ecdh_public_key: String,
    #[graphql(name = "signaturePublicKey")]
    pub signature_public_key: String,
    pub timestamp: crate::content_types::Long,
    pub ips: Vec<String>,
    pub signature: String,
    #[graphql(name = "fromIp")]
    pub from_ip: String,
    #[graphql(name = "awareSupported")]
    pub aware_supported: bool,
}

#[derive(Default)]
pub struct PeersQuery;

#[Object]
impl PeersQuery {
    async fn peers(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Peer>> {
        let facts = host_call(ctx, "systemPeerFacts", json!({})).await?;
        Ok(super::public_facts::rows(&facts, peer))
    }

    /// The SIM slots the platform reports. Empty on a device with no
    /// telephony, which is the honest answer rather than an error.
    async fn sims(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Sim>> {
        let facts = host_call(ctx, "systemSimFacts", json!({})).await?;
        Ok(super::public_facts::rows(&facts, |value| Sim {
            id: super::public_facts::id(value, "id"),
            label: super::public_facts::text(value, "label"),
            number: super::public_facts::text(value, "number"),
            subscription_id: super::public_facts::integer(value, "subscriptionId") as i32,
        }))
    }
}

#[derive(Default)]
pub struct PeersMutation;

#[Object]
impl PeersMutation {
    async fn pair_device(
        &self,
        ctx: &Context<'_>,
        input: PairingDeviceInput,
    ) -> async_graphql::Result<bool> {
        host_call(ctx, "systemPairDevice", json!({ "input": device(&input) })).await?;
        Ok(true)
    }

    async fn cancel_pairing(
        &self,
        ctx: &Context<'_>,
        device_id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        host_call(
            ctx,
            "systemCancelPairing",
            json!({ "deviceId": device_id.as_str() }),
        )
        .await?;
        Ok(true)
    }

    async fn respond_to_pairing(
        &self,
        ctx: &Context<'_>,
        input: PairingRequestInput,
        accepted: bool,
    ) -> async_graphql::Result<bool> {
        host_call(
            ctx,
            "systemRespondToPairing",
            json!({ "input": request(&input), "accepted": accepted }),
        )
        .await?;
        Ok(true)
    }

    async fn delete_peer(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        host_call(ctx, "systemDeletePeer", json!({ "id": id.as_str() })).await?;
        Ok(true)
    }

    async fn unpair_peer(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        host_call(ctx, "systemUnpairPeer", json!({ "id": id.as_str() })).await?;
        Ok(true)
    }
}

fn peer(value: &Value) -> Peer {
    Peer {
        id: super::public_facts::id(value, "id"),
        name: super::public_facts::text(value, "name"),
        ip: super::public_facts::text(value, "ip"),
        status: match super::public_facts::text(value, "status").as_str() {
            "UNPAIRED" => PeerStatus::Unpaired,
            "CHANNEL" => PeerStatus::Channel,
            _ => PeerStatus::Paired,
        },
        port: super::public_facts::integer(value, "port") as i32,
        device_type: parse_device_type(&super::public_facts::text(value, "deviceType")),
        created_at: super::public_facts::stored_instant(&super::public_facts::text(
            value, "createdAt",
        )),
        updated_at: super::public_facts::stored_instant(&super::public_facts::text(
            value, "updatedAt",
        )),
        online: super::public_facts::flag(value, "online"),
    }
}

/// The enums are the contract's own, so both sides spell them the same way
/// and the host reads them straight back off the wire.
fn device(input: &PairingDeviceInput) -> Value {
    json!({
        "id": input.id.as_str(),
        "name": input.name,
        "ips": input.ips,
        "port": input.port,
        "deviceType": input.device_type.as_str(),
        "version": input.version,
        "platform": input.platform,
        "lastSeen": input.last_seen,
        "discoveryMethods": input
            .discovery_methods
            .iter()
            .map(|method| method.as_str())
            .collect::<Vec<_>>(),
    })
}

fn request(input: &PairingRequestInput) -> Value {
    json!({
        "fromId": input.from_id.as_str(),
        "fromName": input.from_name,
        "port": input.port,
        "deviceType": input.device_type.as_str(),
        "ecdhPublicKey": input.ecdh_public_key,
        "signaturePublicKey": input.signature_public_key,
        "timestamp": input.timestamp.0,
        "ips": input.ips,
        "signature": input.signature,
        "fromIp": input.from_ip,
        "awareSupported": input.aware_supported,
    })
}

async fn host_call(ctx: &Context<'_>, method: &str, params: Value) -> async_graphql::Result<Value> {
    ctx.data_unchecked::<Arc<Host>>()
        .call(method, params)
        .await
        .map_err(async_graphql::Error::new)
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_peers.rs"]
mod tests;