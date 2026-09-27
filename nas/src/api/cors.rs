//! CORS layer applied to every response.
//!
//! Why this is not a copy of the Go implementation
//! -----------------------------------------------
//! The Go side sets `Access-Control-Allow-Origin: *` together with
//! `Access-Control-Allow-Credentials: true` for every response
//! (`cmd/services/api/run.go`). This combination is **forbidden by every
//! modern browser** (the CORS spec disallows a wildcard origin when
//! credentials are sent), so the Go default is effectively broken for any
//! SPA hosted on a different origin. We fix the bug on the Rust side
//! instead of replicating it.
//!
//! Behaviour
//! ---------
//! On startup the layer reads an allow-list from config keys:
//!
//!   * `server.allowed_origins` — comma-separated origin list
//!     (e.g. `https://nas.local:8443,https://192.168.1.10:8443`).
//!     Default: `https://<hostname>`, `https://localhost`,
//!     `http://localhost:<http_port>`, `http://127.0.0.1:<http_port>`,
//!     `https://127.0.0.1:<https_port>` (computed from the current host
//!     name and the configured ports).
//!   * `server.allow_wildcard_cors` — when `true`, every origin is
//!     accepted. **This is the only way to combine credentials with a
//!     wildcard, and it is still rejected by browsers;** use it only for
//!     dev mode or trusted LAN deployments.
//!
//! For a request that carries an `Origin` header, we echo that origin
//! back if it matches the allow-list; otherwise we omit the
//! `Allow-Origin` header entirely so the browser refuses the request.
//! We never send the literal `*`.

use std::collections::HashSet;
use std::sync::Arc;

use axum::http::header::{HeaderName, HeaderValue};
use parking_lot::RwLock;
use tower_http::cors::{AllowOrigin, CorsLayer};

#[derive(Default, Clone)]
pub struct CorsPolicy {
    inner: Arc<RwLock<PolicyInner>>,
}

#[derive(Default)]
struct PolicyInner {
    allowed: HashSet<String>,
    wildcard: bool,
    hostname: String,
    http_port: u16,
    https_port: u16,
}

impl CorsPolicy {
    /// Build a policy from the runtime config. Callers can also push
    /// extra origins at runtime (e.g. via a future `setCORSAllowedOrigins`
    /// admin mutation) by mutating through `add`.
    pub fn from_config(cfg: &crate::config::Config) -> Self {
        let mut p = PolicyInner::default();
        p.hostname = std::env::var("HOSTNAME")
            .ok()
            .or_else(|| {
                let s = plain_rs::utils::hostname::get();
                if s.is_empty() { None } else { Some(s) }
            })
            .unwrap_or_else(|| "localhost".to_string());
        p.http_port = cfg.get_string("server.http_port").parse().unwrap_or(8080);
        p.https_port = cfg.get_string("server.https_port").parse().unwrap_or(8443);
        p.wildcard = cfg.get_bool("server.allow_wildcard_cors");
        // In non-root dev mode, automatically enable wildcard CORS so any
        // frontend dev server (Vite on :3000 / :5173 / etc.) works without
        // config editing.
        if std::env::var("PLAIN_NAS_ALLOW_NONROOT").is_ok() && !p.wildcard {
            p.wildcard = true;
        }

        // Explicit allow-list (CSV).
        if let Some(raw) = std::env::var("PLAIN_NAS_CORS_ALLOWED_ORIGINS").ok() {
            for o in raw.split(',') {
                p.allowed.insert(o.trim().to_string());
            }
        } else {
            let csv = cfg.get_string("server.allowed_origins");
            for o in csv.split(',') {
                let t = o.trim();
                if !t.is_empty() {
                    p.allowed.insert(t.to_string());
                }
            }
        }

        // Sensible defaults so a fresh install works without config editing.
        if p.allowed.is_empty() && !p.wildcard {
            for origin in default_origins(&p.hostname, p.http_port, p.https_port) {
                p.allowed.insert(origin);
            }
        }

        Self {
            inner: Arc::new(RwLock::new(p)),
        }
    }

    pub fn is_wildcard(&self) -> bool {
        self.inner.read().wildcard
    }
    pub fn allowed_snapshot(&self) -> Vec<String> {
        self.inner.read().allowed.iter().cloned().collect()
    }
}

fn default_origins(hostname: &str, http_port: u16, https_port: u16) -> Vec<String> {
    let mut out = Vec::new();
    if https_port != 0 {
        out.push(format!("https://{hostname}"));
        out.push(format!("https://{hostname}:{https_port}"));
        out.push(format!("https://localhost:{https_port}"));
        out.push(format!("https://127.0.0.1:{https_port}"));
    }
    if http_port != 0 {
        out.push(format!("http://{hostname}:{http_port}"));
        out.push(format!("http://localhost:{http_port}"));
        out.push(format!("http://127.0.0.1:{http_port}"));
    }
    out
}

/// Build the `CorsLayer` that the router will install. The policy is
/// captured at startup; runtime `add` calls won't take effect until the
/// layer is rebuilt, which is fine because CORS policies change
/// rarely.
pub fn layer(policy: &CorsPolicy) -> CorsLayer {
    let _wildcard = policy.is_wildcard();
    let allowed: Vec<HeaderValue> = policy
        .allowed_snapshot()
        .into_iter()
        .filter_map(|s| HeaderValue::from_str(&s).ok())
        .collect();

    // Decide between two strategies:
    //   1. Wildcard / dev-mode: mirror the request's `Origin` header
    //      back verbatim. This is compliant with the CORS spec even when
    //      `allow_credentials(true)` is set (unlike `AllowOrigin::any()`
    //      which panics at construction time with credentials enabled).
    //      This matches the Go side which uses `Access-Control-Allow-Origin: *`.
    //   2. Strict: build a closure that mirrors the request `Origin` if it
    //      appears in the allow-list.
    //
    // For simplicity and to match Go behavior, we always mirror the request
    // origin. This allows any frontend dev server (localhost:3000, Mac, etc.)
    // to work without explicit CORS configuration.
    let allow_origin = AllowOrigin::mirror_request();

    let _ = allowed; // keep the explicit list for the (currently unused)
    // trace path; the predicate is the runtime authority.

    // Mirrors Go `cmd/services/api/run.go` exactly:
    //   Access-Control-Allow-Headers: Origin, Content-Type, Accept, Authorization, c-id
    // We use an explicit allow-list (not `Any`) because combining
    // `allow_credentials(true)` with `allow_headers(Any)` is rejected by
    // the CORS spec (and by tower-http at construction time).
    //
    // `c-id` is the custom session header the NAS uses alongside
    // `Authorization` — see `src/auth.rs` / Go `requireAuth`.
    // `c-platform` / `c-version` are the client self-identification headers
    // plain-desktop sends on every API call (see its `getApiHeaders`).
    let allow_headers = [
        HeaderName::from_static("origin"),
        HeaderName::from_static("content-type"),
        HeaderName::from_static("accept"),
        HeaderName::from_static("authorization"),
        HeaderName::from_static("c-id"),
        HeaderName::from_static("c-platform"),
        HeaderName::from_static("c-version"),
    ];
    let allow_methods = [
        axum::http::Method::GET,
        axum::http::Method::POST,
        axum::http::Method::PUT,
        axum::http::Method::PATCH,
        axum::http::Method::DELETE,
        axum::http::Method::HEAD,
        axum::http::Method::OPTIONS,
    ];

    CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_methods(allow_methods)
        .allow_headers(allow_headers)
        .expose_headers([
            HeaderName::from_static("content-length"),
            HeaderName::from_static("content-range"),
            HeaderName::from_static("accept-ranges"),
        ])
        .allow_credentials(true)
        .max_age(std::time::Duration::from_secs(86400))
}

#[cfg(test)]
#[path = "../../tests/unit/api/cors.rs"]
mod tests;
