use crate::assembly::server::ServerState;
use plain_rs::chat::enums::{DeviceType, PeerStatus};

pub struct RemoteLogin {
    pub id: String,
    pub name: String,
    pub host: String,
    pub device_type: DeviceType,
    pub token: String,
    pub signature_public_key: String,
    pub chat_key: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginPeer {
    pub id: String,
    pub name: String,
    pub ip: String,
    pub status: PeerStatus,
    pub online: bool,
    pub port: i32,
    pub device_type: DeviceType,
    pub token: String,
    pub public_key: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone)]
pub struct NativePeers {
    state: ServerState,
}

impl NativePeers {
    pub(crate) fn new(state: ServerState) -> Self {
        Self { state }
    }

    pub fn login(&self, input: RemoteLogin) -> Result<(), String> {
        self.state.native_client_id()?;
        let (ip, port) = plain_rs::utils::http_url::split_peer_host(&input.host);
        self.state.db.login_peer(
            &input.id,
            &input.name,
            &ip,
            port,
            input.device_type,
            &input.token,
            &input.signature_public_key,
            &input.chat_key,
        )
    }

    pub fn logout(&self, id: &str) -> Result<(), String> {
        self.state.native_client_id()?;
        self.state.db.logout_peer(id)
    }

    pub fn update_name(&self, id: &str, name: &str) -> Result<(), String> {
        self.state.native_client_id()?;
        self.state.db.update_peer_name(id, name)
    }

    pub fn logged_in(&self) -> Result<Vec<LoginPeer>, String> {
        self.state.native_client_id()?;
        let mut rows = plain_rs::db::chat_store::peers::all(&self.state.db)
            .map_err(|error| error.to_string())?;
        rows.retain(|row| !row.token.is_empty());
        rows.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(a.id.cmp(&b.id)));
        let connections = self.state.peer_status.connections.snapshot(&self.state.db);
        Ok(rows
            .into_iter()
            .map(|row| LoginPeer {
                online: connections["online"]
                    .as_array()
                    .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(row.id.as_str()))),
                id: row.id,
                name: row.name,
                ip: row.ip,
                status: row.status,
                port: i32::from(row.port),
                device_type: row.device_type,
                token: row.token,
                public_key: row.public_key,
                created_at: row.created_at,
                updated_at: row.updated_at,
            })
            .collect())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/native/peers.rs"]
mod tests;
