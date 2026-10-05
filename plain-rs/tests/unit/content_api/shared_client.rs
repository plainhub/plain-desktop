use super::super::shared_download::download_url;
use super::*;
use crate::{
    content_api::ContentServer,
    db::{
        DChat, DPeer,
        chat_store::{SaveMode, messages, peers},
    },
    shares::client::Link,
};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE},
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
fn root() -> (tempfile::TempDir, ContentServer, String) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    prefs.set("client_id", "local").unwrap();
    let token = crate::base64_encode(&[4; 32]);
    let server = ContentServer::start(&dir.path().join("plain.db"), &token, prefs).unwrap();
    (dir, server, token)
}
fn card(port: u16) -> Card {
    serde_json::from_value(json!({"shareId":"fixture","urlToken":URL_SAFE.encode([7;32]),"peerInfo":{"id":"peer","ip":"127.0.0.1","port":port},"name":"old","itemCount":2,"totalSize":8,"expiresAt":"2030-01-01T00:00:00Z"})).unwrap()
}
fn save_card(state: &ServerState, card: &Card) -> DChat {
    let row = DChat::new(
        "peer",
        "me",
        "",
        &json!({"type":"SHARE","value":card}).to_string(),
    );
    messages::save(&state.db, &[row.clone()], SaveMode::Insert).unwrap();
    row
}
async fn tls() -> (u16, tokio::task::JoinHandle<()>, Arc<AtomicUsize>) {
    use axum::{
        Router,
        body::Bytes,
        extract::Query,
        routing::{get, post},
    };
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let config = axum_server::tls_rustls::RustlsConfig::from_pem(
        cert.cert.pem().into_bytes(),
        cert.key_pair.serialize_pem().into_bytes(),
    )
    .await
    .unwrap();
    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = socket.local_addr().unwrap().port();
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    let router=Router::new().route("/guest_graphql",post(move |headers:HeaderMap,body:Bytes|{let count=seen.clone();async move {
        assert_eq!(headers["c-id"],"fixture");let plain=crate::xchacha_decrypt_raw(&[7;32],&body).unwrap();let plain=String::from_utf8(plain).unwrap();let parts:Vec<_>=plain.splitn(3,'|').collect();assert!(uuid::Uuid::parse_str(parts[1]).is_ok());let request:Value=serde_json::from_str(parts[2]).unwrap();count.fetch_add(1,Ordering::SeqCst);
        if request["variables"]["virtualPath"]=="slow" {tokio::time::sleep(Duration::from_millis(200)).await;}
        let info=json!({"data":{"sharedInfo":{"name":"current","readOnly":true,"requiresPassword":false,"expiresAt":null,"urlToken":STANDARD.encode([8;32]),"entries":[{"name":"a 中文.txt","virtualPath":"folder/ +?#中文%","isDir":false,"size":8,"mimeType":"text/plain","hasThumb":false}]}}});
        crate::xchacha_encrypt_raw(&[7;32],info.to_string().as_bytes()).unwrap()
    }})).route("/fs",get(|Query(params):Query<std::collections::HashMap<String,String>>|async move {
        assert_eq!(params["sid"],"fixture");let plain=crate::xchacha_decrypt_raw(&[8;32],&crate::base64_decode(&params["id"])).unwrap();let decoded:Value=serde_json::from_slice(&plain).unwrap();assert_eq!(decoded["virtualPath"],"folder/ +?#中文%");"download"
    }));
    let task = tokio::spawn(async move {
        axum_server::from_tcp_rustls(socket, config)
            .serve(router.into_make_service())
            .await
            .unwrap();
    });
    (port, task, count)
}
#[tokio::test]
async fn authenticated_http_browse_updates_card_and_streams_real_tls_files_without_host() {
    let (_dir, server, token) = root();
    let state = server.runtime_state();
    let (port, tls, _) = tls().await;
    let expected = card(port);
    let row = save_card(&state, &expected);
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/shares/client", server.port);
    let request =
        json!({"action":"browse","message_id":row.id,"expected":expected,"virtual_path":null});
    assert_eq!(
        client
            .post(&url)
            .header("content-type", "application/json")
            .body(request.to_string())
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let response = client
        .post(&url)
        .bearer_auth(&token)
        .header("content-type", "application/json")
        .body(request.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let result: Value = serde_json::from_str(&response.text().await.unwrap()).unwrap();
    let result = &result["result"];
    assert_eq!(result["card"]["name"], "current");
    assert!(result["card"]["expiresAt"].is_null());
    assert_eq!(result["card"]["totalSize"], 8);
    let link: Link = serde_json::from_value(result["link"].clone()).unwrap();
    let remote = link
        .file_url(
            &result["info"]["urlToken"].as_str().unwrap(),
            "folder/ +?#中文%",
            false,
        )
        .unwrap();
    let download = format!("http://127.0.0.1:{}/shares/client/file", server.port);
    let response = client
        .post(download)
        .bearer_auth(&token)
        .header("content-type", "application/json")
        .body(json!({"url":remote}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.text().await.unwrap(), "download");
    assert_eq!(state.lan.capacity.available_permits(), 4);
    assert_eq!(
        client
            .post(&url)
            .bearer_auth(&token)
            .header("content-type", "application/json")
            .body(request.to_string())
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    server.shutdown().await;
    tls.abort();
}
#[tokio::test]
async fn discovery_retry_rereads_peer_address_and_late_response_cannot_replace_card() {
    let (_dir, server, _) = root();
    let state = server.runtime_state();
    let (port, tls, count) = tls().await;
    let expected = card(1);
    let row = save_card(&state, &expected);
    let peer = DPeer::new("peer", "peer", "", 1, crate::chat::enums::DeviceType::Phone);
    peers::save(&state.db, &[peer.clone()], SaveMode::Insert).unwrap();
    let task = {
        let state = state.clone();
        let expected = expected.clone();
        let id = row.id.clone();
        tokio::spawn(async move { browse(&state, &id, &expected, None).await })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut peer = peer;
    peer.ip = "invalid,127.0.0.1".into();
    peer.port = port;
    peers::save(&state.db, &[peer], SaveMode::Update).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(result["link"]["port"], port);
    let current: Card = serde_json::from_value(result["card"].clone()).unwrap();
    let before = count.load(Ordering::SeqCst);
    let task = {
        let state = state.clone();
        let id = row.id.clone();
        tokio::spawn(async move { browse(&state, &id, &current, Some("slow")).await })
    };
    tokio::time::timeout(Duration::from_secs(3), async {
        while count.load(Ordering::SeqCst) == before {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    messages::content(
        &state.db,
        &row.id,
        "{\"type\":\"TEXT\",\"value\":{\"text\":\"replacement\"}}",
    )
    .unwrap();
    assert!(task.await.unwrap().is_err());
    assert!(
        messages::get(&state.db, &row.id)
            .unwrap()
            .unwrap()
            .content
            .contains("replacement")
    );
    server.shutdown().await;
    tls.abort();
}
#[test]
fn discovery_selects_complete_matching_records_and_safe_ipv6_urls() {
    let expected = card(443);
    let mut links = vec![];
    let mut seen = HashSet::new();
    let snapshot = json!({"services":[{"complete":false,"port":8443,"txtRecords":["id=peer"],"ips":["127.0.0.1"]},{"complete":true,"port":8443,"txtRecords":["id=other"],"ips":["127.0.0.1"]},{"complete":true,"port":8443,"txtRecords":["id=peer"],"ips":["127.0.0.1","127.0.0.1"],"ipv6":["::1"]}]});
    discovery_candidates(&mut links, &mut seen, &snapshot, &expected);
    assert_eq!(links.len(), 2);
    assert!(links[1].page_url.starts_with("https://[::1]:8443/"));
    assert!(download_url("http://localhost/fs?sid=s&id=x").is_err());
    assert!(download_url("https://user@localhost/fs?sid=s&id=x").is_err());
    assert!(download_url("https://localhost/proxyfs?sid=s&id=x").is_err());
}

#[tokio::test]
async fn own_share_links_use_root_secret_ports_and_revocation_without_host() {
    let (dir, server, token) = root();
    let state = server.runtime_state();
    state
        .prefs
        .set("master_secret", STANDARD.encode([5; 32]))
        .unwrap();
    state.prefs.set_user("https_port", 2443).unwrap();
    let path = dir.path().join("file.txt");
    std::fs::write(&path, b"fixture").unwrap();
    let service = crate::shares::Service::new(state.db.clone(), state.prefs.clone());
    let share = service
        .create(
            "fixture".into(),
            vec![path.to_str().unwrap().into()],
            STANDARD.encode([8; 32]),
            true,
            None,
        )
        .unwrap();
    let url = format!("http://127.0.0.1:{}/shares/client", server.port);
    let client = reqwest::Client::new();
    let request = json!({"action":"ownLink","id":share.id,"host":"::1"});
    let response = client
        .post(&url)
        .bearer_auth(&token)
        .header("content-type", "application/json")
        .body(request.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let response: Value = serde_json::from_str(&response.text().await.unwrap()).unwrap();
    assert_eq!(
        response["result"]["token"],
        service.token(&share.id).unwrap()
    );
    assert!(
        response["result"]["pageUrl"]
            .as_str()
            .unwrap()
            .starts_with("https://[::1]:2443/s/")
    );
    state.db.share_delete(&share.id).unwrap();
    assert_eq!(
        client
            .post(&url)
            .bearer_auth(&token)
            .header("content-type", "application/json")
            .body(request.to_string())
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    server.shutdown().await;
}
