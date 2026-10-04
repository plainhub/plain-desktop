use super::messages::{ChannelMember, MemberPeerInfo};
use crate::{
    chat::enums::PeerStatus,
    db::{CHANNEL_COLS, DChannel, DPeer, PEER_COLS, now_iso, row_to_channel, row_to_peer},
};
use anyhow::{Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

pub(super) fn channel(c: &Connection, id: &str) -> Result<Option<DChannel>> {
    Ok(c.query_row(
        &format!("SELECT {CHANNEL_COLS} FROM chat_channels WHERE id=?1"),
        [id],
        row_to_channel,
    )
    .optional()?)
}
pub(super) fn peer(c: &Connection, id: &str) -> Result<Option<DPeer>> {
    Ok(c.query_row(
        &format!("SELECT {PEER_COLS} FROM peers WHERE id=?1"),
        [id],
        row_to_peer,
    )
    .optional()?)
}
pub(super) fn members(raw: &str) -> Result<Vec<ChannelMember>> {
    Ok(serde_json::from_str(raw)?)
}
pub(super) fn public_key(key: &str) -> Result<()> {
    if crate::base64_decode(key).len() != 32 {
        bail!("Invalid member public key");
    }
    Ok(())
}
pub(super) fn ensure_peers(c: &Connection, items: &[MemberPeerInfo], actor: &str) -> Result<()> {
    for item in items {
        if item.id.is_empty() {
            bail!("Missing member peer id");
        }
        if item.id == actor || peer(c, &item.id)?.is_some() {
            continue;
        }
        if !item.public_key.is_empty() {
            public_key(&item.public_key)?;
        }
        let now = now_iso();
        if c.execute("INSERT INTO peers(id,name,ip,key,public_key,status,port,device_type,token,created_at,updated_at) VALUES(?1,?2,?3,'',?4,?5,?6,?7,'',?8,?8)",params![item.id,item.name,item.ip,item.public_key,PeerStatus::Channel,item.port,item.device_type,now])?!=1 { bail!("Member peer insert failed"); }
    }
    Ok(())
}
pub(super) fn accepting_peer(
    c: &Connection,
    id: &str,
    name: &str,
    key: &str,
    kind: crate::chat::enums::DeviceType,
) -> Result<()> {
    public_key(key)?;
    match peer(c, id)? {
        Some(existing) => {
            if !existing.public_key.is_empty()
                && crate::base64_decode(&existing.public_key) != crate::base64_decode(key)
            {
                bail!("Member public key mismatch");
            }
            if c.execute("UPDATE peers SET public_key=CASE WHEN public_key='' THEN ?2 ELSE public_key END,name=CASE WHEN name='' THEN ?3 ELSE name END,updated_at=?4 WHERE id=?1",params![id,key,name,now_iso()])?!=1 { bail!("Member peer update failed"); }
        }
        None => ensure_peers(
            c,
            &[MemberPeerInfo {
                id: id.into(),
                name: name.into(),
                public_key: key.into(),
                device_type: kind,
                ip: String::new(),
                port: 0,
            }],
            "",
        )?,
    }
    Ok(())
}
pub(super) fn save(c: &Connection, channel: &DChannel, new: bool) -> Result<()> {
    let sql = if new {
        "INSERT INTO chat_channels(id,name,owner_id,members,key,version,status,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)"
    } else {
        "UPDATE chat_channels SET name=?2,owner_id=?3,members=?4,key=?5,version=?6,status=?7,created_at=?8,updated_at=?9 WHERE id=?1"
    };
    if c.execute(
        sql,
        params![
            channel.id,
            channel.name,
            channel.owner_id,
            channel.members,
            channel.key,
            channel.version,
            channel.status,
            channel.created_at,
            channel.updated_at
        ],
    )? != 1
    {
        bail!("Channel receive write failed");
    }
    Ok(())
}
