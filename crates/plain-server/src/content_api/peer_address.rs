use crate::db::DPeer;
use serde::{Serialize, Serializer};
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Address {
    best_ip: String,
    name: String,
    base_url: String,
    api_url: String,
    status_ws_url: String,
}
#[derive(Serialize)]
pub(super) struct View<'a> {
    #[serde(flatten)]
    peer: &'a DPeer,
    address: Address,
}
pub(super) fn address(peer: &DPeer) -> Address {
    let ip = peer.best_ip();
    let base_url = crate::utils::build_url::build_url("https", &ip, peer.port, "");
    Address {
        name: if peer.name.trim().is_empty() {
            ip.clone()
        } else {
            peer.name.clone()
        },
        api_url: format!("{base_url}/peer_graphql"),
        status_ws_url: crate::utils::build_url::build_url("wss", &ip, peer.port, "/status"),
        best_ip: ip,
        base_url,
    }
}
pub(super) fn view(peer: &DPeer) -> View<'_> {
    View {
        peer,
        address: address(peer),
    }
}
pub(super) fn serialize_view<S: Serializer>(
    peer: &DPeer,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    view(peer).serialize(serializer)
}
pub(super) fn current(db: &crate::db::Db, id: &str, expected: &DPeer) -> anyhow::Result<DPeer> {
    let peer = crate::db::chat_store::peers::get(db, id)?
        .ok_or_else(|| anyhow::anyhow!("Peer removed"))?;
    anyhow::ensure!(
        peer.id == expected.id
            && peer.key == expected.key
            && peer.public_key == expected.public_key
            && peer.status == expected.status
            && peer.ip == expected.ip
            && peer.port == expected.port,
        "Peer changed"
    );
    Ok(peer)
}
pub(super) fn file_url(peer: &DPeer, file_id: &str) -> anyhow::Result<String> {
    let mut url = reqwest::Url::parse(&format!("{}/fs", peer.base_url()))?;
    url.query_pairs_mut().append_pair("id", file_id);
    Ok(url.to_string())
}
#[cfg(test)]
#[path = "../../tests/unit/content_api/peer_address.rs"]
mod tests;
