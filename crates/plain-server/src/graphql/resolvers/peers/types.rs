use crate::assembly::db::DPeer;
use crate::assembly::enums::DeviceType;
use crate::assembly::enums::PeerStatus;
use async_graphql::ID;
use async_graphql::SimpleObject;

#[derive(SimpleObject, serde::Serialize)]
#[graphql(complex)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    pub id: ID,
    pub name: String,
    pub ip: String,
    pub status: PeerStatus,
    pub online: bool,
    pub port: i32,
    pub device_type: DeviceType,
    #[graphql(skip)]
    pub token: String,
    #[graphql(skip)]
    pub public_key: String,
    #[graphql(skip)]
    pub created_at: String,
    #[graphql(skip)]
    pub updated_at: String,
}

impl Peer {
    /// Build the GraphQL `Peer` from a `DPeer` row plus the live online flag
    /// from `PeerStatusManager`. Mirrors plain-app's
    /// `DPeer.toModel()` which calls `PeerStatusManager.isOnline(id)`.
    pub fn from_dpeer(p: DPeer, online: bool) -> Self {
        Self {
            id: p.id.into(),
            name: p.name,
            ip: p.ip,
            status: p.status.into(),
            online,
            port: p.port as i32,
            device_type: p.device_type.into(),
            token: p.token,
            public_key: p.public_key,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

#[async_graphql::ComplexObject]
impl Peer {
    async fn created_at(&self) -> async_graphql::Result<crate::graphql::types::Instant> {
        crate::graphql::types::parse_instant(&self.created_at)
    }
    async fn updated_at(&self) -> async_graphql::Result<crate::graphql::types::Instant> {
        crate::graphql::types::parse_instant(&self.updated_at)
    }
}
