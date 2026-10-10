use super::server::ServerState;

fn changed(state: &ServerState, id: &str) {
    state.transport.forget(id);
    state.prewarmer.forget(id);
    if let Err(error) = state.peer_status.outgoing.reconnect(state) {
        log::warn!("Peer status reconnect after mutation: {error}");
    }
    super::peer_transport::notify(state);
    super::peer_status::emit(state, id, false);
}

pub(super) fn remove(state: &ServerState, id: &str) -> anyhow::Result<bool> {
    let removed = crate::chat::app_file_store::chat_deletion::delete(
        &state.db,
        &state.directory,
        crate::chat::app_file_store::chat_deletion::Selection::PeerRecord(id),
    )? != 0;
    if removed {
        changed(state, id);
        super::pairing_runtime::event(
            state,
            crate::chat::events::WS_MESSAGE_DELETED,
            serde_json::json!(format!("peer:{id}")),
        );
    }
    Ok(removed)
}

pub(super) fn unpair(state: &ServerState, id: &str) -> anyhow::Result<bool> {
    let changed_peer = crate::db::chat_store::peers::unpair(&state.db, id)?;
    if changed_peer {
        changed(state, id);
    }
    Ok(changed_peer)
}
