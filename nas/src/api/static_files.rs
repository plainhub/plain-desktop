//! Static file serving for the Vue build. Mirrors the Go side which
//! uses `embed.FS` to ship `web/dist`.
//!
//! The real build is produced by `bash build-web.sh`, which copies
//! `web/dist/*` into `src/web_dist/` (gitignored — never committed). At
//! runtime the server checks a few on-disk locations (`web/dist/`,
//! `../web/dist/`, `src/web_dist/`) so that `cargo run` during
//! development picks up the latest frontend build without a full rebuild.
//!
//! Asset paths under `/assets`, `/ficons`, `/icons` are served from
//! the dist directory. Single-file endpoints (`/favicon.ico`, `/logo.svg`,
//! `/manifest.json`, `/sw.js`, `/broken-image.png`) prefer the dist
//! version and fall back to an empty body when the dist file is missing.

use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use std::path::PathBuf;

/// Served when no built frontend exists on disk (fresh clone without
/// `build-web.sh`). Kept inline so compilation never depends on the
/// gitignored dist directory.
const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>PlainNAS</title>
<style>body{font-family:system-ui,sans-serif;background:#fff;color:#111;display:grid;place-items:center;height:100vh;margin:0}main{text-align:center}code{background:#f2f2f2;padding:2px 6px;border-radius:4px}</style>
</head>
<body>
<main>
<h1>PlainNAS</h1>
<p>Web UI is not built yet. Run <code>bash build-web.sh</code> and restart.</p>
</main>
</body>
</html>"#;

pub async fn health() -> Response {
    (StatusCode::OK, "ok").into_response()
}

pub async fn index() -> Response {
    // Prefer the on-disk version (picks up `yarn dev` rebuilds), fall
    // back to the inline placeholder for a fresh, unbuilt checkout.
    let body = read_dist_file("index.html").unwrap_or_else(|| INDEX_HTML.to_string());
    let mut resp = (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body,
    )
        .into_response();
    resp.headers_mut()
        .insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());
    resp
}

pub async fn broken_image() -> Response {
    serve_dist_file("broken-image.png", "image/png").await
}
pub async fn favicon() -> Response {
    serve_dist_file("favicon.ico", "image/x-icon").await
}
pub async fn logo() -> Response {
    serve_dist_file("logo.svg", "image/svg+xml").await
}
pub async fn manifest() -> Response {
    serve_dist_file("manifest.json", "application/manifest+json").await
}
pub async fn sw() -> Response {
    serve_dist_file("sw.js", "application/javascript").await
}

/// Serve a file from the dist directory, falling back to an empty
/// placeholder when the file is missing.
async fn serve_dist_file(name: &str, mime: &str) -> Response {
    if let Some(bytes) = read_dist_file_bytes(name) {
        return (StatusCode::OK, [(header::CONTENT_TYPE, mime)], bytes).into_response();
    }
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, mime)],
        Vec::<u8>::new(),
    )
        .into_response()
}

/// Serve a static asset from `/assets/*`, `/ficons/*`, or `/icons/*`.
/// The `prefix` argument is the directory under `web/dist/` (e.g.
/// `"assets"`). The `path` is the remainder of the URL.
pub async fn serve_asset(prefix: &str, path: &str) -> Response {
    // Reject path traversal — the prefix is fixed and `path` should be a
    // relative file name under it.
    if path.contains("..") || path.starts_with('/') {
        return StatusCode::NOT_FOUND.into_response();
    }
    let rel = format!("{prefix}/{path}");
    if let Some(bytes) = read_dist_file_bytes(&rel) {
        let mime = guess_mime_from_path(&rel);
        return (StatusCode::OK, [(header::CONTENT_TYPE, mime)], bytes).into_response();
    }
    StatusCode::NOT_FOUND.into_response()
}

pub async fn serve_assets(Path(path): Path<String>) -> Response {
    serve_asset("assets", &path).await
}
pub async fn serve_ficons(Path(path): Path<String>) -> Response {
    serve_asset("ficons", &path).await
}
pub async fn serve_icons(Path(path): Path<String>) -> Response {
    serve_asset("icons", &path).await
}

/// SPA fallback — returns `index.html` for unknown paths so client-side
/// routing can take over.
pub async fn spa_fallback(_path: String) -> Response {
    index().await
}

/// Try to read a file from the web dist directory. Checks several
/// candidate locations relative to the current working directory so it
/// works both when running from the repo root and from `src/`.
fn dist_root() -> Option<PathBuf> {
    let candidates = ["web/dist", "../web/dist", "../../web/dist", "src/web_dist"];
    for c in &candidates {
        let p = PathBuf::from(c);
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

fn read_dist_file(name: &str) -> Option<String> {
    let root = dist_root()?;
    let full = root.join(name);
    std::fs::read_to_string(&full).ok()
}

fn read_dist_file_bytes(name: &str) -> Option<Vec<u8>> {
    let root = dist_root()?;
    let full = root.join(name);
    std::fs::read(&full).ok()
}

/// Minimal MIME guesser for static assets. Covers the file types the
/// Vue build emits (JS, CSS, fonts, images, JSON, SVG).
fn guess_mime_from_path(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".js") {
        "application/javascript"
    } else if lower.ends_with(".mjs") {
        "application/javascript"
    } else if lower.ends_with(".css") {
        "text/css"
    } else if lower.ends_with(".json") {
        "application/json"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".ico") {
        "image/x-icon"
    } else if lower.ends_with(".woff") {
        "font/woff"
    } else if lower.ends_with(".woff2") {
        "font/woff2"
    } else if lower.ends_with(".ttf") {
        "font/ttf"
    } else if lower.ends_with(".otf") {
        "font/otf"
    } else if lower.ends_with(".eot") {
        "application/vnd.ms-fontobject"
    } else if lower.ends_with(".wasm") {
        "application/wasm"
    } else if lower.ends_with(".map") {
        "application/json"
    } else {
        "application/octet-stream"
    }
}
