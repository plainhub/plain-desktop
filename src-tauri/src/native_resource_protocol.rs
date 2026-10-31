//! Tauri native resource protocol: serves shared-core file handles to the
//! webview under an opaque-id URL scheme.
//!
//! The scheme never carries a filesystem path. A window first asks the
//! `native_resource_open` command for a handle; the command resolves and
//! stores it in that window's own [`ResourceRegistry`] slot, and the URL it
//! returns embeds only the opaque handle id. Every protocol request is
//! checked against the requesting webview label, so one window can never
//! read another window's handles.

use std::path::Path;
use std::sync::Arc;

use plain_rs::libs::utils::http::{RangeParse, content_disposition, parse_range_header};
use plain_rs::libs::host::request_scope::NativeRequestScope;
pub use plain_server::native::resources::MAX_RESOURCE_RESPONSE_BYTES as MAX_RESPONSE_BYTES;
use plain_server::native::resources::{NativeResourceRegistry, ResourceAccessError};
use serde::Serialize;
use tauri::http::{HeaderValue, Request, Response, StatusCode, header};
use tauri::{Manager, Runtime, UriSchemeContext, UriSchemeResponder};

pub const RESOURCE_SCHEME: &str = "plain-resource";
pub const RESOURCE_HOST: &str = "file";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedResource {
    pub id: String,
    pub url: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
}

#[derive(Default)]
pub struct ResourceRegistry {
    resources: NativeResourceRegistry,
}

impl ResourceRegistry {
    pub async fn open(
        &self,
        scope: &NativeRequestScope,
        path: &Path,
    ) -> Result<OpenedResource, String> {
        let info = self.resources.open(scope, path).await?;
        Ok(OpenedResource {
            url: resource_url(&info.id),
            id: info.id,
            name: info.name,
            mime: info.mime,
            size: info.size,
        })
    }

    pub fn close(&self, scope: &NativeRequestScope, id: &str) -> bool {
        self.resources.close(scope, id)
    }

    pub fn close_owner(&self, owner: &str) {
        self.resources.close_owner(owner);
    }
}

pub fn resource_url(id: &str) -> String {
    format!("{RESOURCE_SCHEME}://{RESOURCE_HOST}/{id}")
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

fn error_response(status: StatusCode) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_LENGTH, 0)
        .body(Vec::new())
        .expect("static error response")
}

pub async fn serve(
    registry: &ResourceRegistry,
    scope: &NativeRequestScope,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let Some(id) = requested_id(&request) else {
        responder.respond(error_response(StatusCode::NOT_FOUND));
        return;
    };
    let resource = match registry.resources.get(scope, &id) {
        Ok(resource) => resource,
        Err(error) => {
            let status = match error {
                ResourceAccessError::Unavailable => StatusCode::NOT_FOUND,
                ResourceAccessError::Forbidden => StatusCode::FORBIDDEN,
            };
            responder.respond(error_response(status));
            return;
        }
    };
    let size = resource.info.size;

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
    let length = if size == 0 { 0 } else { end - start + 1 };
    if length > MAX_RESPONSE_BYTES {
        responder.respond(error_response(StatusCode::PAYLOAD_TOO_LARGE));
        return;
    }

    let bytes = match resource.read_range(start, length).await {
        Ok(bytes) => bytes,
        Err(_) => {
            registry.close(scope, &id);
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
        .header(header::CONTENT_TYPE, header_value(&resource.info.mime))
        .header(header::CONTENT_LENGTH, bytes.len())
        .header(header::ACCEPT_RANGES, header_value("bytes"))
        .header(
            header::CONTENT_DISPOSITION,
            header_value(&content_disposition("inline", &resource.info.name)),
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

pub fn with_native_resource_protocol<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    let registry = Arc::new(ResourceRegistry::default());
    let handler = registry.clone();
    builder
        .manage(registry)
        .register_asynchronous_uri_scheme_protocol(
            RESOURCE_SCHEME,
            move |context: UriSchemeContext<'_, R>, request, responder| {
                let handler = handler.clone();
                let Some(views) = context
                    .app_handle()
                    .try_state::<crate::commands::native::views::NativeViews>()
                else {
                    responder.respond(error_response(StatusCode::NOT_FOUND));
                    return;
                };
                let scope = views.scope(context.webview_label());
                tauri::async_runtime::spawn(async move {
                    serve(&handler, &scope, request, responder).await;
                });
            },
        )
}
