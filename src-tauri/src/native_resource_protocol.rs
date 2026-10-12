//! Tauri native resource protocol: serves shared-core file handles to the
//! webview under an opaque-id URL scheme.
//!
//! The scheme never carries a filesystem path. A window first asks the
//! `native_resource_open` command for a handle; the command resolves and
//! stores it in that window's own [`ResourceRegistry`] slot, and the URL it
//! returns embeds only the opaque handle id. Every protocol request is
//! checked against the requesting webview label, so one window can never
//! read another window's handles.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use plain_rs::utils::http::{RangeParse, parse_range_header};
use plain_server::native::resources::{MAX_RESOURCE_READ_BYTES, NativeFileResources};
use serde::Serialize;
use tauri::http::{HeaderValue, Request, Response, StatusCode, header};
use tauri::{Manager, Runtime, UriSchemeContext, UriSchemeResponder};

pub const RESOURCE_SCHEME: &str = "plain-resource";
pub const RESOURCE_HOST: &str = "file";

/// Largest single protocol response. Tauri 2's
/// `UriSchemeResponder::respond` only accepts a fully buffered
/// `Cow<'static, [u8]>` body — there is no streaming response — so an
/// oversized request is refused instead of being aggregated in memory.
pub const MAX_RESPONSE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedResource {
    pub id: String,
    pub url: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
}

struct Handle {
    owner: String,
    name: String,
    mime: String,
    size: u64,
}

/// Per-window handle stores. Each owner (a webview label) gets its own
/// [`NativeFileResources`] so closing a window closes exactly its own
/// handles and wakes only its own pending reads.
#[derive(Default)]
pub struct ResourceRegistry {
    stores: Mutex<HashMap<String, Arc<NativeFileResources>>>,
    handles: Mutex<HashMap<String, Handle>>,
}

impl ResourceRegistry {
    pub fn open(&self, owner: &str, path: &Path) -> Result<OpenedResource, String> {
        let store = {
            let mut stores = self.stores.lock().unwrap();
            stores
                .entry(owner.to_string())
                .or_insert_with(|| Arc::new(NativeFileResources::default()))
                .clone()
        };
        let info = store.open(path).await;
        let info = match info {
            Ok(info) => info,
            Err(error) => return Err(error),
        };
        self.handles.lock().unwrap().insert(
            info.id.clone(),
            Handle {
                owner: owner.to_string(),
                name: info.name.clone(),
                mime: info.mime.clone(),
                size: info.size,
            },
        );
        Ok(OpenedResource {
            id: info.id,
            url: resource_url(&info.id),
            name: info.name,
            mime: info.mime,
            size: info.size,
        })
    }

    pub fn close(&self, owner: &str, id: &str) -> bool {
        let owned = self
            .handles
            .lock()
            .unwrap()
            .get(id)
            .is_some_and(|handle| handle.owner == owner);
        if !owned {
            return false;
        }
        self.handles.lock().unwrap().remove(id);
        if let Some(store) = self.stores.lock().unwrap().get(owner) {
            store.close(id);
        }
        true
    }

    /// Drop every handle a window opened. Wired to the window's destroy
    /// event by the app builder; the protocol also calls it for owners
    /// whose webview is already gone.
    pub fn close_owner(&self, owner: &str) {
        let ids: Vec<String> = self
            .handles
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, handle)| handle.owner == owner)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            self.close(owner, &id);
        }
        if let Some(store) = self.stores.lock().unwrap().remove(owner) {
            store.close_all();
        }
    }

    #[allow(clippy::type_complexity)]
    fn handle(&self, id: &str) -> Option<(Arc<NativeFileResources>, String, String, String, u64)> {
        let handles = self.handles.lock().unwrap();
        let handle = handles.get(id)?;
        let store = self.stores.lock().unwrap().get(&handle.owner)?.clone();
        Some((
            store,
            handle.owner.clone(),
            handle.name.clone(),
            handle.mime.clone(),
            handle.size,
        ))
    }
}

pub fn resource_url(id: &str) -> String {
    format!("{RESOURCE_SCHEME}://{RESOURCE_HOST}/{id}")
}

pub fn is_resource_url(url: &str) -> bool {
    url.starts_with(&format!("{RESOURCE_SCHEME}://"))
}

/// The opaque handle id is the last path segment of the request URI; the
/// scheme and host are fixed and carry no caller-controlled input.
fn requested_id(request: &Request<Vec<u8>>) -> Option<String> {
    let id = request.uri().path().rsplit('/').next()?;
    if id.is_empty() || id.contains("..") {
        return None;
    }
    Some(percent_decode(id))
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn header_value(value: &str) -> HeaderValue {
    HeaderValue::from_str(value).unwrap_or(HeaderValue::from_static(""))
}

fn content_disposition(name: &str) -> String {
    format!(
        "inline; filename=\"{}\"; filename*=UTF-8''{}",
        name.replace(['\\', '"'], "_"),
        name.replace(['\\', '"', ' ', '%'], "_")
    )
}

fn error_response(status: StatusCode) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_LENGTH, 0)
        .body(Vec::new())
        .expect("static error response")
}

/// Read `length` bytes starting at `offset` in bounded chunks, so no single
/// allocation exceeds the shared core's per-read limit.
async fn read_range(
    store: &NativeFileResources,
    id: &str,
    offset: u64,
    length: u64,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::with_capacity(length.min(MAX_RESPONSE_BYTES) as usize);
    let mut read_offset = offset;
    while (bytes.len() as u64) < length {
        let remaining = length - bytes.len() as u64;
        let chunk = remaining.min(MAX_RESOURCE_READ_BYTES as u64) as usize;
        let chunk = store.read(id, read_offset, chunk).await?;
        if chunk.is_empty() {
            break;
        }
        read_offset += chunk.len() as u64;
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub async fn serve<R: Runtime>(
    registry: &ResourceRegistry,
    webview_label: &str,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let Some(id) = requested_id(&request) else {
        responder.respond(error_response(StatusCode::NOT_FOUND));
        return;
    };
    let Some((store, owner, name, mime, size)) = registry.handle(&id) else {
        responder.respond(error_response(StatusCode::NOT_FOUND));
        return;
    };
    if owner != webview_label {
        responder.respond(error_response(StatusCode::FORBIDDEN));
        return;
    }

    let range = request
        .headers()
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let (start, end, partial) = match parse_range_header(range, size) {
        RangeParse::Full => (0, size.saturating_sub(1), false),
        RangeParse::Partial(start, end) => (start, end, true),
        RangeParse::Unsatisfiable => {
            let mut response = Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::CONTENT_LENGTH, 0)
                .header(header::CONTENT_RANGE, format!("bytes */{size}"));
            response = response.header(header::ACCEPT_RANGES, header_value("bytes"));
            responder.respond(
                response
                    .body(Vec::new())
                    .expect("static unsatisfiable response"),
            );
            return;
        }
    };
    let length = end.saturating_sub(start).saturating_add(1);
    if length > MAX_RESPONSE_BYTES {
        responder.respond(error_response(StatusCode::PAYLOAD_TOO_LARGE));
        return;
    }

    let bytes = match read_range(&store, &id, start, length).await {
        Ok(bytes) => bytes,
        Err(_) => {
            registry.close(&owner, &id);
            responder.respond(error_response(StatusCode::NOT_FOUND));
            return;
        }
    };
    let mut response = Response::builder()
        .status(if partial {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        })
        .header(header::CONTENT_TYPE, header_value(&mime))
        .header(header::CONTENT_LENGTH, bytes.len())
        .header(header::ACCEPT_RANGES, header_value("bytes"))
        .header(
            header::CONTENT_DISPOSITION,
            header_value(&content_disposition(&name)),
        );
    if partial {
        response = response.header(
            header::CONTENT_RANGE,
            header_value(&format!("bytes {start}-{end}/{size}")),
        );
    }
    match response.body(bytes) {
        Ok(response) => responder.respond(response),
        Err(_) => responder.respond(error_response(StatusCode::INTERNAL_SERVER_ERROR)),
    }
}

/// Register the scheme and manage the shared registry. Codex chains this
/// into the app builder; it is the only wiring the protocol needs.
pub fn with_native_resource_protocol<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    let registry = Arc::new(ResourceRegistry::default());
    let handler = registry.clone();
    builder
        .manage(registry)
        .register_asynchronous_uri_scheme_protocol(
            RESOURCE_SCHEME,
            move |context: UriSchemeContext<'_, R>, request, responder| {
                let handler = handler.clone();
                let label = context.webview_label().to_string();
                tauri::async_runtime::spawn(async move {
                    serve(&handler, &label, request, responder).await;
                });
            },
        )
}
