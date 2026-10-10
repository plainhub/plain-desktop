use crate::assembly::server::ServerState;

fn changed(state: &ServerState, id: &str) {
    state.transport.forget(id);
    state.prewarmer.forget(id);
    if let Err(error) = state.peer_status.outgoing.reconnect(state) {
        log::warn!("Peer status reconnect after mutation: {error}");
    }
    crate::services::peers::peer_transport::notify(state);
    crate::services::peers::peer_status::emit(state, id, false);
}

pub(crate) fn remove(state: &ServerState, id: &str) -> anyhow::Result<bool> {
    let removed = plain_rs::chat::attachments::app_file_store::chat_deletion::delete(
        &state.db,
        &state.directory,
        plain_rs::chat::attachments::app_file_store::chat_deletion::Selection::PeerRecord(id),
    )? != 0;
    if removed {
        changed(state, id);
        crate::services::discovery::pairing_runtime::event(
            state,
            plain_rs::chat::events::WS_MESSAGE_DELETED,
            serde_json::json!(format!("peer:{id}")),
        );
    }
    Ok(removed)
}

pub(crate) fn unpair(state: &ServerState, id: &str) -> anyhow::Result<bool> {
    let changed_peer = plain_rs::db::chat_store::peers::unpair(&state.db, id)?;
    if changed_peer {
        changed(state, id);
    }
    Ok(changed_peer)
}
