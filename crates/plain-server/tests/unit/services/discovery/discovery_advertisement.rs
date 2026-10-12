//! Advertisement facts read through the shared core. This is reached from
//! the mDNS and pairing runtimes, not from its own HTTP route, so it is
//! exercised through `execute` rather than a listener.

use super::*;

fn core_with_host(
    directory: &std::path::Path,
) -> (crate::assembly::server::ContentServer, ServerState) {
    let prefs =
        std::sync::Arc::new(plain_rs::prefs::Prefs::load(&directory.join("system.json")).unwrap());
    prefs.set("client_id", "fixture-root").unwrap();
    let core = crate::assembly::server::ContentServer::start_core(
        &directory.join("plain.db"),
        &plain_rs::utils::base64::base64_encode(&[9; 32]),
        prefs,
    )
    .unwrap();
    let state = core.runtime_state();
    (core, state)
}

#[tokio::test]
async fn facts_follow_the_current_root_preferences_over_delayed_os_facts() {
    let directory = tempfile::tempdir().unwrap();
    let prefs = std::sync::Arc::new(
        plain_rs::prefs::Prefs::load(&directory.path().join("system.json")).unwrap(),
    );
    prefs.set("client_id", "fixture-root").unwrap();
    let core = crate::assembly::server::ContentServer::start_core(
        &directory.path().join("plain.db"),
        &plain_rs::utils::base64::base64_encode(&[9; 32]),
        prefs.clone(),
    )
    .unwrap();
    let state = core.runtime_state();
    let host = state.host.clone();
    let (generation, mut rx) = host.connect();
    let changed = prefs.clone();
    let replies = host.clone();
    let responder = tokio::spawn(async move {
        for index in 0..2 {
            let request = rx.recv().await.unwrap();
            assert_eq!(request["method"], "discoveryFacts");
            assert_eq!(request["params"], json!({}));
            // The preferences change while the first OS read is still in
            // flight, so the next read has to pick the new values up rather
            // than serve the ones it was started with.
            if index == 1 {
                changed.set_user("device_name", "Current 中文").unwrap();
                changed.set_user("https_port", 3443).unwrap();
                changed.set("mdns_hostname", "current.local").unwrap();
            }
            replies.reply(
                generation,
                json!({"id":request["id"],"result":{"name":"Actual tablet","deviceType":"TABLET","version":"3.3.24","platform":"IOS","ips":["203.0.113.9"],"awareSupported":true,"awareRunning":true}}),
            )
            .unwrap();
        }
    });

    let first = execute(&state, Request::Reply {})
        .await
        .expect("the first read");
    assert_eq!(first["id"], "fixture-root");
    assert_eq!(first["deviceType"], "TABLET");

    let second = execute(&state, Request::Reply {})
        .await
        .expect("the second read");
    assert_eq!(
        second["name"], "Current 中文",
        "a preference change must not be hidden by a cached fact row"
    );
    // The advertised port is the live listener's, not the preference: a core
    // with no public listener advertises none.
    assert_eq!(second["port"], 0);
    assert_eq!(second["deviceType"], "TABLET");
    responder.await.unwrap();
    host.disconnect(generation);
    core.stop_core().await;
}

#[tokio::test]
async fn ble_decoding_is_hostless_and_bounded() {
    let directory = tempfile::tempdir().unwrap();
    let (core, state) = core_with_host(directory.path());
    assert!(
        !state.host.connected(),
        "decoding must not need a connected host"
    );
    assert!(
        execute(&state, Request::BleDecode { payload: None })
            .await
            .unwrap()
            .is_null()
    );
    let oversized = vec![0u8; 1651];
    assert!(
        execute(
            &state,
            Request::BleDecode {
                payload: Some(oversized)
            }
        )
        .await
        .is_err(),
        "an oversized advertising payload is refused by contract"
    );
    assert!(
        execute(&state, Request::BleShortId { id: String::new() })
            .await
            .is_err()
    );
    let short = execute(
        &state,
        Request::BleShortId {
            id: "peer-1234".to_string(),
        },
    )
    .await
    .unwrap();
    assert!(short.is_string(), "a short id is a plain string");
    core.stop_core().await;
}
