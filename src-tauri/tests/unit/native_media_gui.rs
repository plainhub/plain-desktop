use crate::commands::native::views::NativeViews;
use crate::native_resource_protocol::{ResourceRegistry, with_native_resource_protocol};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex, OnceLock, mpsc},
    time::Duration,
};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

static REQUESTS: OnceLock<Mutex<Vec<Value>>> = OnceLock::new();

pub(crate) fn record_request(
    scope: &str,
    request: &tauri::http::Request<Vec<u8>>,
) -> Option<usize> {
    let mut records = REQUESTS.get()?.lock().unwrap();
    let id = records.len();
    records.push(json!({"scope": scope, "url": request.uri().to_string(),
        "method": request.method().as_str(),
        "range": request.headers().get("range").and_then(|v| v.to_str().ok())}));
    Some(id)
}

pub(crate) fn record_response(id: Option<usize>, response: &tauri::http::Response<Vec<u8>>) {
    if let Some(id) = id {
        let mut records = REQUESTS.get().unwrap().lock().unwrap();
        records[id]["status"] = json!(response.status().as_u16());
        records[id]["bytes"] = json!(response.body().len());
        records[id]["content_range"] = json!(
            response
                .headers()
                .get("content-range")
                .and_then(|v| v.to_str().ok())
        );
    }
}

fn hosted_runner() -> bool {
    [
        ("GITHUB_ACTIONS", "true"),
        ("RUNNER_ENVIRONMENT", "github-hosted"),
        ("RUNNER_OS", "Linux"),
        ("GITHUB_REPOSITORY", "plainhub/plain-desktop"),
    ]
    .iter()
    .all(|(key, value)| std::env::var(key).as_deref() == Ok(*value))
}

#[test]
fn native_media_admission_and_hosted_acceptance() {
    if !hosted_runner() {
        writeln!(
            std::io::stderr(),
            "NOT RUN: native media acceptance requires the dedicated GitHub-hosted Linux runner; only admission was checked"
        ).unwrap();
        return;
    }
    let script =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/native-media/runner.py");
    let output = Command::new("python3")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .arg(script)
        .arg(std::env::current_exe().unwrap())
        .arg("native_media_gui::native_media_child")
        .output()
        .expect("NOT RUN: existing runner Python 3 is required");
    std::io::stdout().write_all(&output.stdout).unwrap();
    std::io::stderr().write_all(&output.stderr).unwrap();
    assert!(
        output.status.success(),
        "Native media acceptance or owned cleanup failed"
    );
}

fn child_root() -> PathBuf {
    assert!(
        hosted_runner(),
        "Native GUI refused outside the dedicated runner"
    );
    assert_eq!(
        std::env::var("PLAIN_NATIVE_MEDIA_CHILD").as_deref(),
        Ok("1")
    );
    let root =
        PathBuf::from(std::env::var_os("PLAIN_NATIVE_MEDIA_ROOT").expect("Owned root missing"));
    assert_eq!(root.canonicalize().unwrap(), root);
    let marker: Value =
        serde_json::from_slice(&fs::read(root.join("admission.json")).unwrap()).unwrap();
    assert_eq!(marker["root"].as_str(), root.to_str());
    assert_eq!(
        marker["binary"].as_str(),
        std::env::current_exe().unwrap().to_str()
    );
    assert_eq!(
        marker["display"].as_str(),
        std::env::var("DISPLAY").ok().as_deref()
    );
    assert_eq!(
        marker["bus"].as_str(),
        std::env::var("DBUS_SESSION_BUS_ADDRESS").ok().as_deref()
    );
    let parent = fs::read_to_string("/proc/self/status").unwrap();
    let parent = parent
        .lines()
        .find_map(|line| line.strip_prefix("PPid:"))
        .unwrap()
        .trim();
    assert_eq!(marker["parent"].as_u64().unwrap().to_string(), parent);
    assert_eq!(
        std::env::var("HOME").unwrap(),
        root.join("home").to_str().unwrap()
    );
    assert!(!std::env::vars().any(|(key, _)| key.starts_with("WAYLAND_")));
    root
}

fn page_script(urls: Value) -> String {
    include_str!("../fixtures/native-media/page.js").replace("FIXTURE_URLS", &urls.to_string())
}

async fn probe_http_media(url: &str, path: &Path) -> Result<Value, String> {
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

async fn http_fixture(
    root: &Path,
) -> Result<(plain_server::server::ContentServer, [String; 2], Value), String> {
    use plain_rs::features::files::resource_link::{self, Resource, Scope};
    let data = root.join("data/core");
    let cache = plain_rs::infrastructure::cache_paths::system::resolve(&data)?;
    if !cache.starts_with(root.join("cache")) || data.starts_with(&cache) {
        return Err("Core roots escaped the disposable fixture".into());
    }
    fs::create_dir_all(&data).map_err(|_| "Owned Core data directory unavailable")?;
    let prefs = Arc::new(
        plain_rs::features::prefs::Prefs::load(&plain_rs::features::prefs::default_path(&data))
            .map_err(|_| "Owned Core preferences unavailable")?,
    );
    let core =
        plain_server::server::ContentServer::start(&data, &plain_libs::crypto::gen_token(), prefs)?;
    core.set_build_debug(false);
    let prepared = async {
        let key = plain_libs::utils::base64::base64_decode_checked(&core.native_url_token()?)
            .map_err(|_| "Core resource credential unavailable")?;
        let url = |name: &str| -> Result<String, String> {
            let path = root.join(name);
            let id = resource_link::seal(
                &Scope::Global,
                &key,
                &Resource::reference(
                    path.to_str()
                        .ok_or("Synthetic resource path is not UTF-8")?,
                ),
            )
            .map_err(|_| "Synthetic resource authorization failed")?;
            Ok(format!(
                "http://127.0.0.1:{}/fs?id={id}&action=preview",
                core.port
            ))
        };
        let urls = [url("video.webm")?, url("audio.wav")?];
        let mut probes = Vec::new();
        for (name, url) in ["video.webm", "audio.wav"].into_iter().zip(&urls) {
            probes.push(probe_http_media(url, &root.join(name)).await?);
        }
        Ok::<_, String>((urls, json!(probes)))
    }
    .await;
    match prepared {
        Ok((urls, probes)) => Ok((core, urls, probes)),
        Err(error) => {
            core.shutdown().await;
            Err(error)
        }
    }
}

#[test]
#[ignore = "Supervisor-only: requires verified owned Xvfb, DBus and disposable roots"]
fn native_media_child() {
    let root = child_root();
    REQUESTS.set(Mutex::new(Vec::new())).unwrap();
    let (messages, received) = mpsc::channel::<(String, String)>();
    let result = Arc::new(Mutex::new(None::<Value>));
    let final_result = result.clone();
    let output_root = root.clone();
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    context.config_mut().build.dev_url = None;
    let app = with_native_resource_protocol(tauri::Builder::default().any_thread())
        .manage(NativeViews::default())
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                crate::commands::native::views::page_changed(window.app_handle(), window.label(), false);
            }
        })
        .setup(move |app| {
            let registry = app.state::<Arc<ResourceRegistry>>();
            let views = app.state::<NativeViews>();
            let owner = views.scope("media-owner");
            let foreign = views.scope("media-foreign");
            let video = tauri::async_runtime::block_on(registry.open(&owner, &root.join("small-video.webm")))
                .map_err(std::io::Error::other)?;
            let audio = tauri::async_runtime::block_on(registry.open(&owner, &root.join("control.wav")))
                .map_err(std::io::Error::other)?;
            let control = tauri::async_runtime::block_on(registry.open(&foreign, &root.join("control.wav")))
                .map_err(std::io::Error::other)?;
            let (core, http_urls, http_probes) = tauri::async_runtime::block_on(http_fixture(&root))
                .map_err(std::io::Error::other)?;
            let fixture_info: Value = serde_json::from_slice(&fs::read(root.join("fixtures.json"))?)?;
            let spec = |name: &str, kind: &str, url: &str| json!({"name":name, "kind":kind, "url":url,
                "duration":fixture_info[name]["duration"], "route":fixture_info[name]["expected_route"]});
            let media = json!([
                spec("video.webm", "video", &http_urls[0]),
                spec("audio.wav", "audio", &http_urls[1]),
                spec("small-video.webm", "video", &video.url),
                spec("control.wav", "audio", &audio.url),
            ]);
            for (label, urls) in [
                ("media-owner", json!({"media":media})),
                ("media-foreign", json!({"foreign":video.url, "control":spec("control.wav", "audio", &control.url)})),
            ] {
                let script = page_script(urls);
                let sender = messages.clone();
                let errors = messages.clone();
                WebviewWindowBuilder::new(app, label, WebviewUrl::External("about:blank".parse()?))
                    .on_page_load(move |window, payload| {
                        if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                            if let Err(error) = window.eval(&script) {
                                let error = error.to_string();
                                let detail = if error.contains("http://") || error.contains("https://") {
                                    "Test script evaluation failed; resource URL omitted"
                                } else { &error };
                                let _ = errors.send((window.label().to_string(), format!("plain-harness-error:{detail}")));
                            }
                        }
                    })
                    .data_directory(root.join("data").join(label))
                    .disable_drag_drop_handler()
                    .on_permission_request(|_, _| tauri::webview::PermissionResponse::Deny)
                    .on_document_title_changed(move |window, title| {
                        let _ = sender.send((window.label().to_string(), title));
                    })
                    .build()?;
            }
            let handle = app.handle().clone();
            let owner_id = owner.id().to_string();
            let foreign_id = foreign.id().to_string();
            std::thread::spawn(move || {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut reports = serde_json::Map::new();
                    let mut owner_done = false;
                    let mut foreign_ready = false;
                    let mut close_sent = false;
                    let deadline = std::time::Instant::now() + Duration::from_secs(180);
                    while reports.len() < 2 {
                        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                        let (label, title) = received.recv_timeout(remaining).expect("Native media event watchdog expired");
                        if let Some(error) = title.strip_prefix("plain-harness-error:") {
                            panic!("Native test script failed in {label}: {error}");
                        }
                        if let Some(value) = title.strip_prefix("plain-result:") {
                            reports.insert(label.clone(), serde_json::from_str::<Value>(value).unwrap());
                            owner_done |= label == "media-owner";
                        }
                        foreign_ready |= title.starts_with("plain-foreign-ready:");
                        if owner_done && foreign_ready && !close_sent {
                            handle.get_webview_window("media-owner").unwrap().destroy().unwrap();
                            let limit = std::time::Instant::now() + Duration::from_secs(10);
                            while !owner.is_closed() && std::time::Instant::now() < limit {
                                std::thread::sleep(Duration::from_millis(20));
                            }
                            assert!(owner.is_closed(), "Destroyed WebView scope remained open");
                            handle.get_webview_window("media-foreign").unwrap().eval("window.afterOwnerClose()").unwrap();
                            close_sent = true;
                        }
                    }
                    std::thread::sleep(Duration::from_millis(500));
                    let settled = REQUESTS.get().unwrap().lock().unwrap().len();
                    std::thread::sleep(Duration::from_secs(1));
                    let records = REQUESTS.get().unwrap().lock().unwrap().clone();
                    let complete = reports.get("media-owner").and_then(Value::as_array).is_some_and(|v| v.len() == 4)
                        && reports.get("media-foreign").and_then(Value::as_array).is_some_and(|v| v.len() == 3);
                    let media_passed = complete && reports.values().flat_map(|v| v.as_array().unwrap()).all(|v| v["status"] == "passed");
                    let denied = |status| records.iter().any(|r| r["scope"] == foreign_id && r["url"] == video.url && r["status"] == status);
                    let bounded_responses = records.iter().all(|r| r["bytes"].as_u64().is_some_and(|bytes| {
                        if bytes > crate::native_resource_protocol::MAX_RESPONSE_BYTES { return false; }
                        if r["status"] != 206 { return true; }
                        let Some(value) = r["content_range"].as_str().and_then(|v| v.strip_prefix("bytes ")) else { return false; };
                        let Some((range, total)) = value.split_once('/') else { return false; };
                        let Some((start, end)) = range.split_once('-') else { return false; };
                        let (Ok(start), Ok(end), Ok(total)) = (start.parse::<u64>(), end.parse::<u64>(), total.parse::<u64>()) else { return false; };
                        let expected = if r["url"] == video.url { video.size } else if r["url"] == audio.url { audio.size } else { total };
                        start <= end && end < total && total == expected && end - start + 1 == bytes
                    }));
                    let passed = media_passed && denied(403) && denied(404)
                        && bounded_responses && records.len() == settled;
                    json!({"status": if passed {"passed"} else {"failed"}, "reports": reports,
                        "http_route_probes":http_probes,
                        "native_scheme":{"verification_method":"production-scheme-observer", "requests":records,
                            "foreign_handle_denied":denied(403), "closed_owner_denied":denied(404),
                            "bounded_exact_range_lengths":bounded_responses,
                            "requests_stopped_after_cancel":records.len() == settled},
                        "large_media_route":"production-core-http-fs",
                        "large_native_scheme_playback":"unsupported; existing non-Range 413 unit boundary retained",
                        "webview_http_headers_observed":false,
                        "inflight_read_cancellation":"not exposed by the current Tauri responder"})
                }));
                let mut value = outcome.unwrap_or_else(|panic| {
                    let error = panic.downcast_ref::<String>().cloned().or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_else(|| "Native controller panic".into());
                    json!({"status":"failed", "error":error, "requests":REQUESTS.get().unwrap().lock().unwrap().clone()})
                });
                let port = core.port;
                let stopped = tauri::async_runtime::block_on(async {
                    tokio::time::timeout(Duration::from_secs(30), core.shutdown()).await.is_ok()
                });
                let listener_closed = std::net::TcpStream::connect_timeout(
                    &std::net::SocketAddr::from(([127, 0, 0, 1], port)), Duration::from_millis(250)).is_err();
                value["core_shutdown"] = json!({"completed":stopped, "loopback_listener_closed":listener_closed});
                if !stopped || !listener_closed { value["status"] = json!("failed"); }
                fs::write(output_root.join("native-results.json"), serde_json::to_vec_pretty(&value).unwrap()).unwrap();
                let passed = value["status"] == "passed";
                *final_result.lock().unwrap() = Some(value);
                handle.exit(if passed { 0 } else { 1 });
            });
            Ok(())
        })
        .build(context)
        .expect("Real Tauri WebView setup failed");
    let code = app.run_return(|_, _| {});
    let result = result.lock().unwrap();
    println!(
        "Native media result: {}",
        result.as_ref().unwrap_or(&Value::Null)
    );
    assert_eq!(code, 0);
    assert_eq!(result.as_ref().unwrap()["status"], "passed");
}
