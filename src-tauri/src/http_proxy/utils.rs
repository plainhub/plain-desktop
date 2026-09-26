use plain_rs::query::percent_decode;

// ─── SHA-1 (RFC 3174) — only for the synthetic WebSocket handshake ────────────

/// SHA-1 is obsolete for security use; it survives here solely because the
/// RFC 6455 handshake hash requires it. No crate is pulled in for ~55 lines.
pub(crate) fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476, 0xC3D2_E1F0];
    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for block in msg.chunks(64) {
        let mut w = [0u32; 80];
        for (i, word) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for (i, v) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

/// RFC 6455 §1.3: `Sec-WebSocket-Accept = base64(SHA-1(key + GUID))`.
pub(crate) fn ws_accept_key(client_key: &str) -> String {
    let mut input = client_key.as_bytes().to_vec();
    input.extend_from_slice(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    plain_rs::utils::base64::base64_encode(&sha1(&input))
}

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
