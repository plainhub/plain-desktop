//! SPA static assets for the public web UI.
//!
//! The Vue bundle is 11 MB / 1000+ files and ships inside the APK / iOS
//! bundle, which the Rust crate cannot read on its own. The host extracts it
//! once into a version-stamped directory under the app data dir and passes
//! that path to `start_public`; everything after that — path safety, SPA
//! fallback, cache headers, the `__SERVER_TIME__` bootstrap — is decided here.
//!
//! Embedding the bundle in the crate instead would duplicate 11 MB in every
//! `.so`/framework and force a full plain-rs rebuild on every web-only change.

use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderValue, Method, StatusCode, header},
    response::Response,
};
use std::path::{Component, Path, PathBuf};

/// Assets under these prefixes are content-hashed by the bundler.
const IMMUTABLE_PREFIXES: &[&str] = &["assets/", "ficons/", "icons/"];

fn content_type(resource: &str) -> &'static str {
    match resource.rsplit_once('.').map(|(_, ext)| ext) {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json" | "webmanifest") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("ico") => "image/x-icon",
        Some("webp") => "image/webp",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("txt") => "text/plain; charset=utf-8",
        Some("xml") => "text/xml; charset=utf-8",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    }
}

fn cache_control(requested: &str) -> &'static str {
    if IMMUTABLE_PREFIXES
        .iter()
        .any(|prefix| requested.starts_with(prefix))
    {
        "public, max-age=31536000"
    } else {
        "no-cache, no-store"
    }
}

/// The bundle root is a host-provided directory: everything the client sends
/// must stay inside it. Normalising first and rejecting `..`/absolute/prefix
/// components keeps a crafted path from ever reaching the filesystem.
fn safe_relative(requested: &str) -> Option<PathBuf> {
    if requested.contains('\\') {
        return None;
    }
    let mut path = PathBuf::new();
    for segment in requested.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return None;
        }
        path.push(segment);
    }
    match path.components().next() {
        Some(Component::Normal(_)) => Some(path),
        _ => None,
    }
}

/// The SPA shell must learn the server time before the app boots, so the
/// script is injected right after `<head>` exactly like the Kotlin path did.
fn inject_server_time(html: &str, now_ms: i64) -> String {
    let script = format!("<script>window.__SERVER_TIME__={now_ms}</script>");
    match html.find("<head>") {
        Some(index) => {
            let (head, tail) = html.split_at(index + "<head>".len());
            format!("{head}{script}{tail}")
        }
        None => format!("{script}{html}"),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis() as i64)
        .unwrap_or_default()
}

pub(super) async fn serve(
    prefs: &crate::prefs::Prefs,
    root: Option<&Path>,
    method: &Method,
    path: &str,
) -> Option<Response> {
    if method != Method::GET && method != Method::HEAD {
        return None;
    }
    if !(prefs.get_user_or("service", false) && prefs.get_user_or("desktop_access", true)) {
        return None;
    }
    let root = root?;
    let requested = path.split(['?', '#']).next().unwrap_or_default();
    let requested = requested.strip_prefix('/').unwrap_or(requested);
    // Anything that is not a real file, and any extension-less path, is a
    // client-side route and must fall back to the SPA shell.
    let relative = safe_relative(requested);
    let direct = relative
        .as_ref()
        .map(|path| Path::new(&root).join(path))
        .filter(|path| path.is_file());
    let (served, resource) = match direct {
        Some(path) => (Some(path), requested.to_owned()),
        None => {
            let has_extension = relative
                .as_ref()
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains('.'));
            if has_extension {
                return None;
            }
            (
                Some(Path::new(&root).join("index.html")),
                "index.html".to_owned(),
            )
        }
    };
    let path = served?;
    let bytes = tokio::fs::read(&path).await.ok()?;
    let (body, cache) = if resource.ends_with(".html") {
        (
            inject_server_time(&String::from_utf8_lossy(&bytes), now_ms()).into_bytes(),
            "no-cache, no-store",
        )
    } else {
        (bytes, cache_control(&resource))
    };
    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type(&resource))
        .header(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    if resource == "index.html" {
        response = response.header(
            "Cross-Origin-Opener-Policy",
            HeaderValue::from_static("same-origin"),
        );
    }
    response.body(Body::from(body)).ok()
}

pub(super) async fn fallback(
    State(state): State<super::server::ServerState>,
    request: Request,
) -> Response {
    let root = state
        .web_root
        .read()
        .ok()
        .and_then(|guard| guard.clone());
    serve(
        &state.prefs,
        root.as_deref(),
        request.method(),
        request.uri().path(),
    )
    .await
        .unwrap_or_else(|| {
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::empty())
                .unwrap()
        })
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_static.rs"]
mod tests;
