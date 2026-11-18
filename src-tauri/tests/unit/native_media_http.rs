use plain_libs::serde_json::{Value, json};
use plain_rs::features::files::resource_link::{self, Resource, Scope};
use plain_rs::features::services::files::content::Action;
use plain_server::server::ContentServer;
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    time::Duration,
};

pub(super) fn fixture_media_url(
    core: &ContentServer,
    port: u16,
    path: &Path,
) -> Result<String, String> {
    let key = plain_libs::utils::base64::base64_decode_checked(&core.native_url_token()?)
        .map_err(|_| "Core resource credential unavailable")?;
    let id = resource_link::seal(
        &Scope::Session(core.runtime().native_client_id()?),
        &key,
        &Resource::reference(
            path.to_str()
                .ok_or("Synthetic resource path is not UTF-8")?,
        ),
    )
    .map_err(|_| "Synthetic resource authorization failed")?;
    let action = Action::Preview.as_str();
    Ok(format!(
        "http://127.0.0.1:{port}/fs?id={id}&action={action}"
    ))
}

pub(super) async fn probe_http_media(url: &str, path: &Path) -> Result<Value, String> {
    let client = plain_libs::protocols::http_client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| "HTTP probe client initialization failed")?;
    let size = fs::metadata(path)
        .map_err(|_| "Synthetic media metadata unavailable")?
        .len();
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| "Production HTTP GET failed")?;
    if response.status().as_u16() != 200
        || response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            != Some(size)
    {
        return Err("Production HTTP GET status or length differs from the fixture".into());
    }
    let first = response
        .chunk()
        .await
        .map_err(|_| "Production HTTP first chunk failed")?
        .ok_or("Production HTTP body is empty")?;
    let first_bytes = first.len();
    if first_bytes == 0 || first_bytes > 1024 * 1024 || first_bytes as u64 >= size {
        return Err("Production HTTP stream did not provide a bounded first chunk".into());
    }
    drop(first);
    drop(response);
    let start = 32 * 1024 * 1024 + 137;
    let end = start + 4095;
    let mut response = client
        .get(url)
        .header("Range", format!("bytes={start}-{end}"))
        .send()
        .await
        .map_err(|_| "Production HTTP range request failed")?;
    let expected_range = format!("bytes {start}-{end}/{size}");
    if response.status().as_u16() != 206
        || response
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            != Some(expected_range.as_str())
    {
        return Err("Production HTTP range status or Content-Range differs".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Production HTTP range body failed")?
    {
        if bytes.len() + chunk.len() > 4096 {
            return Err("Production HTTP range returned too many bytes".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let mut file = fs::File::open(path).map_err(|_| "Synthetic comparison file unavailable")?;
    file.seek(SeekFrom::Start(start))
        .map_err(|_| "Synthetic comparison seek failed")?;
    let mut expected = [0; 4096];
    file.read_exact(&mut expected)
        .map_err(|_| "Synthetic comparison read failed")?;
    if bytes.as_slice() != expected.as_slice() {
        return Err("Production HTTP range bytes differ from the synthetic source".into());
    }
    Ok(
        json!({"fixture":path.file_name().and_then(|v| v.to_str()), "status":"passed",
        "route":"production-core-http-fs", "verification_method":"independent-rust-http-client",
        "plain_get_status":200, "plain_get_first_chunk_bytes":first_bytes, "plain_get_body_dropped":true,
        "range_status":206, "content_range":expected_range, "range_bytes":bytes.len(),
        "exact_source_bytes":true, "webview_http_headers_observed":false}),
    )
}

#[tokio::test]
async fn native_media_preflight_streams_resource_and_serves_exact_range() {
    let root = plain_libs::temporary::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    let media = root.join("synthetic.wav");
    let mut file = fs::File::create(&media).unwrap();
    file.set_len(33 * 1024 * 1024).unwrap();
    file.seek(SeekFrom::Start(32 * 1024 * 1024 + 137)).unwrap();
    file.write_all(&[0x5a; 4096]).unwrap();
    drop(file);
    let core = ContentServer::open_with_cache(&root.join("data"), &root.join("cache")).unwrap();
    let (cert, key) = plain_libs::tls::generate_pem(&["localhost".into()]).unwrap();
    let (http_port, _) = core.start_public(0, 0, cert, key).await.unwrap();
    let result = probe_http_media(
        &fixture_media_url(&core, http_port, &media).unwrap(),
        &media,
    )
    .await;
    core.shutdown().await;
    let report = result.unwrap();
    assert_eq!(report["plain_get_status"], 200);
    assert_eq!(report["range_status"], 206);
    assert_eq!(report["range_bytes"], 4096);
    assert_eq!(report["exact_source_bytes"], true);
}
