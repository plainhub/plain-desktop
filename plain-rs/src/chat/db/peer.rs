use rusqlite::params;

use super::ChatDb;
use super::utils::now_iso;
use crate::chat::enums::{DeviceType, PeerStatus};
use crate::utils::http_url::build_url;

/// Matches plain-app `DPeer` entity.
#[derive(Clone, Debug)]
pub struct DPeer {
    pub id: String,
    pub name: String,
    pub ip: String,
    /// Base64-encoded XChaCha20 shared key (ECDH-derived). Empty until paired.
    pub key: String,
    /// Base64-encoded raw Ed25519 public key (32 bytes).
    pub public_key: String,
    pub status: PeerStatus,
    pub port: u16,
    pub device_type: DeviceType,
    /// Login session token (XChaCha20 shared key, base64). Empty when logged out.
    pub token: String,
    pub created_at: String,
    pub updated_at: String,
}

impl DPeer {
    pub fn new(id: &str, name: &str, ip: &str, port: u16, device_type: DeviceType) -> Self {
        let now = now_iso();
        Self {
            id: id.to_string(),
            name: name.to_string(),
            ip: ip.to_string(),
            key: String::new(),
            public_key: String::new(),
            status: PeerStatus::Unpaired,
            port,
            device_type,
            token: String::new(),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    pub fn is_paired(&self) -> bool {
        self.status == PeerStatus::Paired
    }

    pub fn best_ip(&self) -> &str {
        self.ip.split(',').next().unwrap_or(&self.ip).trim()
    }

    pub fn base_url(&self) -> String {
        build_url("https", self.best_ip(), self.port, "")
    }

    pub fn peer_graphql_url(&self) -> String {
        format!("{}/peer_graphql", self.base_url())
    }

    pub fn file_url(&self, file_id: &str) -> String {
        format!("{}/fs?id={}", self.base_url(), file_id)
    }
}

const PEER_COLS: &str =
    "id,name,ip,key,public_key,status,port,device_type,token,created_at,updated_at";

fn row_to_peer(row: &rusqlite::Row<'_>) -> rusqlite::Result<DPeer> {
    Ok(DPeer {
        id: row.get(0)?,
        name: row.get(1)?,
        ip: row.get(2)?,
        key: row.get(3)?,
        public_key: row.get(4)?,
        status: row.get(5)?,
        port: row.get::<_, i64>(6)? as u16,
        device_type: row.get(7)?,
        token: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

impl ChatDb {
    pub fn get_peers(&self) -> Vec<DPeer> {
        let conn = self.0.lock().unwrap();
        let mut stmt =
            match conn.prepare(&format!("SELECT {PEER_COLS} FROM peers ORDER BY name ASC")) {
                Ok(s) => s,
                Err(_) => return vec![],
            };
        stmt.query_map(params![], row_to_peer)
            .ok()
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    pub fn get_peer_by_id(&self, id: &str) -> Option<DPeer> {
        let conn = self.0.lock().unwrap();
        conn.query_row(
            &format!("SELECT {PEER_COLS} FROM peers WHERE id=?"),
            params![id],
            row_to_peer,
        )
        .ok()
    }

    pub fn upsert_peer(&self, peer: &DPeer) {
        let conn = self.0.lock().unwrap();
        let result = conn.execute(
            "INSERT INTO peers (id,name,ip,key,public_key,status,port,device_type,token,created_at,updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
             ON CONFLICT(id) DO UPDATE SET
               name=excluded.name, ip=excluded.ip, key=excluded.key,
               public_key=excluded.public_key, status=excluded.status,
               port=excluded.port, device_type=excluded.device_type,
               token=excluded.token, updated_at=excluded.updated_at",
            params![
                peer.id, peer.name, peer.ip, peer.key, peer.public_key,
                peer.status, peer.port as i64, peer.device_type, peer.token,
                peer.created_at, peer.updated_at
            ],
        );
        if let Err(e) = result {
            log::error!("upsert_peer failed id={} err={e}", peer.id);
        } else {
            log::info!(
                "upsert_peer ok id={} name={} status={}",
                peer.id,
                peer.name,
                peer.status
            );
        }
    }

    pub fn delete_peer(&self, id: &str) {
        let conn = self.0.lock().unwrap();
        let _ = conn.execute("DELETE FROM peers WHERE id=?", params![id]);
    }

    pub fn update_peer_status(&self, id: &str, status: PeerStatus) {
        let now = now_iso();
        let conn = self.0.lock().unwrap();
        let _ = conn.execute(
            "UPDATE peers SET status=?1, updated_at=?2 WHERE id=?3",
            params![status, now, id],
        );
    }

    pub fn update_peer_status_and_key(&self, id: &str, status: PeerStatus, key: &str) {
        let now = now_iso();
        let conn = self.0.lock().unwrap();
        let _ = conn.execute(
            "UPDATE peers SET status=?1, key=?2, updated_at=?3 WHERE id=?4",
            params![status, key, now, id],
        );
    }

    /// Records a login session for a remote device. Creates the peer as
    /// UNPAIRED when missing without a chat key; otherwise writes PAIRED
    /// and the chat key. A login without a chat key keeps existing pairing
    /// state. Empty `name`/`device_type` keep the stored values; a
    /// non-empty `signature_public_key` (TOFU login key) replaces the
    /// stored one.
    #[allow(clippy::too_many_arguments)]
    pub fn login_peer(
        &self,
        id: &str,
        name: &str,
        ip: &str,
        port: u16,
        device_type: DeviceType,
        token: &str,
        signature_public_key: &str,
        chat_key: &str,
    ) -> Result<(), String> {
        let now = now_iso();
        let conn = self.0.lock().unwrap();
        let result = conn.execute(
            "INSERT INTO peers (id,name,ip,key,public_key,status,port,device_type,token,created_at,updated_at)
             VALUES (?1,?2,?3,?9,?7,CASE WHEN ?9<>'' THEN 'PAIRED' ELSE 'UNPAIRED' END,?4,?5,?6,?8,?8)
             ON CONFLICT(id) DO UPDATE SET
               token=excluded.token, ip=excluded.ip, port=excluded.port,
               name=CASE WHEN excluded.name<>'' THEN excluded.name ELSE peers.name END,
               device_type=CASE WHEN excluded.device_type<>'' THEN excluded.device_type ELSE peers.device_type END,
               public_key=CASE WHEN excluded.public_key<>'' THEN excluded.public_key ELSE peers.public_key END,
               key=CASE WHEN excluded.key<>'' THEN excluded.key ELSE peers.key END,
               status=CASE WHEN excluded.key<>'' THEN 'PAIRED' ELSE peers.status END,
               updated_at=excluded.updated_at",
            params![id, name, ip, port as i64, device_type, token, signature_public_key, now, chat_key],
        );
        result.map_err(|e| e.to_string())?;
        log::info!("login_peer ok id={} name={} host={}:{}", id, name, ip, port);
        Ok(())
    }

    pub fn logout_peer(&self, id: &str) {
        let now = now_iso();
        let conn = self.0.lock().unwrap();
        if let Err(e) = conn.execute(
            "UPDATE peers SET token='', updated_at=?1 WHERE id=?2",
            params![now, id],
        ) {
            log::error!("logout_peer failed id={} err={e}", id);
        }
    }

    pub fn update_peer_name(&self, id: &str, name: &str) {
        let now = now_iso();
        let conn = self.0.lock().unwrap();
        let _ = conn.execute(
            "UPDATE peers SET name=?1, updated_at=?2 WHERE id=?3",
            params![name, now, id],
        );
    }

    /// Peers with an active login token (the device-switcher list).
    pub fn get_login_peers(&self) -> Vec<DPeer> {
        let conn = self.0.lock().unwrap();
        let mut stmt = match conn.prepare(&format!(
            "SELECT {PEER_COLS} FROM peers WHERE token<>'' ORDER BY created_at DESC"
        )) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![], row_to_peer)
            .ok()
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/chat/db/peer.rs"]
mod tests;
