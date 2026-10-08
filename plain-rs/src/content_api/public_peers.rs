//! Public peer, SIM and pairing roots.

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
        let state = ctx.data::<super::server::ServerState>()?;
        let online = state.peer_status.connections.snapshot(&state.db);
        Ok(crate::db::chat_store::peers::all(&state.db)?
            .into_iter()
            .map(|row| Peer {
                online: online["online"]
                    .as_array()
                    .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(row.id.as_str()))),
                id: row.id.into(),
                name: row.name,
                ip: row.ip,
                status: match row.status {
                    crate::chat::enums::PeerStatus::Paired => PeerStatus::Paired,
                    crate::chat::enums::PeerStatus::Unpaired => PeerStatus::Unpaired,
                    crate::chat::enums::PeerStatus::Channel => PeerStatus::Channel,
                },
                port: i32::from(row.port),
                device_type: parse_device_type(&row.device_type.to_string()),
                created_at: super::public_facts::stored_instant(&row.created_at),
                updated_at: super::public_facts::stored_instant(&row.updated_at),
            })
            .collect())
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
        super::pairing_runtime::start_device(
            ctx.data::<super::server::ServerState>()?,
            crate::chat::pairing::sessions::Target {
                device_id: input.id.to_string(),
                device_name: input.name,
                device_ip: String::new(),
                device_port: u16::try_from(input.port)?,
            },
            input.ips,
            input
                .discovery_methods
                .iter()
                .map(|method| method.as_str().to_owned())
                .collect(),
        )
        .await?;
        Ok(true)
    }

    async fn cancel_pairing(
        &self,
        ctx: &Context<'_>,
        device_id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        super::pairing_runtime::execute(
            ctx.data::<super::server::ServerState>()?,
            super::pairing_runtime::Request::Cancel {
                id: device_id.to_string(),
                generation: None,
            },
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
        super::pairing_runtime::respond_device(
            ctx.data::<super::server::ServerState>()?,
            serde_json::from_value(request(&input))?,
            accepted,
        )
        .await?;
        Ok(true)
    }

    async fn delete_peer(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        super::peer_actions::remove(ctx.data::<super::server::ServerState>()?, id.as_str())?;
        Ok(true)
    }

    async fn unpair_peer(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        super::peer_actions::unpair(ctx.data::<super::server::ServerState>()?, id.as_str())?;
        Ok(true)
    }
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

#[cfg(all(test, feature = "http_transport"))]
#[path = "../../tests/unit/content_api/public_peers.rs"]
mod tests;
