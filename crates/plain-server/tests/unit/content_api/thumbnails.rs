use crate::content_api::ContentServer;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use sha2::Digest;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};

#[tokio::test]
async fn caches_coalesce_invalidate_and_preserve_system_fallback_and_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("image.png");
    std::fs::write(&path, b"source1").unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap());
    let token = crate::base64_encode(&[5; 32]);
    let server = ContentServer::start(&dir.path().join("content.db"), &token, prefs).unwrap();
    let mut request = format!("ws://127.0.0.1:{}/host", server.port)
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    let (mut socket, _) = connect_async(request).await.unwrap();
    let decodes = Arc::new(AtomicUsize::new(0));
    let count = decodes.clone();
    let deny = Arc::new(AtomicUsize::new(0));
    let refused = deny.clone();
    let cache = dir.path().join("cache");
    let cache_host = cache.clone();
    let output_url = format!("http://127.0.0.1:{}/files/thumbnail/output", server.port);
    let output_token = token.clone();
    let worker = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            let r: Value = serde_json::from_str(&text).unwrap();
            let p = &r["params"];
            let result = match r["method"].as_str().unwrap() {
                "thumbnailAuthorize" => {
                    if refused.load(Ordering::SeqCst) > 0 {
                        Err("permission revoked")
                    } else {
                        Ok(json!(cache_host))
                    }
                }
                "thumbnailDecode" => {
                    count.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                    if p["fileName"] == "no-output" {
                        Ok(json!(true))
                    } else if p["mediaId"] == "system-fails" {
                        Ok(Value::Null)
                    } else {
                        if p["fileName"] == "mutate" {
                            std::fs::write(p["path"].as_str().unwrap(), b"changed-during-decode")
                                .unwrap();
                        }
                        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
                        bytes.extend(std::fs::read(p["path"].as_str().unwrap()).unwrap());
                        bytes.extend(p.to_string().as_bytes());
                        if p["fileName"] == "large" {
                            bytes.resize(8 * 1024 * 1024, 7);
                        }
                        let response = reqwest::Client::new()
                            .post(format!(
                                "{}/{}",
                                output_url,
                                p["outputToken"].as_str().unwrap()
                            ))
                            .bearer_auth(&output_token)
                            .body(bytes)
                            .send()
                            .await
                            .unwrap();
                        assert_eq!(response.status(), reqwest::StatusCode::NO_CONTENT);
                        Ok(json!(true))
                    }
                }
                m => panic!("unexpected {m}"),
            };
            let reply = match result {
                Ok(v) => json!({"id":r["id"],"result":v}),
                Err(e) => json!({"id":r["id"],"error":e}),
            };
            socket
                .send(Message::Text(reply.to_string().into()))
                .await
                .unwrap();
        }
    });
    let url = format!("http://127.0.0.1:{}/files/thumbnail", server.port);
    let client = reqwest::Client::new();
    let body =
        json!({"path":path,"width":80,"height":60,"centerCrop":true,"mediaId":"","fileName":""});
    let mut calls = vec![];
    for _ in 0..8 {
        let client = client.clone();
        let url = url.clone();
        let token = token.clone();
        let body = body.clone();
        calls.push(tokio::spawn(async move {
            client
                .post(url)
                .bearer_auth(token)
                .header("content-type", "application/json")
                .body(body.to_string())
                .send()
                .await
                .unwrap()
                .bytes()
                .await
                .unwrap()
        }));
    }
    let first = calls.remove(0).await.unwrap();
    for call in calls {
        assert_eq!(first, call.await.unwrap());
    }
    assert_eq!(decodes.load(Ordering::SeqCst), 1);
    let send = |body: Value| {
        client
            .post(&url)
            .bearer_auth(&token)
            .header("content-type", "application/json")
            .body(body.to_string())
    };
    std::fs::write(&path, b"source2-longer").unwrap();
    let newer = send(body.clone())
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert_ne!(first, newer);
    assert_eq!(decodes.load(Ordering::SeqCst), 2);
    let mut crop = body.clone();
    crop["centerCrop"] = json!(false);
    assert_ne!(
        newer,
        send(crop).send().await.unwrap().bytes().await.unwrap()
    );
    assert_eq!(decodes.load(Ordering::SeqCst), 3);
    let mut alias = body.clone();
    alias["fileName"] = json!("different.svg");
    send(alias).send().await.unwrap();
    assert_eq!(decodes.load(Ordering::SeqCst), 4);
    let mut system = body.clone();
    system["mediaId"] = json!("system-fails");
    send(system.clone()).send().await.unwrap();
    assert_eq!(decodes.load(Ordering::SeqCst), 6);
    send(system).send().await.unwrap();
    assert_eq!(decodes.load(Ordering::SeqCst), 7);
    let mut good = body.clone();
    good["mediaId"] = json!("system-success");
    send(good.clone()).send().await.unwrap();
    send(good).send().await.unwrap();
    assert_eq!(decodes.load(Ordering::SeqCst), 9);
    let response = send(body.clone()).send().await.unwrap();
    assert_eq!(response.headers()["content-type"], "image/png");
    assert_eq!(
        response.headers()["cache-control"],
        "private, max-age=86400"
    );
    let etag = response.headers()["etag"].to_str().unwrap().to_string();
    assert_eq!(
        etag,
        format!(
            "\"{}\"",
            crate::utils::hex::bytes_to_hex(&sha2::Sha256::digest(&newer))
        )
    );
    let mut conditional = body.clone();
    conditional["ifNoneMatch"] = json!(format!("\"old\", W/{etag}"));
    let not_modified = send(conditional).send().await.unwrap();
    assert_eq!(not_modified.status(), reqwest::StatusCode::NOT_MODIFIED);
    assert!(not_modified.bytes().await.unwrap().is_empty());
    let mut changed_mode = body.clone();
    changed_mode["centerCrop"] = json!(false);
    changed_mode["ifNoneMatch"] = json!(etag);
    assert_eq!(
        send(changed_mode).send().await.unwrap().status(),
        reqwest::StatusCode::OK
    );
    let mut changing = body.clone();
    changing["fileName"] = json!("mutate");
    assert_eq!(
        send(changing).send().await.unwrap().status(),
        reqwest::StatusCode::NO_CONTENT
    );
    let mut large = body.clone();
    large["fileName"] = json!("large");
    let response = send(large).send().await.unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(response.bytes().await.unwrap().len(), 8 * 1024 * 1024);
    let mut missing = body.clone();
    missing["fileName"] = json!("no-output");
    assert_eq!(
        send(missing).send().await.unwrap().status(),
        reqwest::StatusCode::BAD_REQUEST
    );
    assert_eq!(
        server
            .runtime_state()
            .thumbnails
            .outputs
            .lock()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        client
            .post(format!("{url}/output/expired"))
            .bearer_auth(&token)
            .body("late")
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::GONE
    );
    deny.store(1, Ordering::SeqCst);
    assert_eq!(
        send(body.clone()).send().await.unwrap().status(),
        reqwest::StatusCode::BAD_REQUEST
    );
    assert_eq!(decodes.load(Ordering::SeqCst), 12);
    deny.store(0, Ordering::SeqCst);
    let mut invalid = body.clone();
    invalid["width"] = json!(0);
    assert_eq!(
        send(invalid).send().await.unwrap().status(),
        reqwest::StatusCode::BAD_REQUEST
    );
    assert_eq!(
        client
            .post(&url)
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        send(body).send().await.unwrap().status(),
        reqwest::StatusCode::NO_CONTENT
    );
    assert!(
        std::fs::read_dir(cache.join("thumbs/rust"))
            .unwrap()
            .all(|p| p.unwrap().path().extension().unwrap() == "thumb")
    );
    server.shutdown().await;
    worker.abort();
}
