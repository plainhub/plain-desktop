use super::*;
use crate::db::Db;

async fn settled(runtime: &Runtime) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let loading = runtime
                .task
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|task| !task.is_finished());
            if !loading && !runtime.index.status().is_running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn failed_replacement_preserves_active_package_index_and_search() {
    let directory = tempfile::tempdir().unwrap();
    let incoming = directory.path().join("ai_models/incoming");
    let original = crate::image_inference::tests::fixture(&incoming);
    let photo = directory.path().join("photo.png");
    image::RgbImage::from_pixel(64, 64, image::Rgb([128, 64, 32]))
        .save(&photo)
        .unwrap();
    let host = Arc::new(Host::default());
    let (generation, mut requests) = host.connect();
    let peer = host.clone();
    let task = tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let result = match request["method"].as_str().unwrap() {
                "systemImageModelsObserve"
                | "imageIndexVerify"
                | "imageIndexEnd"
                | "imageIndexProgress" => json!(true),
                "imageIndexBegin" => json!({"revision":"one","total":1}),
                "imageIndexPage" => {
                    json!({"revision":"one","items":[{"id":"one","path":photo}],"nextCursor":"one","done":true})
                }
                method => panic!("Unexpected {method}"),
            };
            peer.reply(generation, json!({"id":request["id"],"result":result}))
                .unwrap();
        }
    });
    let prefs = Arc::new(Prefs::load(&directory.path().join("prefs.json")).unwrap());
    let db = Arc::new(Db::open(&directory.path().join("data.db")).unwrap());
    let index = Arc::new(ImageIndex::new(db.clone(), host.clone()));
    let (events, _) = tokio::sync::broadcast::channel(16);
    let runtime = Runtime::new(directory.path().into(), host, prefs, index, events);
    runtime.import().await.unwrap();
    settled(&runtime).await;
    assert_eq!(
        runtime.snapshot().status.status,
        ImageSearchStatusType::Ready
    );
    assert_eq!(crate::library::image_embeddings::count(&db).unwrap(), 1);
    assert_eq!(runtime.search("cat", 50).await.unwrap()[0].image_id, "one");
    let mut replacement = crate::image_inference::tests::fixture(&incoming);
    replacement.files[0].sha256 = "0".repeat(64);
    std::fs::write(
        incoming.join("manifest.json"),
        serde_json::to_vec(&replacement).unwrap(),
    )
    .unwrap();
    runtime.import().await.unwrap();
    settled(&runtime).await;
    assert_eq!(
        runtime.snapshot().status.status,
        ImageSearchStatusType::Ready
    );
    assert!(runtime.snapshot().status.error_message.contains("checksum"));
    assert_eq!(crate::library::image_embeddings::count(&db).unwrap(), 1);
    let active = crate::image_inference::manifest::Manifest::parse(
        &std::fs::read(directory.path().join("ai_models/active/manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        active.fingerprint().unwrap(),
        original.fingerprint().unwrap()
    );
    assert_eq!(runtime.search("cat", 50).await.unwrap()[0].score, 1.0);
    runtime.shutdown().await;
    runtime.enable(true).await.unwrap();
    settled(&runtime).await;
    assert_eq!(
        runtime.snapshot().status.status,
        ImageSearchStatusType::Ready
    );
    assert!(runtime.snapshot().status.error_message.is_empty());
    assert_eq!(runtime.search("cat", 50).await.unwrap()[0].score, 1.0);
    replacement = crate::image_inference::tests::fixture(&incoming);
    replacement.id = "different-space".into();
    std::fs::write(
        incoming.join("manifest.json"),
        serde_json::to_vec(&replacement).unwrap(),
    )
    .unwrap();
    runtime.import().await.unwrap();
    settled(&runtime).await;
    assert_eq!(
        runtime.snapshot().status.status,
        ImageSearchStatusType::Ready
    );
    assert_eq!(crate::library::image_embeddings::count(&db).unwrap(), 1);
    runtime.cancel(true).await.unwrap();
    assert_eq!(crate::library::image_embeddings::count(&db).unwrap(), 0);
    assert!(!directory.path().join("ai_models").exists());
    task.abort();
}
