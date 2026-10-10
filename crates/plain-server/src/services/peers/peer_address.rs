use plain_rs::db::DPeer;
use serde::{Serialize, Serializer};
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Address {
    best_ip: String,
    name: String,
    base_url: String,
    api_url: String,
    status_ws_url: String,
}
#[derive(Serialize)]
pub(crate) struct View<'a> {
    #[serde(flatten)]
    peer: &'a DPeer,
    address: Address,
}
pub(crate) fn address(peer: &DPeer) -> Address {
    let ip = peer.best_ip();
    let base_url = plain_rs::utils::build_url::build_url("https", &ip, peer.port, "");
    Address {
        name: if peer.name.trim().is_empty() {
            ip.clone()
        } else {
            peer.name.clone()
        },
        api_url: format!("{base_url}/peer_graphql"),
        status_ws_url: plain_rs::utils::build_url::build_url("wss", &ip, peer.port, "/status"),
        best_ip: ip,
        base_url,
    }
}
pub(crate) fn view(peer: &DPeer) -> View<'_> {
    View {
        peer,
        address: address(peer),
    }
}
pub(crate) fn serialize_view<S: Serializer>(
    peer: &DPeer,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    view(peer).serialize(serializer)
}
pub(crate) fn current(db: &plain_rs::db::Db, id: &str, expected: &DPeer) -> anyhow::Result<DPeer> {
    let peer = plain_rs::db::chat_store::peers::get(db, id)?
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
pub(crate) fn file_url(peer: &DPeer, file_id: &str) -> anyhow::Result<String> {
    let mut url = reqwest::Url::parse(&format!("{}/fs", peer.base_url()))?;
    url.query_pairs_mut().append_pair("id", file_id);
    Ok(url.to_string())
}
#[cfg(test)]
#[path = "../../../tests/unit/services/peers/peer_address.rs"]
mod tests;
