use super::*;
use axum::{
    Router,
    body::Body,
    http::{Response, header},
    response::Html,
    routing::get,
};
async fn fixture_server() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let server=Router::new().route("/",get(||async{Html("<meta property='og:title' content='Fixture title'><meta property='og:description' content='Fixture description'><meta property='og:image' content='/favicon.png'>")}))
        .route("/favicon.png",get(||async{Response::builder().header(header::CONTENT_TYPE,"image/png").body(Body::from(crate::base64_decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/lYUAAAAASUVORK5CYII="))).unwrap()}))
        .route("/chunked",get(||async{Response::builder().header(header::CONTENT_TYPE,"text/html").body(Body::from_stream(futures_util::stream::iter((0..4).map(|_|Ok::<_,std::io::Error>(vec![b'x';16]))))).unwrap()}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, server).await.unwrap();
    });
    (addr, task)
}
#[tokio::test]
async fn html_image_fetch_and_atomic_message_commit_use_same_rust_pipeline() {
    let (addr, server) = fixture_server().await;
    let client = reqwest::Client::builder()
        .no_proxy()
        .resolve("fixture.example", addr)
        .build()
        .unwrap();
    let url = format!("http://fixture.example:{}/", addr.port());
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("plain.db")).unwrap();
    let mut row = crate::db::DChat::new(
        "me",
        "local",
        "",
        &json!({"type":"TEXT","value":{"text":url}}).to_string(),
    );
    row.id = "message".into();
    db.insert_chat(&row);
    let row = refresh_with(&db, dir.path(), "message", &client)
        .await
        .unwrap()
        .unwrap();
    let content: Value = serde_json::from_str(&row.content).unwrap();
    let preview = &content["value"]["linkPreviews"][0];
    assert_eq!(preview["title"], "Fixture title");
    assert_eq!(preview["description"], "Fixture description");
    assert_eq!(preview["imageWidth"], 1);
    let hash = preview["imageLocalPath"]
        .as_str()
        .unwrap()
        .strip_prefix("fid:")
        .unwrap()
        .split('.')
        .next()
        .unwrap();
    let file = db.app_file_get(hash).unwrap().unwrap();
    assert_eq!(file.ref_count, 1);
    assert_eq!(
        std::fs::read(dir.path().join(&file.real_path)).unwrap(),
        crate::base64_decode(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/lYUAAAAASUVORK5CYII="
        )
    );
    assert!(
        refresh_with(&db, dir.path(), "message", &client)
            .await
            .unwrap()
            .is_none()
    );
    edit(&db, dir.path(), "message", "plain text").unwrap();
    assert!(db.app_file_get(hash).unwrap().is_none());
    server.abort();
}
#[tokio::test]
async fn missing_content_length_cannot_bypass_body_limit() {
    let (addr, server) = fixture_server().await;
    let response = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(format!("http://{addr}/chunked"))
        .send()
        .await
        .unwrap();
    assert!(read_bounded(response, 32).await.is_err());
    server.abort();
}
