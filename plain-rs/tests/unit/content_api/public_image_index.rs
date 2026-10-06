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

#[tokio::test]
async fn image_search_status_is_projected_from_the_platform() {
    let (_dir, schema) = fixture();
    stub(schema_host(&schema), |method, _| match method {
        "systemImageSearchStatus" => json!({
            "status": "DOWNLOADING", "downloadProgress": 42, "errorMessage": "",
            "modelSize": 734003200i64, "modelDir": "/data/models", "isIndexing": true,
            "totalImages": 1200, "indexedImages": 340,
        }),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"query { imageSearchStatus { status downloadProgress errorMessage
                     modelSize modelDir isIndexing totalImages indexedImages } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let status = &response.data.into_json().unwrap()["imageSearchStatus"];
    assert_eq!(status["status"], "DOWNLOADING");
    assert_eq!(status["downloadProgress"], 42);
    // The model is far past 2 GiB on some devices: it must stay a Long.
    assert_eq!(status["modelSize"], 734003200i64);
    assert_eq!(status["modelDir"], "/data/models");
    assert_eq!(status["isIndexing"], true);
    assert_eq!(status["indexedImages"], 340);
}

#[tokio::test]
async fn an_unrecognised_status_reads_as_unavailable() {
    let (_dir, schema) = fixture();
    stub(schema_host(&schema), |method, _| match method {
        "systemImageSearchStatus" => json!({"status":"SOMETHING_NEW"}),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"query { imageSearchStatus { status } }"#)
        .await;
    assert_eq!(
        response.data.into_json().unwrap()["imageSearchStatus"]["status"],
        "UNAVAILABLE"
    );
}

#[tokio::test]
async fn start_image_index_forwards_the_force_choice_and_defaults_to_false() {
    let (_dir, schema) = fixture();
    let seen = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
    let recorder = seen.clone();
    stub(schema_host(&schema), move |method, params| match method {
        "systemStartImageIndex" | "systemCancelImageIndex" => {
            recorder.lock().unwrap().push(params["force"].clone());
            json!(true)
        }
        other => panic!("unexpected host call {other}"),
    });
    for document in [
        r#"mutation { startImageIndex(force:true) }"#,
        r#"mutation { startImageIndex }"#,
        r#"mutation { cancelImageIndex }"#,
    ] {
        let response = schema.execute(document).await;
        assert!(
            response.errors.is_empty(),
            "{document}: {:?}",
            response.errors
        );
    }
    assert_eq!(
        *seen.lock().unwrap(),
        vec![json!(true), json!(false), json!(false)],
        "force must reach the platform exactly as the caller asked"
    );
}

#[tokio::test]
async fn the_image_search_switches_and_download_cancel_all_reach_the_platform() {
    let (_dir, schema) = fixture();
    stub(schema_host(&schema), |method, _| match method {
        "systemEnableImageSearch"
        | "systemDisableImageSearch"
        | "systemCancelImageModelDownload" => {
            json!(true)
        }
        other => panic!("unexpected host call {method}"),
    });
    for mutation in [
        "enableImageSearch",
        "disableImageSearch",
        "cancelImageModelDownload",
    ] {
        let response = schema.execute(&format!("mutation {{ {mutation} }}")).await;
        assert!(
            response.errors.is_empty(),
            "{mutation}: {:?}",
            response.errors
        );
        assert_eq!(response.data.into_json().unwrap()[mutation], true);
    }
}
