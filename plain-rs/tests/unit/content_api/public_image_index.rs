use super::*;
use crate::content_api::host::Host;
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
    let directory = dir.path().to_path_buf();
    let (events, _) = tokio::sync::broadcast::channel(16);
    (
        dir,
        crate::content_api::public_schema::build(host, events, prefs, db, directory),
    )
}

fn schema_host(schema: &PublicSchema) -> Arc<Host> {
    schema.data::<Arc<Host>>().unwrap().clone()
}

#[tokio::test]
async fn image_search_status_is_owned_by_rust_without_platform_projection() {
    let (dir, schema) = fixture();
    stub(schema_host(&schema), |method, _| {
        panic!("Status must not ask platform: {method}")
    });
    let response=schema.execute("{ imageSearchStatus { status modelSize modelDir isIndexing totalImages indexedImages } }").await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let row = &response.data.into_json().unwrap()["imageSearchStatus"];
    assert_eq!(row["status"], "UNAVAILABLE");
    assert_eq!(row["modelSize"], 399703800i64);
    assert_eq!(
        row["modelDir"],
        dir.path().join("ai_models").to_string_lossy().as_ref()
    );
    assert_eq!(row["isIndexing"], false);
    assert_eq!(row["indexedImages"], 0);
}
#[tokio::test]
async fn unavailable_platform_does_not_download_or_enable() {
    let (_dir, schema) = fixture();
    stub(schema_host(&schema), |method, _| match method {
        "systemImageModelAvailability" => json!(false),
        other => panic!("Unexpected platform operation: {other}"),
    });
    let response = schema.execute("mutation { enableImageSearch }").await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["enableImageSearch"],
        true
    );
    let models = schema
        .data::<Arc<super::super::image_models::Runtime>>()
        .unwrap();
    assert_eq!(
        models.snapshot().status.status,
        ImageSearchStatusType::Unavailable
    );
}
#[tokio::test]
async fn cancel_index_mutates_the_same_index_queried_by_model_status() {
    let (_dir, schema) = fixture();
    let index = schema
        .data::<Arc<super::super::image_index::ImageIndex>>()
        .unwrap();
    let before = index.status().version;
    let response = schema.execute("mutation { cancelImageIndex }").await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert!(index.status().version > before);
    let models = schema
        .data::<Arc<super::super::image_models::Runtime>>()
        .unwrap();
    assert_eq!(
        models.snapshot().status.is_indexing,
        index.status().is_running
    );
}
