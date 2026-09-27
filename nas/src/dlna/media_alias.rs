//! Short-id media alias registry.
//!
//! Mirrors `internal/dlna/media_alias.go`. The flow:
//!
//! 1. The front-end builds a `/fs?id=<short_id>` URL (its file-id format).
//! 2. `Cast` rewrites it to `http://<host>:<http_port>/media/<alias>.<ext>`
//!    so the TV can pull the file from our HTTP port (which it can reach
//!    on the LAN, unlike the encrypted GraphQL port).
//! 3. The `/media/:name` handler resolves `<alias>` back to the original
//!    file path and streams it.
//!
//! Entries are in-memory only with a 30-minute TTL, matching the Go side.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// 30-minute TTL, matching the Go side's `mediaAliasTTL`.
const TTL: Duration = Duration::from_secs(30 * 60);

#[derive(Clone, Debug)]
struct MediaAliasEntry {
    path: String,
    mime: String,
    expires_at: Instant,
}

static ALIASES: Mutex<Option<HashMap<String, MediaAliasEntry>>> = Mutex::new(None);

fn aliases_mut() -> std::sync::MutexGuard<'static, Option<HashMap<String, MediaAliasEntry>>> {
    let mut g = ALIASES.lock().unwrap();
    if g.is_none() {
        *g = Some(HashMap::new());
    }
    g
}

/// Mint a short id for a media path and pick the extension we'll use in
/// the rewritten `/media/<alias>.<ext>` URL. The `(path, mime)` pair is
/// stored in the in-memory registry so the `/media/:name` handler can
/// resolve the alias back to the real file path.
///
/// The id is `nanos since epoch in base 36`; collisions are vanishingly
/// rare on a single-process install (would require two `Cast` calls in
/// the same nanosecond).
pub fn register(path: &str, mime: &str) -> (String, String) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let id = base36(now);
    let ext = path
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .filter(|e| is_safe_ext(e))
        .unwrap_or_else(|| "bin".to_string());

    let entry = MediaAliasEntry {
        path: path.to_string(),
        mime: mime.trim().to_string(),
        expires_at: Instant::now() + TTL,
    };
    let mut g = aliases_mut();
    if let Some(map) = g.as_mut() {
        map.insert(id.clone(), entry);
    }
    (id, ext)
}

/// Resolve a short alias id back to its `(path, mime)`. Returns `None`
/// when the id is unknown or expired. Mirrors Go `lookupMediaAlias`.
pub fn lookup(id: &str) -> Option<(String, String)> {
    let id = id.trim();
    if id.is_empty() {
        return None;
    }
    let mut g = aliases_mut();
    let map = g.as_mut()?;
    let entry = map.get(id).cloned()?;
    if Instant::now() > entry.expires_at {
        map.remove(id);
        return None;
    }
    Some((entry.path, entry.mime))
}

/// Rewrite a `http://host/fs?id=<file_id>` URL to a TV-reachable
/// `http://host:<http_port>/media/<alias>.<ext>` URL. URLs that don't
/// match our `/fs?id=…` shape are returned unchanged. Mirrors Go
/// `dlnaSafeMediaURL(inputURL, mime)`.
///
/// The encrypted `file_id` is resolved to a real path via
/// `fsx::path_from_file_id` (the Rust port of `plainfs.PathFromFileID`),
/// then registered as a short alias.
pub fn safe_media_url(input_url: &str, mime: &str) -> String {
    safe_media_url_with_prefs(input_url, mime, None)
}

/// Same as [`safe_media_url`] but resolves the encrypted `file_id` to a
/// filesystem path using the provided `crate::db::Db` (required for the
/// `url_token` lookup). When `db` is `None`, the encrypted id is used
/// verbatim as the alias key — this is a fallback for tests.
pub fn safe_media_url_with_prefs(
    input_url: &str,
    mime: &str,
    prefs: Option<&crate::prefs::Prefs>,
) -> String {
    let parsed = match plain_rs::utils::http_url::parse_http_url(input_url) {
        Some(u) => u,
        None => return input_url.to_string(),
    };
    if parsed.path != "/fs" {
        return input_url.to_string();
    }
    let id = match parsed.query_param("id") {
        Some(v) => v,
        None => return input_url.to_string(),
    };
    let id = id.trim();
    if id.is_empty() {
        return input_url.to_string();
    }

    // Resolve the encrypted file id to a real filesystem path. The Go
    // side calls `plainfs.PathFromFileID`; here we delegate to
    // `fsx::path_from_file_id`. On any failure we pass the id through
    // unchanged so the caller still gets a usable URL (the TV will get
    // a 404 but the API call doesn't fail).
    let path = match prefs {
        Some(p) => match crate::fsx::path_from_file_id(id, p) {
            Ok(p) => p,
            Err(_) => return input_url.to_string(),
        },
        None => id.to_string(),
    };

    let (alias_id, ext) = register(&path, mime);
    if alias_id.is_empty() {
        return input_url.to_string();
    }

    let host = parsed.host.as_str();
    if host.is_empty() {
        return input_url.to_string();
    }
    let http_port = std::env::var("PLAIN_NAS_HTTP_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok());
    let host_port = match (http_port, parsed.scheme.as_str(), parsed.port) {
        (Some(p), _, _) => format!("{host}:{p}"),
        (None, "http", Some(p)) => format!("{host}:{p}"),
        (None, "http", None) => host.to_string(),
        (None, _, None) => {
            if host.contains(':') {
                format!("[{host}]")
            } else {
                host.to_string()
            }
        }
        (None, _, _) => host.to_string(),
    };
    format!("http://{host_port}/media/{alias_id}.{ext}")
}

fn base36(mut n: u128) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut buf = Vec::new();
    while n > 0 {
        let d = (n % 36) as usize;
        buf.push(digits[d]);
        n /= 36;
    }
    buf.reverse();
    String::from_utf8(buf).unwrap_or_else(|_| "0".to_string())
}

fn is_safe_ext(e: &str) -> bool {
    !e.is_empty()
        && e.len() <= 16
        && e.chars()
            .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase())
}

#[cfg(test)]
#[path = "../../tests/unit/dlna/media_alias.rs"]
mod tests;
