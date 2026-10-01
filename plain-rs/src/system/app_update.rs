//! App-update check. 1:1 port of `internal/graph/app_update_api.go`.
//!
//! Polls GitHub's `releases/latest` endpoint for the `ismartcoding/plainnas`
//! repo, caches the result in-process for 10 minutes, and exposes a
//! synchronous `app_update` snapshot.
//!
//! Network errors, non-200 responses, and JSON decode failures all degrade
//! gracefully: we return the current version with `hasUpdate=false` and cache
//! that fallback for the standard TTL.

use parking_lot::Mutex;
use serde::Deserialize;
use std::time::{Duration, Instant};

const GITHUB_URL: &str = "https://api.github.com/repos/ismartcoding/plainnas/releases/latest";
const CACHE_TTL: Duration = Duration::from_secs(10 * 60);
const HTTP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug)]
pub struct AppUpdate {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub has_update: bool,
    pub url: Option<String>,
}

#[derive(Deserialize)]
struct GhRelease {
    #[serde(rename = "tag_name")]
    tag_name: String,
    #[serde(rename = "html_url")]
    html_url: String,
}

#[derive(Default)]
struct Cache {
    value: Option<AppUpdate>,
    fetched_at: Option<Instant>,
}

static CACHE: std::sync::LazyLock<Mutex<Cache>> =
    std::sync::LazyLock::new(|| Mutex::new(Cache::default()));

/// Get the current `appUpdate` snapshot. Caches for 10 min; falls back to
/// "current version only, hasUpdate=false" on network or parse error.
pub fn app_update() -> AppUpdate {
    {
        let c = CACHE.lock();
        if let (Some(v), Some(t)) = (&c.value, c.fetched_at) {
            if t.elapsed() < CACHE_TTL {
                return v.clone();
            }
        }
    }

    let cur = crate::system::version::version().to_string();
    let mut res = AppUpdate {
        current_version: normalize_version(&cur),
        latest_version: None,
        has_update: false,
        url: None,
    };

    // Sync HTTP via ureq. 5-second timeout.
    let agent = ureq::AgentBuilder::new().timeout(HTTP_TIMEOUT).build();
    let req = agent
        .get(GITHUB_URL)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", "plainnas");
    if let Ok(resp) = req.call() {
        if resp.status() == 200 {
            if let Ok(body) = resp.into_string() {
                if let Ok(gh) = serde_json::from_str::<GhRelease>(&body) {
                    let latest = normalize_version(&gh.tag_name);
                    res.latest_version = optional_string(&latest);
                    res.url = optional_string(gh.html_url.trim());
                    res.has_update = has_newer_version(&res.current_version, &latest);
                }
            }
        }
    }

    let mut c = CACHE.lock();
    c.value = Some(res.clone());
    c.fetched_at = Some(Instant::now());
    res
}

/// Strip `PlainNAS`, `v`, build metadata (`+...`), prerelease (`-...`).
/// Mirrors Go `normalizeVersion`.
pub fn normalize_version(v: &str) -> String {
    let trimmed = v.trim();
    let stripped = trimmed.strip_prefix("PlainNAS").unwrap_or(trimmed).trim();
    let no_v = stripped.strip_prefix('v').unwrap_or(stripped).trim();
    if let Some(i) = no_v.find(|c: char| c == '+' || c == '-') {
        no_v[..i].to_string()
    } else {
        no_v.to_string()
    }
}

fn optional_string(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

pub fn parse_semver(v: &str) -> Option<[u64; 3]> {
    let s = normalize_version(v);
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() < 3 {
        return None;
    }
    let mut out = [0u64; 3];
    for (i, p) in parts.iter().take(3).enumerate() {
        out[i] = p.parse().ok()?;
    }
    Some(out)
}

/// Returns true if `latest` is strictly newer than `current`. Mirrors Go
/// `hasNewerVersion`. Falls back to a "different non-empty ⇒ newer" check
/// when either side isn't a clean semver.
pub fn has_newer_version(current: &str, latest: &str) -> bool {
    if let (Some(c), Some(l)) = (parse_semver(current), parse_semver(latest)) {
        if l[0] != c[0] {
            return l[0] > c[0];
        }
        if l[1] != c[1] {
            return l[1] > c[1];
        }
        return l[2] > c[2];
    }
    if current.trim().is_empty() || latest.trim().is_empty() {
        return false;
    }
    normalize_version(current) != normalize_version(latest)
}

#[cfg(test)]
#[path = "../../tests/unit/system/app_update.rs"]
mod tests;
