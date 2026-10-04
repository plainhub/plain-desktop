use super::messages::ChannelMember;
use crate::{
    chat::enums::ChannelStatus,
    db::{
        CHANNEL_COLS, DChannel, Db,
        chat_store::{SaveMode, channels},
        now_iso, row_to_channel,
    },
};
use anyhow::{Result, bail};
use rusqlite::{OptionalExtension, params};

#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Action {
    Rename { name: String },
    Leave,
    Invite { peer: String },
    Kick { peer: String },
    Resend { peer: String },
    Accept,
}

pub fn create(db: &Db, actor: &str, name: &str) -> Result<DChannel> {
    if actor.is_empty() {
        bail!("Missing channel actor");
    }
    let mut channel = DChannel::new(name.trim(), actor);
    channel.key = crate::base64_encode(&crate::random_bytes(32));
    channel.members = serde_json::to_string(&[ChannelMember::new(actor)])?;
    channels::save(db, std::slice::from_ref(&channel), SaveMode::Insert)?;
    Ok(channel)
}

pub fn apply(db: &Db, actor: &str, id: &str, action: Action) -> Result<DChannel> {
    if actor.is_empty() {
        bail!("Missing channel actor");
    }
    db.with_conn(|c| -> Result<DChannel> {
        let tx=c.unchecked_transaction()?;
        let mut channel=tx.query_row(&format!("SELECT {CHANNEL_COLS} FROM chat_channels WHERE id=?1"),[id],row_to_channel).optional()?.ok_or_else(||anyhow::anyhow!("Channel not found"))?;
        let mut members: Vec<ChannelMember>=serde_json::from_str(&channel.members)?;
        let mut changed=true;
        let mut versioned=false;
        match action {
            Action::Rename { name } => { channel.name=name.trim().into(); versioned=true; }
            Action::Leave => {
                if channel.owner_id==actor { bail!("Owner cannot leave; delete the channel instead"); }
                channel.status=ChannelStatus::Left;
                members.retain(|m|m.peer_id!=actor);
            }
            Action::Invite { peer } => {
                if channel.owner_id!=actor { bail!("Only owner can add members"); }
                if peer.is_empty() { bail!("Missing member peer"); }
                if members.iter().any(|m|m.peer_id==peer) { bail!("Already a member"); }
                members.push(ChannelMember::pending(peer)); versioned=true;
            }
            Action::Kick { peer } => {
                if channel.owner_id!=actor { bail!("Only owner can remove members"); }
                if !members.iter().any(|m|m.peer_id==peer) { bail!("Not a member"); }
                members.retain(|m|m.peer_id!=peer); versioned=true;
            }
            Action::Resend { peer } => {
                if channel.owner_id!=actor { bail!("Only owner can resend invites"); }
                let member=members.iter().find(|m|m.peer_id==peer).ok_or_else(||anyhow::anyhow!("Not a member"))?;
                if !member.is_pending() { bail!("Member is not pending"); }
                if !tx.query_row("SELECT EXISTS(SELECT 1 FROM peers WHERE id=?1)",[peer],|r|r.get::<_,bool>(0))? { bail!("Peer not found"); }
                changed=false;
            }
            Action::Accept => {
                if !members.iter().any(|m|m.peer_id==actor && m.is_pending()) { bail!("Invite no longer valid"); }
                if !tx.query_row("SELECT EXISTS(SELECT 1 FROM peers WHERE id=?1)",[&channel.owner_id],|r|r.get::<_,bool>(0))? { bail!("Owner peer not found"); }
                changed=false;
            }
        }
        if changed {
            if versioned { channel.version=channel.version.checked_add(1).ok_or_else(||anyhow::anyhow!("Channel version overflow"))?; }
            channel.members=serde_json::to_string(&members)?;
            channel.updated_at=now_iso();
            if tx.execute("UPDATE chat_channels SET name=?2,members=?3,version=?4,status=?5,updated_at=?6 WHERE id=?1",params![channel.id,channel.name,channel.members,channel.version,channel.status,channel.updated_at])? != 1 { bail!("Channel update failed"); }
        }
        tx.commit()?;
        Ok(channel)
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/chat/channel/state.rs"]
mod tests;
