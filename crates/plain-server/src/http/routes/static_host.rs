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

/// What a client path means once it has been normalised.
enum Requested {
    /// A path relative to the bundle root.
    Relative(PathBuf),
    /// The bare root, which is the SPA shell like any extension-less route.
    Root,
    /// A path that must never reach the filesystem.
    Rejected,
}

/// The bundle root is a host-provided directory: everything the client sends
/// must stay inside it. Normalising first and rejecting `..`/absolute/prefix
/// components keeps a crafted path from ever reaching the filesystem; a
/// rejected path is a 404, never the SPA shell.
fn classify(requested: &str) -> Requested {
    if requested.contains('\\') {
        return Requested::Rejected;
    }
    let mut path = PathBuf::new();
    for segment in requested.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return Requested::Rejected;
        }
        path.push(segment);
    }
    match path.components().next() {
        Some(Component::Normal(_)) => Requested::Relative(path),
        None => Requested::Root,
        _ => Requested::Rejected,
    }
}

/// The bundle root with every symlink in it resolved, so a host that hands
/// over a symlinked directory still serves its files.
fn canonical_root(root: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(root).ok()
}

/// The file a request resolves to, or `None` when it is not a file or leaves
/// the bundle through a symlink. Resolving before comparing is what keeps an
/// in-bundle symlink usable while an escaping one is refused.
fn bundle_file(root: &Path, relative: &Path) -> Option<PathBuf> {
    let resolved = std::fs::canonicalize(root.join(relative)).ok()?;
    resolved
        .starts_with(root)
        .then_some(resolved)
        .filter(|path| path.is_file())
}

fn has_extension(relative: &Path) -> bool {
    relative
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.contains('.'))
}

/// The SPA shell, which is itself subject to the boundary check so a
/// symlinked `index.html` cannot export a file from outside the bundle.
fn shell(root: &Path) -> Option<(PathBuf, String)> {
    Some((
        bundle_file(root, Path::new("index.html"))?,
        "index.html".to_string(),
    ))
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

pub(crate) async fn serve(
    prefs: &plain_rs::prefs::Prefs,
    root: Option<&Path>,
    method: &Method,
    path: &str,
) -> Option<Response> {
    if method != Method::GET && method != Method::HEAD {
        return None;
    }
    if !(prefs.get_user_or("desktop_access", true)) {
        return None;
    }
    let root = canonical_root(root?)?;
    let requested = path.split(['?', '#']).next().unwrap_or_default();
    let requested = requested.strip_prefix('/').unwrap_or(requested);
    // Anything that is not a real file, and any extension-less path, is a
    // client-side route and must fall back to the SPA shell.
    let (served, resource) = match classify(requested) {
        Requested::Rejected => return None,
        Requested::Root => shell(&root)?,
        Requested::Relative(relative) => match bundle_file(&root, &relative) {
            Some(path) => (path, requested.to_owned()),
            None if has_extension(&relative) => return None,
            None => shell(&root)?,
        },
    };
    let bytes = tokio::fs::read(&served).await.ok()?;
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

pub(crate) async fn fallback(
    State(state): State<crate::assembly::server::ServerState>,
    request: Request,
) -> Response {
    let root = state.web_root.read().ok().and_then(|guard| guard.clone());
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
#[path = "../../../tests/unit/http/routes/static_host.rs"]
mod tests;
