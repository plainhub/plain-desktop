//! High-performance pure-Rust thumbnail engine.
//!
//! Pipeline per request (image files):
//!
//! 1. sniff the header (async read of ≤256 KiB) → format + dimensions,
//! 2. small-image passthrough: an image that already fits the target box
//!    (and is ≤1 MiB) is served byte-identical — zero decode, zero encode,
//! 3. hot LRU, then the deterministic file cache
//!    (`{cache_dir}/thumbs/XX/<sha1>.jpg`),
//! 4. single-flight per cache key — concurrent duplicate requests coalesce,
//! 5. two-tier admission (CPU permits + decoded-pixel byte budget),
//! 6. decode (IDCT-scaled JPEG for big→small, `image` crate otherwise)
//!    → box pre-scale → SIMD resample → EXIF orientation → JPEG encode,
//! 7. atomic cache write + LRU insert.
//!
//! Non-image files go through the cover-art path (sidecar/lofty); cover-less
//! MP4-family videos extract a keyframe via pure-Rust demux + H.264 decode
//! (`video.rs`). Other containers/codec combinations have no pure-Rust
//! decoder and answer `204 No Content` (documented in docs/thumbnails.md).

pub mod admission;
#[cfg(test)]
#[path = "../../../tests/unit/media/thumb/benches.rs"]
pub mod benches;
pub mod decode;
pub mod encode;
pub mod exif;
pub mod lru;
pub mod pjpeg;
pub mod prefetch;
pub mod scale;
pub mod singleflight;
pub mod sniff;
pub mod video;

use anyhow::{Result, bail};
use std::sync::{Arc, LazyLock};
use tokio::io::AsyncReadExt;

use exif::Orientation;
use scale::Bitmap;
use sniff::{ImageKind, Sniffed};

/// Largest thumbnail box side accepted from query params.
pub const MAX_TARGET_DIM: u32 = 16_384;

/// Files at or below this size that already fit the target box are served
/// as-is (passthrough) instead of being re-encoded.
pub const PASSTHROUGH_MAX_BYTES: u64 = 1024 * 1024;

/// How much of a file we read to sniff + parse EXIF orientation.
const HEAD_READ: usize = 256 * 1024;

/// Big-to-small JPEG requests (downscale ratio ≥ 2 on either axis) decode
/// via `jpeg-decoder` with IDCT scaling, so the full-resolution bitmap
/// never materializes — the same shrink-on-load libvips/ffmpeg use. Near
/// full-size requests go through the `image` crate (zune-jpeg), whose
/// full decode is faster when almost every decoded pixel is needed.
fn jpeg_use_scaled(w: u32, h: u32, tw: u32, th: u32) -> bool {
    (tw > 0 && w >= tw * 2) || (th > 0 && h >= th * 2)
}

/// Progressive JPEGs whose 1/8 DC grid covers the target box take the
/// DC-only fast path (`pjpeg`): every non-DC DCT basis function has zero
/// mean over its block, so the 1/8 image depends only on the DC scans —
/// the AC scans (~80-90% of the entropy bytes) are skipped byte-wise.
fn dc_fast_eligible(s: Sniffed, tw: u32, th: u32) -> bool {
    s.progressive && tw > 0 && th > 0 && tw <= s.width.div_ceil(8) && th <= s.height.div_ceil(8)
}

/// Outcome of a thumbnail request.
#[derive(Debug)]
pub enum ThumbOutcome {
    /// Generated (or cache-hit) JPEG bytes, `image/jpeg`.
    Generated(Arc<Vec<u8>>),
    /// Source bytes served as-is with their natural MIME type.
    Original { data: Arc<Vec<u8>>, mime: String },
}

/// Everything the engine needs about one request. `mod_unix`/`file_size`
/// come from the stat the HTTP layer already performed.
#[derive(Debug, Clone)]
pub struct ThumbSpec {
    pub path: String,
    pub w: u32,
    pub h: u32,
    pub quality: u8,
    pub mod_unix: i64,
    pub file_size: i64,
}

impl ThumbSpec {
    /// Clamp hostile query params into a sane request.
    pub fn sanitize(path: &str, w: i32, h: i32, quality: i32, mod_unix: i64, size: i64) -> Self {
        ThumbSpec {
            path: path.to_string(),
            w: w.clamp(0, MAX_TARGET_DIM as i32) as u32,
            h: h.clamp(0, MAX_TARGET_DIM as i32) as u32,
            quality: quality.clamp(1, 100) as u8,
            mod_unix,
            file_size: size,
        }
    }
}

static LOCKS: LazyLock<singleflight::KeyedLocks> = LazyLock::new(singleflight::KeyedLocks::new);

/// Test hook: cold generations keyed by cache path (incremented once per
/// actual decode+encode, not per request). Keyed so parallel tests cannot
/// pollute each other's coalescing assertions.
#[cfg(test)]
pub(crate) static GENERATIONS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, u64>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

#[cfg(test)]
pub(crate) fn generations_for(key: &str) -> u64 {
    *GENERATIONS.lock().unwrap().get(key).unwrap_or(&0)
}

#[cfg(test)]
fn bump_generations(key: &str) {
    *GENERATIONS
        .lock()
        .unwrap()
        .entry(key.to_string())
        .or_insert(0) += 1;
}

/// Entry point used by `/fs`.
pub async fn get_thumbnail(spec: ThumbSpec) -> Result<ThumbOutcome> {
    let head = read_head(&spec.path).await;
    let sniffed = sniff::sniff_header(&head)
        .ok()
        .filter(|s| s.kind.decodable());

    match sniffed {
        Some(s) => get_image_thumbnail(&spec, &head, s).await,
        None => dispatch_non_image(&spec).await,
    }
}

async fn get_image_thumbnail(spec: &ThumbSpec, head: &[u8], s: Sniffed) -> Result<ThumbOutcome> {
    // EXIF orientation first: the *upright* dimensions decide the target box
    // so rotated photos produce correctly-proportioned thumbnails.
    let orientation = if s.kind == ImageKind::Jpeg {
        exif::orientation(head).unwrap_or(1)
    } else {
        1
    };
    let (uw, uh) = if Orientation(orientation).swaps_axes() {
        (s.height, s.width)
    } else {
        (s.width, s.height)
    };
    let (tw, th) = compute_target_size(uw, uh, spec.w, spec.h);

    // Passthrough: already thumbnail-sized, browser-renderable, small.
    if tw == uw
        && th == uh
        && u64::try_from(spec.file_size).unwrap_or(u64::MAX) <= PASSTHROUGH_MAX_BYTES
    {
        let data = tokio::fs::read(&spec.path).await?;
        return Ok(ThumbOutcome::Original {
            data: Arc::new(data),
            mime: s.kind.mime().to_string(),
        });
    }

    let cache_path = thumb_cache_path(
        &cache_dir(),
        &spec.path,
        spec.w,
        spec.h,
        spec.quality,
        spec.mod_unix,
        spec.file_size,
    );
    let key = cache_path.to_string_lossy().into_owned();

    // Hot LRU, then file cache.
    if let Some(d) = lru::global().get(&cache_path) {
        return Ok(ThumbOutcome::Generated(d));
    }
    if let Some(d) = read_cache_file(&cache_path).await {
        lru::global().put(cache_path.clone(), d.clone());
        return Ok(ThumbOutcome::Generated(d));
    }

    // Single-flight: identical in-flight requests wait for one generation.
    LOCKS
        .with_lock(key, async {
            if let Some(d) = lru::global().get(&cache_path) {
                return Ok(ThumbOutcome::Generated(d));
            }
            if let Some(d) = read_cache_file(&cache_path).await {
                lru::global().put(cache_path.clone(), d.clone());
                return Ok(ThumbOutcome::Generated(d));
            }
            generate_and_cache(spec, s, orientation, tw, th, cache_path).await
        })
        .await
}

async fn generate_and_cache(
    spec: &ThumbSpec,
    s: Sniffed,
    orientation: u8,
    tw: u32,
    th: u32,
    cache_path: std::path::PathBuf,
) -> Result<ThumbOutcome> {
    let swaps = Orientation(orientation).swaps_axes();
    // The stored bitmap must be resized to the swapped target so the
    // post-resize rotation lands exactly on (tw, th).
    let (rw, rh) = if swaps { (th, tw) } else { (tw, th) };

    // Decompression-bomb guard for full decodes; scaled JPEG decodes only
    // materialize their scaled output, so they are priced by that instead.
    let scaled_jpeg = s.kind == ImageKind::Jpeg
        && (jpeg_use_scaled(s.width, s.height, rw, rh) || dc_fast_eligible(s, rw, rh));
    if !scaled_jpeg && u64::from(s.width) * u64::from(s.height) > admission::MAX_SOURCE_PIXELS {
        bail!("source {}x{} exceeds pixel guard", s.width, s.height);
    }
    let est = estimated_decoded_bytes(s, rw, rh);
    let _permits = admission::global().acquire(est).await?;

    let path = spec.path.clone();
    let quality = spec.quality;
    let cp = cache_path.clone();
    let jpg = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        let bm = decode_to_target(&path, s, rw, rh)?;
        let bm = scale::resize_to(bm, rw, rh).apply_orientation(Orientation(orientation));
        let jpg = encode::encode_jpeg(bm, quality)?;
        let _ = write_thumb_cache(&cp, &jpg);
        Ok(jpg)
    })
    .await??;

    #[cfg(test)]
    {
        let key = cache_path.to_string_lossy().into_owned();
        bump_generations(&key);
    }
    let data = Arc::new(jpg);
    lru::global().put(cache_path, data.clone());
    Ok(ThumbOutcome::Generated(data))
}

/// Sync decode for files. JPEG ladder (fastest first):
/// 1. progressive + 1/8 covers the target → in-tree DC-only decode
///    (`pjpeg`, AC scans skipped; ~5-8× less entropy work than libvips),
/// 2. big-to-small JPEG → `jpeg-decoder` IDCT-scaled decode,
/// 3. otherwise (or on a bail above) → `image` crate full decode.
fn decode_to_target(path: &str, s: Sniffed, rw: u32, rh: u32) -> Result<Bitmap> {
    let p = std::path::Path::new(path);
    if s.kind == ImageKind::Jpeg {
        // Unsupported structure/strictness bails fall through to the
        // generic scaled decoder below.
        if dc_fast_eligible(s, rw, rh)
            && let Ok(bm) = pjpeg::decode_dc_only(p)
        {
            return Ok(bm);
        }
        if jpeg_use_scaled(s.width, s.height, rw, rh) {
            return decode::jpeg_scaled(p, rw, rh).or_else(|_| decode::image_full(p));
        }
    }
    decode::image_full(p)
}

#[allow(dead_code)] // parity with plain-nas: kept for parity tests
fn downscale_ratio(w: u32, h: u32, tw: u32, th: u32) -> f64 {
    let rw = if tw > 0 {
        f64::from(w) / f64::from(tw)
    } else {
        1.0
    };
    let rh = if th > 0 {
        f64::from(h) / f64::from(th)
    } else {
        1.0
    };
    rw.max(rh)
}

/// Estimate the decoded bitmap bytes the request will materialize — this is
/// what admission control prices. The DC-only and scaled JPEG paths are
/// priced by their reduced outputs; everything else by the full bitmap.
fn estimated_decoded_bytes(s: Sniffed, rw: u32, rh: u32) -> u64 {
    let px: u64 = if s.kind == ImageKind::Jpeg && dc_fast_eligible(s, rw, rh) {
        u64::from(s.width.div_ceil(8)) * u64::from(s.height.div_ceil(8))
    } else if s.kind == ImageKind::Jpeg && jpeg_use_scaled(s.width, s.height, rw, rh) {
        let (sw, sh) = decode::predict_scaled(s.width, s.height, rw, rh);
        u64::from(sw) * u64::from(sh)
    } else {
        u64::from(s.width) * u64::from(s.height)
    };
    px.saturating_mul(4)
}

// ---------------------------------------------------------------------------
// Non-image dispatch: cover art, then pure-Rust video frame extraction
// ---------------------------------------------------------------------------

async fn dispatch_non_image(spec: &ThumbSpec) -> Result<ThumbOutcome> {
    // Covers and video frames cache under the same deterministic key scheme
    // as images — the old path wrote this cache but never read it, so every
    // request regenerated (a fresh ffmpeg spawn / cover decode per GET).
    let cache_path = thumb_cache_path(
        &cache_dir(),
        &spec.path,
        spec.w,
        spec.h,
        spec.quality,
        spec.mod_unix,
        spec.file_size,
    );
    let key = cache_path.to_string_lossy().into_owned();

    if let Some(d) = lru::global().get(&cache_path) {
        return Ok(ThumbOutcome::Generated(d));
    }
    if let Some(d) = read_cache_file(&cache_path).await {
        lru::global().put(cache_path.clone(), d.clone());
        return Ok(ThumbOutcome::Generated(d));
    }

    LOCKS
        .with_lock(key, async {
            if let Some(d) = lru::global().get(&cache_path) {
                return Ok(ThumbOutcome::Generated(d));
            }
            if let Some(d) = read_cache_file(&cache_path).await {
                lru::global().put(cache_path.clone(), d.clone());
                return Ok(ThumbOutcome::Generated(d));
            }
            generate_non_image(spec, &cache_path).await
        })
        .await
}

async fn generate_non_image(
    spec: &ThumbSpec,
    cache_path: &std::path::Path,
) -> Result<ThumbOutcome> {
    let path = spec.path.clone();
    let (w, h, quality) = (spec.w, spec.h, spec.quality);

    // 1. Cover art (sidecar file > embedded via lofty) — cheap, CPU permit only.
    {
        let _permits = admission::global().acquire(0).await?;
        let p = path.clone();
        let cover =
            tokio::task::spawn_blocking(move || crate::media::cover::extract_cover(&p)).await?;
        if let Some(cover) = cover {
            let jpg = thumb_from_cover_bytes(&cover.bytes, w, h, quality)?;
            return finish_generated(cache_path, jpg);
        }
    }

    // 2. Cover-less video: pure-Rust MP4 demux + H.264 keyframe decode, split
    //    in two blocking phases so admission prices the decode (YUV+RGB) by
    //    the real frame size between them.
    if is_video_ext(&path) {
        let p = path.clone();
        let plan = tokio::task::spawn_blocking(move || video::plan_keyframe(&p)).await??;
        let _permits = admission::global().acquire(plan.estimated_bytes()).await?;
        let jpg =
            tokio::task::spawn_blocking(move || video::decode_plan_to_jpeg(plan, w, h, quality))
                .await??;
        return finish_generated(cache_path, jpg);
    }

    bail!("unsupported file type for thumbnail")
}

/// Cache + return a generated non-image thumbnail (cover art / video frame).
fn finish_generated(cache_path: &std::path::Path, jpg: Vec<u8>) -> Result<ThumbOutcome> {
    let data = Arc::new(jpg);
    let _ = write_thumb_cache(cache_path, &data);
    lru::global().put(cache_path.to_path_buf(), data.clone());
    #[cfg(test)]
    {
        let key = cache_path.to_string_lossy().into_owned();
        bump_generations(&key);
    }
    Ok(ThumbOutcome::Generated(data))
}

fn thumb_from_cover_bytes(bytes: &[u8], w: u32, h: u32, quality: u8) -> Result<Vec<u8>> {
    let s = sniff::sniff_header(bytes)?;
    if u64::from(s.width) * u64::from(s.height) > admission::MAX_SOURCE_PIXELS {
        bail!("cover {}x{} exceeds pixel guard", s.width, s.height);
    }
    let (tw, th) = compute_target_size(s.width, s.height, w, h);
    let bm = decode::image_from_bytes(bytes)?;
    let bm = scale::resize_to(bm, tw, th);
    encode::encode_jpeg(bm, quality)
}

fn is_video_ext(path: &str) -> bool {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    matches!(
        ext.as_str(),
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "flv" | "m4v"
    )
}

// ---------------------------------------------------------------------------
// Cache plumbing (deterministic path, atomic write, ETag)
// ---------------------------------------------------------------------------

fn cache_dir() -> std::path::PathBuf {
    crate::media::paths::detect().cache_dir
}

/// Compute the deterministic cache file path for a thumbnail.
///
/// Layout: `{cache_dir}/thumbs/XX/<sha1>.jpg` where XX = first 2 hex chars
/// (256 shard directories). The hash covers source_path|w|h|q|mtime|size so
/// any source change produces a new path: file existence == valid cache.
pub fn thumb_cache_path(
    cache_dir: &std::path::Path,
    source_path: &str,
    w: u32,
    h: u32,
    quality: u8,
    mod_unix: i64,
    file_size: i64,
) -> std::path::PathBuf {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(source_path.as_bytes());
    hasher.update(b"|jpg1|");
    hasher.update(format!("{w}x{h}|q{quality}|m{mod_unix}|s{file_size}").as_bytes());
    let hex = crate::utils::hex::bytes_to_hex(&hasher.finalize());
    cache_dir
        .join("thumbs")
        .join(&hex[..2])
        .join(format!("{hex}.jpg"))
}

/// Strong ETag for a thumbnail response. Derives from the cache filename,
/// which already hashes every invalidation input — computable before any
/// generation work, so conditional requests cost a stat only.
pub fn cache_etag(cache_path: &std::path::Path) -> String {
    let hex = cache_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .strip_suffix(".jpg")
        .unwrap_or_default();
    format!("\"t{hex}\"")
}

/// Write thumbnail bytes to the cache file atomically (temp + rename).
pub fn write_thumb_cache(cache_path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let dir = cache_path.parent().unwrap_or(std::path::Path::new("."));
    let mut tmp = tempfile::Builder::new().suffix(".tmp").tempfile_in(dir)?;
    tmp.write_all(data)?;
    tmp.persist(cache_path)?;
    Ok(())
}

async fn read_cache_file(cache_path: &std::path::Path) -> Option<Arc<Vec<u8>>> {
    match tokio::fs::read(cache_path).await {
        Ok(d) if !d.is_empty() => Some(Arc::new(d)),
        _ => None,
    }
}

async fn read_head(path: &str) -> Vec<u8> {
    match tokio::fs::File::open(path).await {
        Ok(mut f) => {
            let mut buf = vec![0u8; HEAD_READ];
            match f.read(&mut buf).await {
                Ok(n) => {
                    buf.truncate(n);
                    buf
                }
                Err(_) => Vec::new(),
            }
        }
        Err(_) => Vec::new(),
    }
}

/// Compute target dimensions preserving aspect ratio (no upscaling), based
/// on the *upright* source dimensions.
pub fn compute_target_size(src_w: u32, src_h: u32, w: u32, h: u32) -> (u32, u32) {
    let mut tw = src_w;
    let mut th = src_h;
    if w > 0 && h > 0 {
        let rw = f64::from(w) / f64::from(src_w);
        let rh = f64::from(h) / f64::from(src_h);
        let r = rw.min(rh);
        if r < 1.0 {
            tw = (f64::from(src_w) * r) as u32;
            th = (f64::from(src_h) * r) as u32;
        }
    } else if w > 0 && w < src_w {
        let r = f64::from(w) / f64::from(src_w);
        tw = w;
        th = (f64::from(src_h) * r) as u32;
    } else if h > 0 && h < src_h {
        let r = f64::from(h) / f64::from(src_h);
        th = h;
        tw = (f64::from(src_w) * r) as u32;
    }
    (tw.max(1), th.max(1))
}

/// Configure engine globals (`[thumbnails]` section). Idempotent; the first
/// call wins (startup before first request).
pub fn init_from_config(cfg: &crate::media::config::Config) {
    admission::init_from_config(cfg);
    lru::init_from_config(cfg);
}

#[cfg(test)]
#[path = "../../../tests/unit/media/thumb/mod.rs"]
mod tests;
