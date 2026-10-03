use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
async fn wait(service: &ImageIndex) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while service.status().is_running {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}
fn setup(
    fail_page: bool,
    fail_end: bool,
) -> (
    Arc<ImageIndex>,
    tokio::task::JoinHandle<()>,
    Arc<AtomicUsize>,
) {
    let db = Arc::new(Db::open(std::path::Path::new(":memory:")).unwrap());
    let host = Arc::new(Host::default());
    let (generation, mut requests) = host.connect();
    let ended = Arc::new(AtomicUsize::new(0));
    let end_count = ended.clone();
    let peer = host.clone();
    let task = tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let id = &request["id"];
            let p = &request["params"];
            let result = match request["method"].as_str().unwrap() {
                "imageIndexBegin" => json!({"revision":"one","total":300}),
                "imageIndexPage" => {
                    if fail_page {
                        peer.reply(generation, json!({"id":id,"error":"permission denied"}))
                            .unwrap();
                        continue;
                    }
                    let offset = p["cursor"].as_str().unwrap().parse::<usize>().unwrap_or(0);
                    let end = (offset + 128).min(300);
                    json!({"revision":"one","items":(offset..end).map(|n|json!({"id":n.to_string(),"path":format!("/synthetic/{n}")})).collect::<Vec<_>>(),"nextCursor":end.to_string(),"done":end==300})
                }
                "imageIndexEmbed" => {
                    json!({"items":p["items"].as_array().unwrap().iter().map(|i|json!({"id":i["id"],"path":i["path"],"embeddingBase64":STANDARD.encode(1.0_f32.to_be_bytes())})).collect::<Vec<_>>(),"skippedIds":[]})
                }
                "imageIndexResolve" => json!(
                    p["ids"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(
                            |i| json!({"id":i,"path":format!("/synthetic/{}",i.as_str().unwrap())})
                        )
                        .collect::<Vec<_>>()
                ),
                "imageIndexEnd" => {
                    end_count.fetch_add(1, Ordering::SeqCst);
                    json!(!fail_end)
                }
                "imageIndexVerify" | "imageIndexProgress" => json!(true),
                method => panic!("unexpected {method}"),
            };
            peer.reply(generation, json!({"id":id,"result":result}))
                .unwrap();
        }
    });
    (Arc::new(ImageIndex::new(db, host)), task, ended)
}
#[tokio::test]
async fn host_job_runs_bounded_scan_and_releases_engine_before_completion() {
    let (service, task, ended) = setup(false, false);
    service.start(false).unwrap();
    wait(&service).await;
    assert_eq!(service.status().indexed_images, 300);
    assert!(service.status().error_message.is_empty());
    assert_eq!(image_embeddings::count(&service.db).unwrap(), 300);
    assert_eq!(ended.load(Ordering::SeqCst), 1);
    task.abort();
}
#[tokio::test]
async fn host_errors_are_visible_and_release_the_engine() {
    let (service, task, ended) = setup(true, false);
    service.start(false).unwrap();
    wait(&service).await;
    assert!(service.status().error_message.contains("permission denied"));
    assert_eq!(ended.load(Ordering::SeqCst), 1);
    task.abort();
    let (service, task, ended) = setup(false, true);
    service.selected(vec!["one".into()]).unwrap();
    wait(&service).await;
    assert!(service.status().error_message.contains("cleanup failed"));
    assert_eq!(ended.load(Ordering::SeqCst), 2);
    task.abort();
}
#[tokio::test]
async fn cancelled_generation_cannot_resurrect_deleted_embeddings() {
    let (service, task, _) = setup(false, false);
    let control = IndexControl {
        generation: 0,
        state: service.state.clone(),
        host: service.host.clone(),
        runtime: tokio::runtime::Handle::current(),
    };
    let input = EmbeddingInput {
        id: "one".into(),
        path: "/synthetic/one".into(),
        embedding_base64: STANDARD.encode(1.0_f32.to_be_bytes()),
    };
    image_embeddings::save(&service.db, std::slice::from_ref(&input)).unwrap();
    assert_eq!(service.remove(&["one".into()]).unwrap(), 1);
    assert!(control.save(&service.db, &[input]).is_err());
    assert_eq!(image_embeddings::count(&service.db).unwrap(), 0);
    task.abort();
}
