use super::*;
use crate::content_api::ContentServer;
use crate::filesystem::tasks::Store;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite_test::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
async fn call(port: u16, token: &str, doc: String) -> Value {
    let text = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/graphql"))
        .bearer_auth(token)
        .header("content-type", "application/json")
        .body(json!({"query":doc}).to_string())
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
#[tokio::test]
async fn mobile_http_tasks_persist_resolved_paths_and_gate_execution_on_native_authorization() {
    let temp = tempfile::tempdir().unwrap();
    let token = crate::base64_encode(&[12; 32]);
    let db_path = temp.path().join("tasks.db");
    let server = ContentServer::start(
        &db_path,
        &token,
        Arc::new(crate::prefs::Prefs::load(&temp.path().join("prefs.json")).unwrap()),
    )
    .unwrap();
    let mut request = format!("ws://127.0.0.1:{}/host", server.port)
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    let (mut socket, _) = connect_async(request).await.unwrap();
    let revoked = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let denied = revoked.clone();
    let scanned = Arc::new(Mutex::new(Vec::<String>::new()));
    let records = scanned.clone();
    let worker = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            let request: Value = serde_json::from_str(&text).unwrap();
            let reply = if denied.load(std::sync::atomic::Ordering::SeqCst) {
                json!({"id":request["id"],"error":"synthetic permission revoked"})
            } else {
                match request["method"].as_str().unwrap() {
                    "fileTaskAuthorize" => {}
                    "fileTaskScan" => records.lock().unwrap().extend(
                        request["params"]["paths"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|value| value.as_str().unwrap().to_owned()),
                    ),
                    method => panic!("unexpected method {method}"),
                }
                json!({"id":request["id"],"result":true})
            };
            socket
                .send(Message::Text(reply.to_string().into()))
                .await
                .unwrap();
        }
    });
    let src = temp.path().join("source.txt");
    let dst = temp.path().join("target.txt");
    std::fs::write(&src, b"synthetic").unwrap();
    std::fs::write(&dst, b"existing").unwrap();
    let document = format!(
        "mutation {{ createFileHostTask(clientId:\"owner\",type:COPY,title:\"copy\",ops:[{{src:{},dst:{},overwrite:false}}]) {{ id status }} }}",
        json!(src.to_str().unwrap()),
        json!(dst.to_str().unwrap())
    );
    let queued = call(server.port, &token, document.clone()).await;
    assert!(queued.get("errors").is_none(), "{queued}");
    let id = queued["data"]["createFileHostTask"]["id"].as_str().unwrap();
    let done = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let result = call(server.port,&token,format!("query {{ fileHostTaskRecord(clientId:\"owner\",id:{}) {{ id status error doneBytes completedOps {{ src dst }} }} }}",json!(id))).await;
            assert!(result.get("errors").is_none(),"{result}");
            let task = result["data"]["fileHostTaskRecord"].clone();
            if task["status"] == "DONE" || task["status"] == "ERROR" { break task; }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }).await.unwrap();
    assert_eq!(done["status"], "DONE");
    let resolved = temp.path().join("target_1.txt");
    assert_eq!(done["completedOps"][0]["dst"], resolved.to_str().unwrap());
    assert!(
        scanned
            .lock()
            .unwrap()
            .contains(&resolved.to_str().unwrap().to_owned())
    );
    assert_eq!(std::fs::read(&resolved).unwrap(), b"synthetic");
    assert_eq!(std::fs::read(&dst).unwrap(), b"existing");
    let foreign = call(
        server.port,
        &token,
        format!(
            "query {{ fileHostTaskRecord(clientId:\"foreign\",id:{}) {{ id }} }}",
            json!(id)
        ),
    )
    .await;
    assert!(foreign["data"]["fileHostTaskRecord"].is_null());
    revoked.store(true, std::sync::atomic::Ordering::SeqCst);
    let denied = call(server.port, &token, document).await;
    assert!(denied.get("errors").is_some());
    assert!(!temp.path().join("target_2.txt").exists());
    let rows = tasks::sqlite::SqliteStore(Arc::new(Db::open(&db_path).unwrap()))
        .list("owner")
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, FileTaskStatus::Done);
    let removed = call(
        server.port,
        &token,
        format!(
            "mutation {{ removeFileHostTask(clientId:\"owner\",id:{}) }}",
            json!(id)
        ),
    )
    .await;
    assert_eq!(removed["data"]["removeFileHostTask"], true);
    worker.abort();
    let _ = worker.await;
    server.shutdown().await;
}

async fn media_move_case(missing_destination: bool) {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let source = base.join("source");
    std::fs::create_dir(&source).unwrap();
    for name in ["audio", "video", "image", "document"] {
        std::fs::write(source.join(name), b"fixture").unwrap();
    }
    let destination = base.join("target");
    std::fs::write(&destination, b"existing").unwrap();
    let db_path = base.join("db");
    let token = crate::base64_encode(&[16; 32]);
    let server = ContentServer::start(
        &db_path,
        &token,
        Arc::new(crate::prefs::Prefs::load(&base.join("prefs")).unwrap()),
    )
    .unwrap();
    let db = Db::open(&db_path).unwrap();
    let audio_path = source.join("audio").to_str().unwrap().to_owned();
    db.with_conn(|c| -> rusqlite::Result<()> {
        for (kind, id, tag) in [(1,"1","audio"),(2,"2","video"),(3,"3","image"),(24,"4","document")] {
            c.execute("INSERT INTO tag_relations(tag_id,key,type,created_at,size,title) VALUES(?1,?2,?3,'first',7,'fixture')",rusqlite::params![tag,id,kind])?;
        }
        c.execute("INSERT INTO tag_relations(tag_id,key,type,created_at,size,title) VALUES('file',?1,22,'first',7,'fixture')",[source.join("document").to_str().unwrap()])?;
        c.execute_batch("INSERT INTO media_item(media_type,media_id,duration_ms,updated_at) VALUES('audio','1',5000000001,'first'),('video','2',6000000001,'first'); INSERT INTO video_play_progress(media_id,position_ms,updated_at) VALUES('2',4000000001,'first');")?;
        c.execute("INSERT INTO audio_queue_items(path,sort_order,title,artist,duration_ms) VALUES(?1,0,'fixture','',5000000001)",[&audio_path])?;
        c.execute("INSERT INTO audio_playback(id,path,position_ms,revision) VALUES(1,?1,100,4)",[&audio_path])?;
        Ok(())
    }).unwrap();
    let mut request = format!("ws://127.0.0.1:{}/host", server.port)
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    let (mut socket, _) = connect_async(request).await.unwrap();
    let source_root = source.clone();
    let clear_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let clear = clear_count.clone();
    let worker = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            let request: Value = serde_json::from_str(&text).unwrap();
            let params = &request["params"];
            let result = match request["method"].as_str().unwrap() {
                "fileTaskAuthorize" | "fileTaskScan" => json!(true),
                "fileTaskMediaSnapshot" => {
                    let rows = params["paths"].as_array().unwrap().iter().filter_map(|path| {
                        let path = path.as_str().unwrap();
                        let old = PathBuf::from(path).starts_with(&source_root);
                        if missing_destination && !old { return None }
                        let (kind,id) = match PathBuf::from(path).file_name().unwrap().to_str().unwrap() {
                            "audio" => (1,1), "video" => (2,2), "image" => (3,3), "document" => (24,4), _ => panic!("unexpected path"),
                        };
                        assert!(PathBuf::from(path).is_file());
                        Some(json!({"mediaType":kind,"mediaId":(if old{id}else{id+10}).to_string(),"path":path}))
                    }).collect::<Vec<_>>();
                    json!(rows)
                }
                "audioEngineCommand" => {
                    assert_eq!(params["action"], "CLEAR");
                    clear.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    json!({"path":params["playback"]["path"],"positionMs":0,"revision":params["playback"]["revision"],"loaded":false})
                }
                method => panic!("unexpected method {method}"),
            };
            socket
                .send(Message::Text(
                    json!({"id":request["id"],"result":result})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        }
    });
    let queued = call(server.port,&token,format!("mutation {{ createFileHostTask(clientId:\"owner\",type:MOVE,title:\"move\",ops:[{{src:{},dst:{},overwrite:false}}]) {{ id }} }}",json!(source.to_str().unwrap()),json!(destination.to_str().unwrap()))).await;
    assert!(queued.get("errors").is_none(), "{queued}");
    let id = queued["data"]["createFileHostTask"]["id"].as_str().unwrap();
    let task = tokio::time::timeout(std::time::Duration::from_secs(5),async {
        loop {
            let row = call(server.port,&token,format!("query {{ fileHostTaskRecord(clientId:\"owner\",id:{}) {{ status error completedOps {{ src dst }} }} }}",json!(id))).await;
            let task = row["data"]["fileHostTaskRecord"].clone();
            if task["status"]=="DONE" || task["status"]=="ERROR" { break task }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }).await.unwrap();
    let moved = base.join("target_1");
    assert!(!source.exists());
    assert!(moved.join("audio").is_file());
    assert_eq!(task["completedOps"][0]["dst"], moved.to_str().unwrap());
    assert_eq!(std::fs::read(&destination).unwrap(), b"existing");
    if missing_destination {
        assert_eq!(task["status"], "ERROR");
        assert!(
            task["error"]
                .as_str()
                .unwrap()
                .contains("moved media identity missing")
        );
        assert_eq!(clear_count.load(std::sync::atomic::Ordering::SeqCst), 0);
        db.with_conn(|c| {
            c.query_row(
                "SELECT key FROM tag_relations WHERE tag_id='audio'",
                [],
                |r| r.get::<_, String>(0),
            )
        })
        .map(|id| assert_eq!(id, "1"))
        .unwrap();
    } else {
        assert_eq!(task["status"], "DONE", "{task}");
        assert_eq!(clear_count.load(std::sync::atomic::Ordering::SeqCst), 1);
        db.with_conn(|c| -> rusqlite::Result<()> {
            for (tag, id) in [
                ("audio", "11"),
                ("video", "12"),
                ("image", "13"),
                ("document", "14"),
            ] {
                assert_eq!(
                    c.query_row(
                        "SELECT key FROM tag_relations WHERE tag_id=?1",
                        [tag],
                        |r| r.get::<_, String>(0)
                    )?,
                    id
                );
            }
            assert_eq!(
                c.query_row(
                    "SELECT key FROM tag_relations WHERE tag_id='file'",
                    [],
                    |r| r.get::<_, String>(0)
                )?,
                moved.join("document").to_str().unwrap()
            );
            assert_eq!(
                c.query_row(
                    "SELECT duration_ms FROM media_item WHERE media_type='audio' AND media_id='11'",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                5000000001
            );
            assert_eq!(
                c.query_row(
                    "SELECT position_ms FROM video_play_progress WHERE media_id='12'",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                4000000001
            );
            assert_eq!(
                c.query_row("SELECT count(*) FROM audio_queue_items", [], |r| r
                    .get::<_, i64>(0))?,
                0
            );
            assert_eq!(
                c.query_row("SELECT path FROM audio_playback WHERE id=1", [], |r| r
                    .get::<_, String>(
                    0
                ))?,
                ""
            );
            Ok(())
        })
        .unwrap();
    }
    worker.abort();
    let _ = worker.await;
    server.shutdown().await;
}
#[tokio::test]
async fn directory_move_rebinds_actual_native_ids_and_stops_stale_playback() {
    media_move_case(false).await;
}
#[tokio::test]
async fn missing_native_destination_keeps_metadata_and_reports_actual_completed_move() {
    media_move_case(true).await;
}
