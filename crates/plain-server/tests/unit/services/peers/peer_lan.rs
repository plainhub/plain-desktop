use super::*;
use crate::assembly::server::ContentServer;
use plain_rs::chat::enums::DeviceType;
use plain_rs::chat::enums::PeerStatus;
use plain_rs::db::chat_store::SaveMode;
use plain_rs::db::chat_store::peers;
fn root(id: &str) -> (tempfile::TempDir, ContentServer, Vec<u8>, String) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    prefs.set("client_id", id).unwrap();
    prefs.set_user("service", true).unwrap();
    let (kp, pk) = plain_rs::crypto::ed25519_generate();
    prefs
        .set(
            "signature_key_pair",
            plain_rs::utils::base64::base64_encode(&kp),
        )
        .unwrap();
    let token = plain_rs::utils::base64::base64_encode(&[3; 32]);
    let server = ContentServer::start(&dir.path().join("plain.db"), &token, prefs).unwrap();
    (dir, server, kp.to_vec(), token)
}
fn peer(id: &str, port: u16, pk: &[u8]) -> DPeer {
    let mut peer = DPeer::new(id, id, "invalid,127.0.0.1", port, DeviceType::Phone);
    peer.status = PeerStatus::Paired;
    peer.key = plain_rs::utils::base64::base64_encode(&[7; 32]);
    peer.public_key = plain_rs::utils::base64::base64_encode(pk);
    peer
}
#[tokio::test]
async fn direct_tls_peer_exchange_uses_xchacha_and_current_identity_without_host() {
    let (_a, a, kp, _) = root("a");
    let (_b, b, bkp, _) = root("b");
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let (_, https) = b
        .start_public(
            0,
            0,
            cert.cert.pem().into_bytes(),
            cert.key_pair.serialize_pem().into_bytes(),
        )
        .await
        .unwrap();
    let remote = peer("b", https, &bkp[32..]);
    peers::save(&a.runtime_state().db, &[remote.clone()], SaveMode::Insert).unwrap();
    peers::save(
        &b.runtime_state().db,
        &[peer("a", 443, &kp[32..])],
        SaveMode::Insert,
    )
    .unwrap();
    let wire =
        plain_rs::transport::contract::signed_request(&kp, "query { __typename }", json!({}))
            .unwrap();
    let result = crate::services::peers::peer_transport::send(
        &a.runtime_state(),
        &remote,
        "",
        &[7; 32],
        &wire,
    )
    .await
    .unwrap();
    assert_eq!(result["data"]["__typename"], "Query");
    let mut changed = remote.clone();
    changed.key = plain_rs::utils::base64::base64_encode(&[9; 32]);
    peers::save(&a.runtime_state().db, &[changed], SaveMode::Update).unwrap();
    assert!(matches!(
        send(&a.runtime_state(), &remote, "", &[7; 32], &wire, || Ok(())).await,
        Err(Failure::Fatal(_))
    ));
    assert_eq!(a.runtime_state().lan.capacity.available_permits(), 4);
    a.shutdown().await;
    b.shutdown().await;
}
