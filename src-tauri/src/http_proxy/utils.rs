use plain_rs::query::percent_decode;

pub(crate) struct ProxyParams {
    /// Request path with `_pt`/`_cid` removed, other params forwarded verbatim.
    pub path: String,
    /// Decoded `_pt` value — the target base URL (`https://ip:port`).
    pub pt: String,
    /// Decoded `_cid` value — a paired peer id whose current `ip:port`
    /// overrides the authority in `pt` before dialing (WebSocket dials).
    pub cid: String,
}

/// Remove `_pt`/`_cid` from the query string; return the cleaned path plus
/// both decoded values.
///
/// The other params are forwarded **verbatim** — no percent-decode. The
/// proxy hands the rebuilt path straight to reqwest, which re-encodes
/// the URL when it talks to the upstream device. Decoding here would
/// turn `+` into a space and corrupt opaque values such as the
/// base64-encrypted `id` parameter used by `/fs` (see
/// `lib/api/file.ts::getFileUrl` and `local/server/file_server.rs`).
///
/// Iteration order over the remaining params is preserved so the
/// returned path is byte-for-byte stable when `_pt` is absent.
pub(crate) fn extract_proxy_params(path: &str) -> ProxyParams {
    let (base, query) = match path.split_once('?') {
        Some((b, q)) => (b, q),
        None => {
            return ProxyParams {
                path: path.to_owned(),
                pt: String::new(),
                cid: String::new(),
            }
        }
    };
    let mut rest: Vec<&str> = Vec::new();
    let mut pt = String::new();
    let mut cid = String::new();
    for param in query.split('&') {
        if param.is_empty() {
            continue;
        }
        if let Some(val) = param.strip_prefix("_pt=") {
            pt = percent_decode(val);
        } else if let Some(val) = param.strip_prefix("_cid=") {
            cid = percent_decode(val);
        } else {
            rest.push(param);
        }
    }
    let cleaned = if rest.is_empty() {
        base.to_owned()
    } else {
        format!("{}?{}", base, rest.join("&"))
    };
    ProxyParams {
        path: cleaned,
        pt,
        cid,
    }
}
