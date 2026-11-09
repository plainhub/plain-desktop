use crate::commands::native::views::NativeViews;
use crate::native_resource_protocol::{ResourceRegistry, with_native_resource_protocol};
use serde_json::{Value, json};
use std::{
    borrow::Cow,
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

static REQUESTS: OnceLock<Mutex<Vec<Value>>> = OnceLock::new();
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
const SHELL_PATH: &str = "/native-media-test.html";
const SHELL_URL: &str = "tauri://localhost/native-media-test.html";
const SHELL_HTML: &[u8] =
    b"<!doctype html><html><head><meta charset=\"utf-8\"></head><body></body></html>";

struct HarnessAssets;

impl tauri::Assets<tauri::Wry> for HarnessAssets {
    fn get(&self, key: &tauri::utils::assets::AssetKey) -> Option<Cow<'_, [u8]>> {
        (key.as_ref() == SHELL_PATH).then_some(Cow::Borrowed(SHELL_HTML))
    }

    fn iter(&self) -> Box<tauri::utils::assets::AssetsIter<'_>> {
        Box::new(std::iter::once((
            Cow::Borrowed(SHELL_PATH),
            Cow::Borrowed(SHELL_HTML),
        )))
    }

    fn csp_hashes(
        &self,
        _: &tauri::utils::assets::AssetKey,
    ) -> Box<dyn Iterator<Item = tauri::utils::assets::CspHash<'_>> + '_> {
        Box::new(std::iter::empty())
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
struct PageEvent {
    stage: String,
    sequence: u64,
    origin: String,
    href: String,
    transport: TransportState,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct TransportState {
    pending: u64,
    failure: Option<String>,
    prior_sequence: u64,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct ReportPacket {
    sequence: u64,
    kind: String,
    payload: Value,
}

struct ReportSink {
    sender: mpsc::Sender<(String, String)>,
    sequences: Mutex<std::collections::HashMap<String, u64>>,
}

#[tauri::command]
fn native_media_report(
    webview: tauri::Webview,
    packet: ReportPacket,
    sink: tauri::State<'_, ReportSink>,
) -> Result<u64, String> {
    let label = webview.label();
    let validated = (|| {
        if !matches!(label, "media-owner" | "media-foreign")
            || webview
                .url()
                .map_err(|_| "Report document unavailable")?
                .as_str()
                != SHELL_URL
        {
            return Err("Report sender is not an owned test document".to_string());
        }
        if !matches!(packet.kind.as_str(), "event" | "diagnostic" | "result")
            || serde_json::to_vec(&packet)
                .map_err(|_| "Invalid report packet")?
                .len()
                > 16384
        {
            return Err("Report type or size is invalid".to_string());
        }
        let mut sequences = sink
            .sequences
            .lock()
            .map_err(|_| "Report sequence state unavailable")?;
        let previous = sequences.entry(label.to_string()).or_default();
        if packet.sequence != *previous + 1 || packet.sequence > 512 {
            return Err("Report sequence or count is invalid".to_string());
        }
        sink.sender
            .send((
                label.to_string(),
                format!(
                    "plain-packet:{}",
                    serde_json::to_string(&packet).map_err(|_| "Invalid report packet")?
                ),
            ))
            .map_err(|_| "Report receiver stopped")?;
        *previous = packet.sequence;
        Ok(packet.sequence)
    })();
    if let Err(error) = &validated {
        let _ = sink
            .sender
            .send((label.to_string(), format!("plain-harness-error:{error}")));
    }
    validated
}

fn retain_diagnostic(label: &str, message: &str, events: &mut Vec<Value>) -> Result<(), String> {
    let packet = message
        .strip_prefix("plain-packet:")
        .map(serde_json::from_str::<ReportPacket>)
        .transpose()
        .map_err(|_| "Malformed terminal report packet")?;
    let diagnostic = if let Some(packet) = packet {
        events.push(json!({"sequence":next_sequence(), "window":label, "packet_sequence":packet.sequence, "packet_kind":packet.kind}));
        if packet.kind != "diagnostic" {
            return Err("Unexpected report after reporting completed".into());
        }
        packet.payload
    } else if let Some(payload) = message.strip_prefix("plain-diagnostic:") {
        serde_json::from_str(payload).map_err(|_| "Malformed native diagnostic")?
    } else {
        events.push(json!({"sequence":next_sequence(), "window":label, "error":message}));
        return Err("Native report transport or evaluation failed".into());
    };
    let fatal = matches!(
        diagnostic["stage"].as_str(),
        Some("script-error" | "script-failed" | "unhandled-rejection" | "web-process-terminated")
    );
    events.push(json!({"sequence":next_sequence(), "window":label, "diagnostic":diagnostic}));
    if fatal {
        Err("Native page or WebProcess failed; see retained diagnostics".into())
    } else {
        Ok(())
    }
}

fn report_transport_idle(window: &tauri::WebviewWindow) -> Result<Value, String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        let (sender, receiver) = mpsc::channel();
        window
            .eval_with_callback("window.__plainTransportState()", move |result| {
                let _ = sender.send(result);
            })
            .map_err(|_| "Report transport query could not be scheduled")?;
        let raw = receiver
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .map_err(|_| "Report transport query timed out")?;
        if raw.len() > 16384 {
            return Err("Report transport query exceeded its bound".into());
        }
        let state: Value = serde_json::from_str(&raw)
            .map_err(|_| "Report transport query was not a native JSON object")?;
        if state["failure"] != Value::Null
            || state["pending"].as_u64().is_none()
            || state["acknowledged"].as_u64().is_none()
            || state["sequence"].as_u64().is_none()
        {
            return Err(format!("Report transport failed: {state}"));
        }
        if state["pending"] == 0 && state["acknowledged"] == state["sequence"] {
            return Ok(state);
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!("Final report acknowledgement is missing: {state}"));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn next_sequence() -> u64 {
    SEQUENCE.fetch_add(1, Ordering::SeqCst) + 1
}

fn response_after(
    records: &[Value],
    scope: &str,
    url: &str,
    status: u16,
    after: u64,
) -> Option<u64> {
    records.iter().find_map(|record| {
        (record["scope"] == scope
            && record["url"] == url
            && record["status"] == status
            && record["sequence"]
                .as_u64()
                .is_some_and(|sequence| sequence > after))
        .then(|| record["response_sequence"].as_u64())
        .flatten()
    })
}

pub(crate) fn record_request(
    scope: &str,
    request: &tauri::http::Request<Vec<u8>>,
) -> Option<usize> {
    let mut records = REQUESTS.get()?.lock().unwrap();
    let id = records.len();
    records.push(json!({"sequence": next_sequence(), "scope": scope, "url": request.uri().to_string(),
        "method": request.method().as_str(),
        "request_cache_control": request.headers().get("cache-control").and_then(|v| v.to_str().ok()),
        "range": request.headers().get("range").and_then(|v| v.to_str().ok())}));
    Some(id)
}

pub(crate) fn record_response(id: Option<usize>, response: &tauri::http::Response<Vec<u8>>) {
    if let Some(id) = id {
        let mut records = REQUESTS.get().unwrap().lock().unwrap();
        records[id]["response_sequence"] = json!(next_sequence());
        records[id]["status"] = json!(response.status().as_u16());
        records[id]["bytes"] = json!(response.body().len());
        records[id]["response_cache_control"] = json!(
            response
                .headers()
                .get("cache-control")
                .and_then(|v| v.to_str().ok())
        );
        records[id]["content_type"] = json!(
            response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
        );
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
) -> Result<(plain_server::server::ContentServer, [String; 4], Value), String> {
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
        let urls = [
            url("video.webm")?,
            url("audio.wav")?,
            url("small-video.webm")?,
            url("control.wav")?,
        ];
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
    drop(context.set_assets(Box::new(HarnessAssets)));
    let app = with_native_resource_protocol(tauri::Builder::default().any_thread())
        .manage(NativeViews::default())
        .manage(ReportSink { sender: messages.clone(), sequences: Mutex::new(std::collections::HashMap::new()) })
        .invoke_handler(tauri::generate_handler![native_media_report])
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
            let image = tauri::async_runtime::block_on(registry.open(&owner, &root.join("image.png")))
                .map_err(std::io::Error::other)?;
            let control_image = tauri::async_runtime::block_on(registry.open(&foreign, &root.join("image.png")))
                .map_err(std::io::Error::other)?;
            let (core, http_urls, http_probes) = tauri::async_runtime::block_on(http_fixture(&root))
                .map_err(std::io::Error::other)?;
            let fixture_info: Value = serde_json::from_slice(&fs::read(root.join("fixtures.json"))?)?;
            let spec = |name: &str, kind: &str, url: &str| json!({"name":name, "kind":kind, "url":url,
                "duration":fixture_info[name]["duration"], "route":fixture_info[name]["expected_route"]});
            let media = json!([
                spec("video.webm", "video", &http_urls[0]),
                spec("audio.wav", "audio", &http_urls[1]),
                spec("small-video.webm", "video", &http_urls[2]),
                spec("control.wav", "audio", &http_urls[3]),
            ]);
            let image_spec = |url: &str| json!({"url":url, "width":fixture_info["image.png"]["width"],
                "height":fixture_info["image.png"]["height"]});
            for (label, urls) in [
                ("media-owner", json!({"media":media, "image":image_spec(&image.url)})),
                ("media-foreign", json!({"foreign":image.url, "image":image_spec(&control_image.url)})),
            ] {
                let script = page_script(urls);
                let errors = messages.clone();
                let process_events = messages.clone();
                let window = WebviewWindowBuilder::new(app, label, WebviewUrl::App(SHELL_PATH.into()))
                    .on_page_load(move |window, payload| {
                        let stage = match payload.event() {
                            tauri::webview::PageLoadEvent::Started => "page-load-started",
                            tauri::webview::PageLoadEvent::Finished => "page-load-finished",
                        };
                        let document_url = if payload.url().as_str() == SHELL_URL { SHELL_URL } else { "[Unexpected document URL omitted]" };
                        let _ = errors.send((window.label().to_string(), format!("plain-diagnostic:{}",
                            json!({"stage":stage, "document_url":document_url}))));
                        if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                            if let Err(error) = window.eval(&script) {
                                let error = error.to_string();
                                let detail = if error.contains("http://") || error.contains("https://") {
                                    "Test script evaluation failed; resource URL omitted"
                                } else { &error };
                                let _ = errors.send((window.label().to_string(), format!("plain-harness-error:{detail}")));
                            } else {
                                let _ = errors.send((window.label().to_string(), format!("plain-diagnostic:{}",
                                    json!({"stage":"script-evaluation-submitted"}))));
                            }
                        }
                    })
                    .data_directory(root.join("data/webview-profile"))
                    .inner_size(480.0, 480.0)
                    .position(if label == "media-owner" { 8.0 } else { 520.0 }, 32.0)
                    .visible(true)
                    .focused(label == "media-owner")
                    .disable_drag_drop_handler()
                    .on_permission_request(|_, _| tauri::webview::PermissionResponse::Deny)
                    .build()?;
                let _ = messages.send((label.to_string(), format!("plain-diagnostic:{}", json!({
                    "stage":"window-created", "visible":window.is_visible()?,
                    "outer_position":window.outer_position()?, "outer_size":window.outer_size()?, "inner_size":window.inner_size()?
                }))));
                window.with_webview(move |webview| {
                    use gtk::prelude::ObjectExt;
                    let terminated = process_events.clone();
                    webview.inner().connect_local("web-process-terminated", false, move |values| {
                        let _ = terminated.send((label.to_string(), format!("plain-diagnostic:{}",
                            json!({"stage":"web-process-terminated", "reason":format!("{:?}", values.get(1))}))));
                        None
                    });
                    let _ = process_events.send((label.to_string(), format!("plain-diagnostic:{}",
                        json!({"stage":"web-process-observer-attached"}))));
                })?;
            }
            let handle = app.handle().clone();
            let owner_id = owner.id().to_string();
            let foreign_id = foreign.id().to_string();
            std::thread::spawn(move || {
                let mut reports = serde_json::Map::from_iter([(String::from("media-owner"), json!([])), (String::from("media-foreign"), json!([]))]);
                let mut events = Vec::new();
                let mut transport_states = serde_json::Map::new();
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut page_sequences = std::collections::HashMap::<String, u64>::new();
                    let mut packet_sequences = std::collections::HashMap::<String, u64>::new();
                    let mut origins = serde_json::Map::new();
                    let mut owner_ready = false;
                    let mut foreign_ready = false;
                    let mut foreign_started = None;
                    let mut foreign_denied = false;
                    let mut owner_closed = None;
                    let mut foreign_complete = false;
                    let deadline = std::time::Instant::now() + Duration::from_secs(180);
                    while !foreign_complete {
                        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                        let (label, mut message) = received.recv_timeout(remaining).expect("Native media event watchdog expired");
                        if let Some(payload) = message.strip_prefix("plain-packet:") {
                            let packet: ReportPacket = serde_json::from_str(payload).unwrap();
                            packet_sequences.insert(label.clone(), packet.sequence);
                            events.push(json!({"sequence":next_sequence(), "window":label, "packet_sequence":packet.sequence, "packet_kind":packet.kind}));
                            message = format!("plain-{}:{}", packet.kind, packet.payload);
                        }
                        if let Some(payload) = message.strip_prefix("plain-result:") {
                            let result: Value = serde_json::from_str(payload).unwrap();
                            let received_reports = reports.get_mut(&label).unwrap().as_array_mut().unwrap();
                            assert!(received_reports.len() < if label == "media-owner" { 5 } else { 3 }, "Too many media results");
                            received_reports.push(result);
                            continue;
                        }
                        if let Some(error) = message.strip_prefix("plain-harness-error:") {
                            events.push(json!({"sequence":next_sequence(), "window":label, "stage":"script-evaluation-failed", "error":error}));
                            panic!("Native test script failed in {label}: {error}");
                        }
                        if message.starts_with("plain-diagnostic:") {
                            retain_diagnostic(&label, &message, &mut events).unwrap();
                            continue;
                        }
                        let Some(payload) = message.strip_prefix("plain-event:") else { continue; };
                        let event: PageEvent = serde_json::from_str(payload).unwrap();
                        assert_eq!(event.origin, "tauri://localhost", "Test document origin is opaque or unexpected");
                        assert_eq!(event.href, SHELL_URL);
                        assert_eq!(event.transport.pending, 0);
                        assert!(event.transport.failure.is_none());
                        assert_eq!(event.transport.prior_sequence + 1, packet_sequences[&label]);
                        let sequence = page_sequences.entry(label.clone()).or_default();
                        assert_eq!(event.sequence, *sequence + 1, "Page event sequence changed");
                        *sequence = event.sequence;
                        origins.insert(label.clone(), json!(event.origin));
                        events.push(json!({"sequence":next_sequence(), "window":label, "page":event}));
                        match (label.as_str(), event.stage.as_str()) {
                            ("media-owner", "owner-ready") => {
                                assert!(!owner_ready);
                                let owner_window = handle.get_webview_window("media-owner").unwrap();
                                let foreign_window = handle.get_webview_window("media-foreign").unwrap();
                                let owner_position = owner_window.outer_position().unwrap();
                                let foreign_position = foreign_window.outer_position().unwrap();
                                let owner_size = owner_window.outer_size().unwrap();
                                let foreign_size = foreign_window.outer_size().unwrap();
                                assert!(owner_window.is_visible().unwrap() && foreign_window.is_visible().unwrap());
                                assert!(i64::from(owner_position.x) + i64::from(owner_size.width) <= i64::from(foreign_position.x), "Owned test windows overlap");
                                for (position, size) in [(owner_position, owner_size), (foreign_position, foreign_size)] {
                                    assert!(position.x >= 0 && position.y >= 0 && i64::from(position.x) + i64::from(size.width) <= 1024
                                        && i64::from(position.y) + i64::from(size.height) <= 768, "Owned test window escaped the Xvfb surface");
                                }
                                events.push(json!({"sequence":next_sequence(), "stage":"window-layout-verified", "visible":true, "nonoverlapping":true,
                                    "owner_position":owner_position, "owner_size":owner_size, "foreign_position":foreign_position, "foreign_size":foreign_size}));
                                let received_reports = reports[&label].as_array().unwrap();
                                assert_eq!(received_reports.len(), 5);
                                let decoded = received_reports.last().unwrap();
                                assert_eq!(decoded["kind"], "image");
                                assert_eq!(decoded["status"], "passed", "Owner image must decode before foreign access");
                                assert_eq!(decoded["retained"], true);
                                owner_ready = true;
                            }
                            ("media-foreign", "foreign-ready") => {
                                assert!(!foreign_ready);
                                assert!(reports[&label].as_array().unwrap().is_empty(), "Foreign window requested resources before owner readiness");
                                foreign_ready = true;
                            }
                            ("media-foreign", "foreign-denied") => {
                                assert!(!foreign_denied);
                                let received_reports = reports[&label].as_array().unwrap();
                                assert_eq!(received_reports.len(), 1);
                                assert_eq!(received_reports[0]["kind"], "foreign-owner");
                                assert_eq!(received_reports[0]["status"], "passed", "Foreign image unexpectedly loaded");
                                let records = REQUESTS.get().unwrap().lock().unwrap().clone();
                                assert!(response_after(&records, &foreign_id, &image.url, 403, foreign_started.unwrap()).is_some(),
                                    "Image error without a production 403 is not authorization evidence");
                                foreign_denied = true;
                            }
                            ("media-foreign", "foreign-complete") => {
                                assert!(owner_closed.is_some());
                                let received_reports = reports[&label].as_array().unwrap();
                                assert_eq!(received_reports.len(), 3);
                                assert_eq!(received_reports[1]["kind"], "closed-owner");
                                assert_eq!(received_reports[2]["kind"], "image");
                                foreign_complete = true;
                            }
                            _ => panic!("Unexpected native acceptance stage"),
                        }
                        if owner_ready && foreign_ready && foreign_started.is_none() {
                            let records = REQUESTS.get().unwrap().lock().unwrap().clone();
                            let owner_response = response_after(&records, &owner_id, &image.url, 200, 0)
                                .expect("Owner decode lacks a production 200 response");
                            assert!(records.iter().any(|record| record["response_sequence"] == owner_response
                                && record["bytes"] == image.size && record["content_type"] == "image/png"));
                            let sequence = next_sequence();
                            assert!(owner_response < sequence);
                            events.push(json!({"sequence":sequence, "stage":"foreign-request-start", "owner_response_sequence":owner_response}));
                            foreign_started = Some(sequence);
                            handle.get_webview_window("media-foreign").unwrap().eval("window.afterOwnerWarm()").unwrap();
                        }
                        if foreign_denied && owner_closed.is_none() {
                            let state = report_transport_idle(&handle.get_webview_window("media-owner").unwrap()).unwrap();
                            transport_states.insert("media-owner".into(), state);
                            events.push(json!({"sequence":next_sequence(), "stage":"owner-close-requested"}));
                            handle.get_webview_window("media-owner").unwrap().destroy().unwrap();
                            let limit = std::time::Instant::now() + Duration::from_secs(10);
                            while !owner.is_closed() && std::time::Instant::now() < limit {
                                std::thread::sleep(Duration::from_millis(20));
                            }
                            assert!(owner.is_closed(), "Destroyed WebView scope remained open");
                            let sequence = next_sequence();
                            owner_closed = Some(sequence);
                            events.push(json!({"sequence":sequence, "stage":"owner-closed"}));
                            handle.get_webview_window("media-foreign").unwrap().eval("window.afterOwnerClose()").unwrap();
                        }
                    }
                    let settled = REQUESTS.get().unwrap().lock().unwrap().len();
                    let settling_deadline = std::time::Instant::now() + Duration::from_millis(1500);
                    loop {
                        let remaining = settling_deadline.saturating_duration_since(std::time::Instant::now());
                        match received.recv_timeout(remaining) {
                            Ok((label, message)) => retain_diagnostic(&label, &message, &mut events).unwrap(),
                            Err(mpsc::RecvTimeoutError::Timeout) => break,
                            Err(mpsc::RecvTimeoutError::Disconnected) => panic!("Report channel stopped before cleanup"),
                        }
                    }
                    let records = REQUESTS.get().unwrap().lock().unwrap().clone();
                    let complete = reports.get("media-owner").and_then(Value::as_array).is_some_and(|v| v.len() == 5)
                        && reports.get("media-foreign").and_then(Value::as_array).is_some_and(|v| v.len() == 3);
                    let cases_passed = complete && reports.values().flat_map(|v| v.as_array().unwrap()).all(|v| v["status"] == "passed");
                    let forbidden = response_after(&records, &foreign_id, &image.url, 403, foreign_started.unwrap());
                    let revoked = response_after(&records, &foreign_id, &image.url, 404, owner_closed.unwrap());
                    let control = response_after(&records, &foreign_id, &control_image.url, 200, revoked.unwrap_or(u64::MAX));
                    let png_no_store = records.iter()
                        .filter(|record| record["url"] == image.url || record["url"] == control_image.url)
                        .all(|record| record["response_cache_control"] == "no-store");
                    let bounded_responses = records.iter().all(|r| r["bytes"].as_u64().is_some_and(|bytes| {
                        if bytes > crate::native_resource_protocol::MAX_RESPONSE_BYTES { return false; }
                        if r["status"] != 206 { return true; }
                        let Some(value) = r["content_range"].as_str().and_then(|v| v.strip_prefix("bytes ")) else { return false; };
                        let Some((range, total)) = value.split_once('/') else { return false; };
                        let Some((start, end)) = range.split_once('-') else { return false; };
                        let (Ok(start), Ok(end), Ok(total)) = (start.parse::<u64>(), end.parse::<u64>(), total.parse::<u64>()) else { return false; };
                        let expected = [&image, &control_image].into_iter()
                            .find(|resource| r["url"] == resource.url).map(|resource| resource.size);
                        start <= end && end < total && Some(total) == expected && end - start + 1 == bytes
                    }));
                    let passed = cases_passed && forbidden.is_some() && revoked.is_some() && control.is_some()
                        && png_no_store && bounded_responses && records.len() == settled;
                    json!({"status": if passed {"passed"} else {"failed"}, "reports": reports,
                        "events":events, "page_origins":origins, "report_transport":transport_states,
                        "shared_profile":"owned data/webview-profile", "same_nonopaque_origin":true,
                        "http_route_probes":http_probes,
                        "native_scheme":{"verification_method":"production-scheme-observer", "requests":records,
                            "foreign_handle_denied":forbidden.is_some(), "closed_owner_denied":revoked.is_some(),
                            "foreign_owned_image_survived":control.is_some(),
                            "png_responses_no_store":png_no_store,
                            "cache_hit_observed":Value::Null,
                            "cache_scope":"same profile/origin; owner decoded before foreign request; renderer sharing and cache hits are not directly observed",
                            "bounded_exact_range_lengths":bounded_responses,
                            "requests_stopped_after_cancel":records.len() == settled},
                        "large_media_route":"production-core-http-fs",
                        "small_media_route":"production-core-http-fs",
                        "large_native_scheme_playback":"unsupported; existing non-Range 413 unit boundary retained",
                        "webview_http_headers_observed":false,
                        "inflight_read_cancellation":"not exposed by the current Tauri responder"})
                }));
                let mut value = outcome.unwrap_or_else(|panic| {
                    let error = panic.downcast_ref::<String>().cloned().or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_else(|| "Native controller panic".into());
                    json!({"status":"failed", "error":error, "reports":reports, "events":events,
                        "http_route_probes":http_probes,
                        "requests":REQUESTS.get().unwrap().lock().unwrap().clone()})
                });
                if value["status"] != "passed" {
                    let mut transport = serde_json::Map::new();
                    for label in ["media-owner", "media-foreign"] {
                        let Some(window) = handle.get_webview_window(label) else { continue; };
                        let (sender, receiver) = mpsc::channel();
                        let scheduled = window.eval_with_callback(
                            "window.__plainTransportState ? window.__plainTransportState() : { failure: 'report script unavailable' }",
                            move |result| { let _ = sender.send(result); });
                        let state = if scheduled.is_err() { json!({"failure":"diagnostic evaluation unavailable"}) }
                            else { match receiver.recv_timeout(Duration::from_secs(2)) {
                                Ok(raw) if raw.len() <= 16384 => serde_json::from_str(&raw).unwrap_or_else(|_| json!({"failure":"diagnostic result was not JSON"})),
                                _ => json!({"failure":"diagnostic result missing or oversized"}),
                            }};
                        transport.insert(label.to_string(), state);
                    }
                    value["report_transport"] = json!(transport);
                }
                let port = core.port;
                let stopped = tauri::async_runtime::block_on(async {
                    tokio::time::timeout(Duration::from_secs(30), core.shutdown()).await.is_ok()
                });
                let listener_closed = std::net::TcpStream::connect_timeout(
                    &std::net::SocketAddr::from(([127, 0, 0, 1], port)), Duration::from_millis(250)).is_err();
                if value["status"] == "passed" {
                    match handle.get_webview_window("media-foreign")
                        .ok_or_else(|| "Final report window is missing".to_string())
                        .and_then(|window| report_transport_idle(&window)) {
                        Ok(state) => { value["report_transport"]["media-foreign"] = state; }
                        Err(error) => {
                            value["status"] = json!("failed");
                            value["report_transport_error"] = json!(error);
                        }
                    }
                }
                for (label, message) in received.try_iter() {
                    if let Err(error) = retain_diagnostic(&label, &message, &mut events) {
                        value["status"] = json!("failed");
                        value["late_report_error"] = json!(error);
                    }
                }
                value["events"] = json!(events);
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
