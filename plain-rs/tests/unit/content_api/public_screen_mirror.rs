use super::*;
use crate::content_api::public_schema::PublicSchema;
use serde_json::{Value, json};

fn stub(host: Arc<Host>, handler: impl Fn(&str, Value) -> Value + Send + 'static) {
    let (generation, mut requests) = host.connect();
    let host = host.clone();
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let Some(id) = request["id"].as_u64() else {
                continue;
            };
            let result = handler(
                &request["method"].as_str().unwrap_or_default(),
                request["params"].clone(),
            );
            let _ = host.reply(generation, json!({ "id": id, "result": result }));
        }
    });
}

fn fixture() -> (tempfile::TempDir, PublicSchema) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap());
    let db = Arc::new(crate::db::Db::open(&dir.path().join("data.db")).unwrap());
    let host = Arc::new(Host::default());
    (
        dir,
        crate::content_api::public_schema::build(host, prefs, db),
    )
}

fn schema_host(schema: &PublicSchema) -> Arc<Host> {
    schema.data::<Arc<Host>>().unwrap().clone()
}

fn state(running: bool, codec: Value) -> Value {
    json!({"running": running, "controlEnabled": true, "codec": codec})
}

#[tokio::test]
async fn mirroring_state_is_projected_from_one_platform_read() {
    let (_dir, schema) = fixture();
    stub(schema_host(&schema), |method, _| match method {
        "systemScreenMirrorState" => state(true, json!({"annexB":"AAAA","keyFrame":"BBBB"})),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"query { isScreenMirroring screenMirrorControlEnabled
                     screenMirrorVideoCodec { annexB keyFrame } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["isScreenMirroring"], true);
    assert_eq!(data["screenMirrorControlEnabled"], true);
    assert_eq!(data["screenMirrorVideoCodec"]["annexB"], "AAAA");
    assert_eq!(data["screenMirrorVideoCodec"]["keyFrame"], "BBBB");
}

/// No keyframe yet is null, not an empty string — the client distinguishes
/// "not emitted" from "emitted nothing".
#[tokio::test]
async fn a_codec_without_a_keyframe_reports_null() {
    let (_dir, schema) = fixture();
    stub(schema_host(&schema), |method, _| match method {
        "systemScreenMirrorState" => state(false, json!({"annexB":"AAAA","keyFrame": Value::Null})),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"query { screenMirrorVideoCodec { annexB keyFrame } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let codec = &response.data.into_json().unwrap()["screenMirrorVideoCodec"];
    assert_eq!(codec["annexB"], "AAAA");
    assert_eq!(codec["keyFrame"], Value::Null);
}

#[tokio::test]
async fn no_published_codec_is_null_rather_than_an_empty_object() {
    let (_dir, schema) = fixture();
    stub(schema_host(&schema), |method, _| match method {
        "systemScreenMirrorState" => state(false, Value::Null),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"query { screenMirrorVideoCodec { annexB } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["screenMirrorVideoCodec"],
        Value::Null
    );
}

#[tokio::test]
async fn quality_is_read_from_the_platform_store() {
    let (_dir, schema) = fixture();
    stub(schema_host(&schema), |method, _| match method {
        "systemScreenMirrorQuality" => json!({"mode":"SMOOTH","resolution":720}),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"query { screenMirrorQuality { mode resolution } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let quality = &response.data.into_json().unwrap()["screenMirrorQuality"];
    assert_eq!(quality["mode"], "SMOOTH");
    assert_eq!(quality["resolution"], 720);
}

/// An unrecognised stored mode reads back as HD — the store only ever holds
/// the two the picker offers.
#[tokio::test]
async fn an_unknown_stored_quality_mode_reads_as_hd() {
    let (_dir, schema) = fixture();
    stub(schema_host(&schema), |method, _| match method {
        "systemScreenMirrorQuality" => json!({"mode":"WAT","resolution":1080}),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"query { screenMirrorQuality { mode resolution } }"#)
        .await;
    assert_eq!(
        response.data.into_json().unwrap()["screenMirrorQuality"]["mode"],
        "HD"
    );
}

#[tokio::test]
async fn requesting_mirror_audio_reports_whether_the_device_already_granted_it() {
    let (_dir, schema) = fixture();
    stub(schema_host(&schema), |method, _| match method {
        "systemRequestScreenMirrorAudio" => json!(false),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { requestScreenMirrorAudio }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    // false means the platform raised the runtime prompt; the caller retries.
    assert_eq!(
        response.data.into_json().unwrap()["requestScreenMirrorAudio"],
        false
    );
}

#[tokio::test]
async fn start_screen_mirror_passes_the_audio_choice_through() {
    let (_dir, schema) = fixture();
    stub(schema_host(&schema), |method, params| match method {
        "systemStartScreenMirror" => {
            assert_eq!(params["audio"], true);
            json!(true)
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { startScreenMirror(audio:true) }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["startScreenMirror"],
        true
    );
}

#[tokio::test]
async fn quality_updates_send_the_mode_name_the_store_understands() {
    let (_dir, schema) = fixture();
    stub(schema_host(&schema), |method, params| match method {
        "systemUpdateScreenMirrorQuality" => {
            assert_eq!(params["mode"], "SMOOTH");
            json!(true)
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { updateScreenMirrorQuality(mode:SMOOTH) }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["updateScreenMirrorQuality"],
        true
    );
}

#[tokio::test]
async fn web_settings_carry_the_optional_feature_name() {
    let (_dir, schema) = fixture();
    stub(schema_host(&schema), |method, params| match method {
        "systemOpenWebSettings" => {
            assert_eq!(params["feature"], "CALL_LOGS");
            json!(true)
        }
        "systemOpenAccessibilitySettings" => json!(true),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { openWebSettings(feature:CALL_LOGS) }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let response = schema
        .execute(r#"mutation { openAccessibilitySettings }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
}

#[tokio::test]
async fn web_settings_without_a_feature_reach_the_host_as_null() {
    let (_dir, schema) = fixture();
    stub(schema_host(&schema), |method, params| match method {
        "systemOpenWebSettings" => {
            assert_eq!(params["feature"], Value::Null);
            json!(true)
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema.execute(r#"mutation { openWebSettings }"#).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
}
