//! Video codec probing and browser-playability transcoding.
//!
//! Mirrors plain-app `probeVideoCodec` / `transcodeMp4ForBrowser`: the shared
//! web client probes `/fs?probe=1` for the first video track's codec fourcc
//! and switches to `/fs?tr=1` on browsers without an HEVC decoder. plain-app
//! runs its probe in-process (Android `MediaExtractor`); the equivalent here
//! is an in-tree ISO-BMFF walker — spawning `ffprobe` cost ~0.5s of CPU on a
//! weak NAS box per first view, which dominated video start latency. The
//! `ffmpeg` CLI remains the fallback for non-ISO containers (Matroska, WebM)
//! and the transcode path.

use anyhow::Result;

/// Upper bound on the `moov` box we are willing to buffer for the in-process
/// probe. Real-world moov boxes are at most a few MB even for 4K movies;
/// anything larger is pathological — fall back to ffprobe instead.
const MAX_MOOV_BYTES: u64 = 64 * 1024 * 1024;

/// A parsed ISO-BMFF box header.
struct BoxHeader {
    kind: [u8; 4],
    /// Bytes the header itself consumed at the cursor (8, or 16 with the
    /// 64-bit largesize form).
    header_len: u64,
    /// Full declared box size including the header. The `size == 0` form
    /// ("box extends to end of file/parent") is normalized to `remaining`.
    box_len: u64,
}

impl BoxHeader {
    /// Payload bytes actually available: the declared size clamped to what
    /// is left, so a lying size never reads out of bounds.
    fn payload_len(&self, remaining: u64) -> u64 {
        self.box_len
            .saturating_sub(self.header_len)
            .min(remaining.saturating_sub(self.header_len))
    }
}

/// Read one box header at the cursor. `None` at EOF or on a truncated or
/// self-inconsistent header (smaller than itself, garbage).
fn read_box_header(
    r: &mut impl std::io::Read,
    remaining: u64,
) -> std::io::Result<Option<BoxHeader>> {
    if remaining < 8 {
        return Ok(None);
    }
    let mut hdr = [0u8; 8];
    if r.read_exact(&mut hdr).is_err() {
        return Ok(None);
    }
    let mut size = u32::from_be_bytes(hdr[0..4].try_into().unwrap()) as u64;
    let kind: [u8; 4] = hdr[4..8].try_into().unwrap();
    let mut header_len = 8u64;
    if size == 1 {
        // 64-bit largesize form.
        if remaining < 16 {
            return Ok(None);
        }
        let mut ext = [0u8; 8];
        if r.read_exact(&mut ext).is_err() {
            return Ok(None);
        }
        size = u64::from_be_bytes(ext);
        header_len = 16;
    }
    if size != 0 && size < header_len {
        return Ok(None); // garbage: box smaller than its own header
    }
    Ok(Some(BoxHeader {
        kind,
        header_len,
        box_len: if size == 0 { remaining } else { size },
    }))
}

fn is_kind(kind: &[u8; 4], name: &str) -> bool {
    kind == name.as_bytes()
}

/// Walk `parent`'s child boxes in order, handing each `(kind, payload)` to
/// `f`; the walk stops early once `f` returns `true`. On any structural
/// oddity the walk just stops (callers treat that as "nothing more found").
fn walk_child_boxes(parent: &[u8], mut f: impl FnMut(&[u8; 4], &[u8]) -> bool) {
    use std::io::Seek;
    let mut cursor = std::io::Cursor::new(parent);
    let mut remaining = parent.len() as u64;
    while let Ok(Some(hdr)) = read_box_header(&mut cursor, remaining) {
        let payload_len = hdr.payload_len(remaining);
        let start = cursor.position() as usize;
        let end = start.saturating_add(payload_len as usize).min(parent.len());
        if f(&hdr.kind, &parent[start..end]) {
            return;
        }
        if cursor
            .seek(std::io::SeekFrom::Current(payload_len as i64))
            .is_err()
        {
            return;
        }
        remaining = parent.len() as u64 - cursor.position();
    }
}

/// Codec fourcc of the first video track of an ISO-BMFF file (MP4/MOV),
/// read entirely in-process: walk top-level boxes, buffer only the `moov`
/// box, then `moov → trak → mdia → hdlr(vide) → minf → stbl → stsd` and
/// take the first sample entry's fourcc (`avc1`/`hvc1`/`hev1`/`vp09`/…).
/// Reads are header-sized except the moov buffer itself, so a moov-at-end
/// file costs one seek, not a scan.
fn probe_iso_bmff_video_fourcc(path: &std::path::Path) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};

    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    if len < 8 {
        return None;
    }

    // Walk top-level boxes; skip everything that is not moov.
    let mut moov: Vec<u8> = Vec::new();
    let mut pos: u64 = 0;
    while pos < len {
        if f.seek(SeekFrom::Start(pos)).is_err() {
            return None;
        }
        let remaining = len - pos;
        let Ok(Some(hdr)) = read_box_header(&mut f, remaining) else {
            return None;
        };
        let payload_len = hdr.payload_len(remaining);
        if is_kind(&hdr.kind, "moov") {
            if payload_len == 0 || payload_len > MAX_MOOV_BYTES {
                return None;
            }
            moov = vec![0u8; payload_len as usize];
            if f.read_exact(&mut moov).is_err() {
                return None;
            }
            break;
        }
        pos = pos.saturating_add(hdr.box_len);
    }
    if moov.is_empty() {
        return None;
    }

    // First trak whose handler is `vide` wins; its first stsd sample entry
    // fourcc is the codec tag (same pick as ffprobe's `-select_streams v:0`).
    let mut fourcc: Option<String> = None;
    walk_child_boxes(&moov, |kind, trak| {
        if fourcc.is_some() {
            return true; // found — stop the whole walk
        }
        if !is_kind(kind, "trak") {
            return false;
        }
        walk_child_boxes(trak, |kind2, mdia| {
            if !is_kind(kind2, "mdia") {
                return false;
            }
            let mut is_video = false;
            let mut entry: Option<[u8; 4]> = None;
            walk_child_boxes(mdia, |kind3, b| {
                if is_kind(kind3, "hdlr") && b.len() >= 12 {
                    // hdlr payload: version+flags (4) + pre_defined (4) + handler_type (4)
                    is_video = is_kind(b[8..12].try_into().unwrap(), "vide");
                } else if is_kind(kind3, "minf") {
                    walk_child_boxes(b, |kind4, stbl| {
                        if is_kind(kind4, "stbl") {
                            walk_child_boxes(stbl, |kind5, stsd| {
                                // stsd payload: version+flags (4) + entry_count (4)
                                // + entries, each starting with size (4) + fourcc (4).
                                if is_kind(kind5, "stsd") && stsd.len() >= 16 {
                                    entry = Some(stsd[12..16].try_into().unwrap());
                                }
                                false
                            });
                        }
                        false
                    });
                }
                false
            });
            if is_video && let Some(e) = entry {
                fourcc = Some(String::from_utf8_lossy(&e).to_string());
            }
            is_video // a video trak settles the answer either way
        });
        fourcc.is_some()
    });
    fourcc
}

/// fourcc of the first video track's codec tag (e.g. "hvc1", "avc1"), or ""
/// when the file has no parsable video track. Non alphanumeric characters
/// (e.g. `[0][0][0][0]` for untagged streams) are stripped, mirroring
/// plain-app. ISO-BMFF files are parsed in-process (sub-millisecond);
/// everything else falls back to the `ffprobe` CLI.
pub async fn probe_video_codec(path: &str) -> String {
    probe_with_fallback(path, |p: String| async move { ffprobe_codec_tag(&p).await }).await
}

/// `fallback` is only consulted when the in-process walker cannot answer
/// (non-ISO container / unparsable file). It is injected so tests can prove
/// deterministically that ISO-BMFF probes never pay the ~0.5 s process
/// spawn on weak CPUs — the whole point of the walker.
async fn probe_with_fallback<F, Fut>(path: &str, fallback: F) -> String
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = String>,
{
    let owned = path.to_string();
    let in_process = tokio::task::spawn_blocking(move || {
        probe_iso_bmff_video_fourcc(std::path::Path::new(&owned))
    })
    .await;
    if let Ok(Some(fourcc)) = in_process {
        return filter_codec_tag(&fourcc);
    }
    fallback(path.to_string()).await
}

async fn ffprobe_codec_tag(path: &str) -> String {
    let out = tokio::process::Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_tag_string",
            "-of",
            "csv=p=0",
            path,
        ])
        .output()
        .await;
    match out {
        Ok(out) if out.status.success() => {
            let tag = String::from_utf8_lossy(&out.stdout).trim().to_string();
            filter_codec_tag(&tag)
        }
        _ => String::new(),
    }
}

/// Keep only alphanumeric characters, mirroring plain-app
/// `probeVideoCodec(...).filter { it.isLetterOrDigit() }`.
fn filter_codec_tag(tag: &str) -> String {
    tag.chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Deterministic cache path for the browser-playable transcode of `path`:
/// `{cache_dir}/videos/XX/<sha1>.mp4` where the hash covers
/// `source_path|mtime|size`, so file existence == valid cache.
pub fn transcode_cache_path(
    cache_dir: &std::path::Path,
    source_path: &str,
    mod_unix: i64,
    file_size: i64,
) -> std::path::PathBuf {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(source_path.as_bytes());
    hasher.update(b"|browser-mp4|");
    hasher.update(format!("m{mod_unix}|s{file_size}").as_bytes());
    let hex = crate::utils::hex::bytes_to_hex(&hasher.finalize());
    cache_dir
        .join("videos")
        .join(&hex[..2])
        .join(format!("{hex}.mp4"))
}

/// Produce an H.264-transcoded variant of the HEVC video at `path` for
/// browsers without an HEVC decoder (audio stream-copied, cached on disk).
/// Returns the cached file path. Only run when the client explicitly opted
/// in (`tr=1`) — transcoding is far more expensive than a remux.
pub async fn transcode_mp4_for_browser(path: &str) -> Result<std::path::PathBuf> {
    let meta = std::fs::metadata(path)?;
    let mod_unix = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let size = meta.len() as i64;

    let cache_dir = crate::media::paths::detect().cache_dir;
    let cache_path = transcode_cache_path(&cache_dir, path, mod_unix, size);
    if cache_path.is_file() {
        return Ok(cache_path);
    }

    let src = path.to_string();
    // Render into a sibling temp file first so a killed/failed run never
    // leaves a half-written entry at the deterministic cache path.
    tokio::task::spawn_blocking(move || -> Result<std::path::PathBuf> {
        let parent = cache_path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();
        std::fs::create_dir_all(&parent)?;
        let tmp = tempfile::Builder::new()
            .suffix(".mp4.tmp")
            .tempfile_in(&parent)?;
        let tmp_path = tmp.path().to_path_buf();
        let output = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-i",
                &src,
                "-map",
                "0:v:0",
                "-map",
                "0:a?",
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "23",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "copy",
                "-movflags",
                "+faststart",
                // The temp file's `.mp4.tmp` suffix tells ffmpeg nothing about
                // the container — pin it explicitly.
                "-f",
                "mp4",
            ])
            .arg(&tmp_path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() {
            // ffmpeg's diagnosis lives in the last stderr lines — keep a tail
            // so a failed transcode is diagnosable from logs alone.
            let tail: String = stderr
                .lines()
                .rev()
                .take(8)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join(" | ");
            anyhow::bail!("ffmpeg transcode failed: {tail}");
        }
        // Keep the permissions; persist() replaces an existing file if a
        // concurrent request won the race.
        tmp.persist(&cache_path)?;
        Ok(cache_path)
    })
    .await?
}

#[cfg(test)]
#[path = "../../tests/unit/media/video.rs"]
mod tests;
