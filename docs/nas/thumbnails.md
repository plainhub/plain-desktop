# Thumbnails & Cover Rules

This document describes PlainNAS thumbnail generation behavior and the cover-art selection policy for audio/video files.

## Thumbnail engine

Implementation: `src/media/thumb_engine/` (pure Rust, no C libraries, no external processes).

`/fs?w=...&h=...&cc=1` requests flow through the engine in this order:

1. **Header sniff** (`sniff.rs`): ≤256 KiB async read classifies format and dimensions (JPEG / PNG / GIF / WebP / BMP) without decoding.
2. **Passthrough** (`mod.rs`): a browser-renderable image that already fits the requested box and is ≤1 MiB is served **byte-identical** with its natural MIME type — no decode, no re-encode, no cache write. Icons and already-small images cost ~zero CPU.
3. **Hot LRU** (`lru.rs`): in-memory byte cache (default 32 MB / 4096 entries, `[thumbnails] lru_mb`) — repeated grid renders are an `Arc` clone (<0.2 ms measured).
4. **File cache** (`{cache_dir}/thumbs/XX/<sha1>.jpg`): deterministic key over `(source_path, w, h, q, mtime, size)`; file existence == valid cache. No KV store involved.
5. **Single-flight** (`singleflight.rs`): concurrent requests for the same cache key coalesce into one generation; the rest read the just-written cache (measured: 64 identical concurrent requests cost one decode).
6. **Admission control** (`admission.rs`): two-tier — `2×cores` CPU permits plus a decoded-pixel **byte budget** (default 2 GB, `[thumbnails] mem_budget_mb`, one 64 KiB permit per unit). Small jobs are effectively free (sub-unit floor), so hundreds can run concurrently; monster decodes are held back by the budget, not by a blunt global counter. Sources above 300 MP are rejected outright (decompression-bomb guard).
7. **Pipeline** (blocking pool): decode → box pre-scale (`scale.rs`, integer two-pass box filter) → SIMD convolution resample (`fast_image_resize`; Bilinear < 200 px, else Lanczos3) → EXIF orientation applied on the small result → pure-Rust JPEG encode (`jpeg-encoder`, quality from `q`, default 75, 4:2:0 below q90). Alpha is composited onto white.

### JPEG decode strategy (measured, not guessed)

The engine picks from a three-rung ladder, fastest first:

1. **Progressive + target fits the 1/8 grid → in-tree DC-only decode** (`pjpeg.rs`).
   Every non-DC DCT basis function has zero mean over its 8x8 block, so a
   1/8-scale image (one pixel per block) depends **only** on the DC
   coefficients. Progressive JPEGs code DC in dedicated scans (Ss=Se=0)
   carrying ~10–20% of the entropy bytes; `pjpeg` decodes those (plus any
   trailing DC refinement scans) and skips every AC scan byte-wise. The
   sample reconstruction, fir chroma upsampling (H2V1/H2V2) and fixed-point
   YCbCr→RGB mirror `jpeg-decoder` exactly — pinned **byte-identical** by
   fixture tests (`pjpeg.rs`, real vips/libjpeg scan scripts with trailing
   DC refinement). Anything unusual (arithmetic coding, 12-bit, CMYK,
   16-bit quant tables, malformed streams) bails to rung 2.
2. **Big-to-small (either-axis ratio ≥ 2) → `jpeg-decoder` IDCT-scaled
   decode** (1/8–1/2): the full-resolution bitmap never materializes —
   the same shrink-on-load libvips/ffmpeg use. On parse failure it falls
   back to a full decode (zune is lenient on truncated files).
3. **Near full size → `image` crate full decode** (zune-jpeg backend), the
   fastest pure-Rust decoder when almost every pixel is needed.

### Performance contract (smartbox-verified, do not regress)

Measured on the smartbox (Intel Celeron J3160, 4x1.6 GHz — the weakest
supported target), dev build (workspace opt-level 1 + deps opt-level 3),
over HTTPS LAN, cold cache, 2026-09-18:

| Workload | Before | After | Reference |
|---|---|---|---|
| 24 MP progressive (3.4 MB) → 512 px | 5.4 s debug / 1.75 s release | **0.20–0.28 s** | vipsthumbnail 1.05 s, ffmpeg 1.10 s |
| Whole gallery sweep, 31 progressive photos → ~478 px | — | 29/31 at 0.16–0.46 s, worst 0.64 s (small 3–4 MP photos take rung 2: full entropy, ≈ vips parity) | vips ≈ 1.0 s each |
| Warm / 304 | <0.1 s | unchanged | — |

The contract, locked by tests:

- `pjpeg` fixture parity must stay **byte-exact** against `jpeg-decoder`'s
  scaled decode (`parity_*` tests in `pjpeg.rs`) — this pins the DC-only
  math and both fir upsamplers.
- `jpeg_decode_strategy_matrix` pins the routing (progressive-1/8 → DC
  path; baseline or 1/8-too-small → scaled; near-full-size → full decode).
- `bench_thumb_phases` keeps asserting decode remains the dominant phase
  (no copy/encode regressions).
- On-device re-verification: `curl -sk -o /dev/null -w '%{time_total}'`
  a fresh-size `/fs?w=<unused>&h=<unused>` thumbnail; a 24 MP progressive
  photo must stay well under 0.5 s. `thumb_bench <file> 512` gives the
  full-decode/scaled phase medians on-device.

Memory shape (the "zero big copies" rule): the DC path materializes the
file bytes + ~2 bytes/block DC grids (~1.1 MB for 24 MP) + the 1/8 RGB
bitmap (~1.1 MB) + the thumbnail — never a 72 MB full bitmap, never the
96 MB box-filter intermediate the old full-decode path paid. The HTTP
response body wraps the cached `Arc<Vec<u8>>` via `Bytes::from_owner`
(no response-body copy).

### Output format

- Thumbnails are **JPEG** (`image/jpeg`), quality via the `q` query parameter (default 75). This replaced lossy WebP/libwebp: pure-Rust JPEG encodes ~3× faster at thumbnail sizes, has no C dependency, and LAN bandwidth makes the ~30% size difference irrelevant.
- Responses carry `ETag` (derived from the deterministic cache key) and `Cache-Control: private, max-age=300`; `If-None-Match` is answered with `304` **before** any generation work — conditional revalidation costs a stat.
- The format change also invalidated all pre-existing WebP cache entries (cache-key marker `|jpg1|` vs `|webp|`) — a one-time cold regen, no migration needed.

### EXIF orientation

JPEG APP1 orientation is honored (in-tree minimal parser, `exif.rs`): rotated phone photos produce correctly-proportioned upright thumbnails (the target box is computed on upright dimensions, rotation is applied after resize on the small bitmap). Passthrough responses rely on the browser's native EXIF handling (`image-orientation: from-image`).

### Static HEIC/AVIF

Still `204 No Content` — no pure-Rust decoder exists. Animated images / SVG never reach the engine (see passthrough rules below).

## Video frame extraction

Cover-less videos extract a keyframe **entirely in pure Rust** (`thumb_engine/video.rs`): MP4-family demux via the `mp4` crate + H.264 decode via `rusty_h264-decoder` (both C-free; the decoder's optional openh264-asm `accel` feature is deliberately not enabled). The engine is now **process-free end to end**.

Pipeline for mp4/m4v/mov files with an AVC (`avc1`) video track:

1. Demux the moov box, take the first video track, read the real duration from mdhd (fixing the old bug where a hard-coded `0` always grabbed frame 0).
2. Timepoint heuristic (unchanged): duration < 4s → 0s; < 60s → 1s; ≥ 60s → `duration × 0.1`.
3. Walk the sample tables (stts decode timestamps, stss sync samples) to find the keyframe sample at or before the timepoint; read only that sample plus a bounded tail (≤16 samples / ≤4 MiB) — the file is never loaded whole.
4. Convert the avcC length-prefixed NALs to Annex-B (in-tree), re-injecting SPS/PPS from the sample entry, and decode until the first frame comes out.
5. Planar YUV 4:2:0 → RGB (BT.601 limited range, in-tree) → the shared box-fit resize → JPEG encode.

Admission control prices the decode by the real frame size (YUV + RGB bytes) between the demux and decode phases. Cover/video thumbnails now also **read** the LRU/file cache and single-flight (the old path wrote the cache but never consulted it, regenerating on every request).

Reference numbers (Mac, release, committed 320×240 fixtures): demux ≈ 0.1 ms, decode+resize+encode ≈ 1 ms — the retired ffmpeg CLI path paid ~30 ms of process startup per request on top of decode.

**Not decodable in pure Rust** (cover art is still tried first; a cover-less file then answers `204 No Content`, no ffmpeg fallback): mkv/avi/flv/webm containers, and non-AVC tracks (HEVC/hvc1, VP9 …). `tr=1` playback transcoding in `src/media/video.rs` is a separate feature and still uses ffmpeg — only thumbnail extraction went process-free.

## `/fs` passthrough rules (plain-app alignment)

The shared web client talks to `/fs` with the same contract as plain-app's `FileServer.kt`. `/fs` therefore serves the following requests **without** going through the thumbnail pipeline:

- **Animated images and SVG** (mirror of plain-app `isAnimatedImageOrSvg`, implemented in `src/fsx.rs::is_animated_image_or_svg`): `.svg` by extension (or a `<svg` tag in the first 256 bytes), all GIFs, animated WebP (VP8X animation bit) and animated HEIF (`msf1`/`hevc`/`hevx` brands). These are streamed as-is with their native MIME type and full Range support.
- **`probe=1`**: responds `{"codec":"<fourcc>"}` for the first video track; `""` when unparsable. ISO-BMFF files (MP4/MOV) are parsed **in-process** (`src/media/video.rs::probe_iso_bmff_video_fourcc`: walk top-level boxes, buffer only `moov`, take the first `vide` trak's first `stsd` sample entry fourcc) — sub-millisecond, matching plain-app's in-process Android `MediaExtractor` probe. Non-ISO containers (mkv/webm/…) fall back to the ffprobe CLI. The ffprobe spawn measured ~0.5 s per first view on a weak NAS CPU and dominated video start latency, so MP4 probes must never regress to the spawn path (locked by `probe_reads_fourcc_from_real_fixtures` + the synthetic walker tests).
- **`tr=1`** (HEVC MP4 only): transcodes to browser-playable H.264 via ffmpeg, cached at `{cache_dir}/videos/XX/<sha1>.mp4`; failures surface as `415`. Implementation: `src/media/video.rs`.

Byte-range semantics (`serve_file` in `src/api/fs.rs`, tri-state `plain_rs::utils::http::parse_range_header`): satisfiable `Range` → `206` + `Content-Range`; syntactically valid but out of bounds → `416` + `Content-Range: bytes */<size>` (RFC 7233 §4.4, same shape Ktor serves for plain-app); absent/malformed/unknown-unit → `200` full body. Locked by the `api::fs::tests` handler tests.

Streaming hot-path notes (smartbox/J3160 measured, debug build): serving costs ~35 ms CPU per MB over HTTPS vs ~13 ms over plain HTTP — TLS record processing dominates and is the floor for any user-space work; the file→socket user-space path reads directly into the yielded `Bytes` chunk (`plain_rs::utils::async_read_stream`, no intermediate copy — tokio-util `ReaderStream`'s exact pattern). Recent-file tracking runs once per view (no Range header, or a range starting at byte 0) instead of on every ~2 MiB continuation chunk, so chunked playback no longer rewrites the 500-entry recent list per chunk (matters on SD-card storage). Locked by `fs_recent_tracked_once_per_view_not_per_chunk`.

Not ported from plain-app (unreachable or Android-only): `content://` URIs, `pkgicon://`, BLE byte-range mode, zip virtual paths, HEIC→PNG / MP4-remux conversions.

## Cover extraction policy (audio/video)

Priority (implementation: `src/media/cover.rs`, pure Rust via `lofty`):

1. **Sidecar image** (same directory): `<stem>.{jpg,jpeg,webp,png,gif}`, then `cover.*`, then `folder.*`
2. **Embedded cover**: MP3 `APIC`, FLAC `PICTURE`, MP4/M4A `covr`, OGG/Opus Vorbis comments

The thumbnail cache key uses the **media file's own** `mtime/size` (not the sidecar's), matching previous behavior.

## No-cover behavior

If generation fails (no cover, unsupported type/container/codec), `/fs?cc=1` returns `204 No Content`. There is no negative cache; the next request retries.

## Background prefetcher

`thumb_engine/prefetch.rs` warms the cache for the files the UI grids request before the user opens the page, turning first-open cold generation (~180 ms per 12 MP photo) into a cache hit:

- **Sources**: `mediaBuckets` topItems (4 per directory, all three media types — exactly what the bucket grid shows) and `db::recent`. Jobs are capped at 2000 per round.
- **Parameters match the frontend exactly** (verified against the shared web client): bucket grids request `w=128&h=128` (`BucketThumb.vue`), the recent list `w=50&h=50` (`lib/file.ts`), both at the default quality 75. Since the cache key covers path/w/h/q/mtime/size, a mismatch in any of these would make the prefetch worthless.
- **Triggers**: startup, scan completion (`media_scan` → `prefetch::on_scan_complete`), and a 10-minute idle ticker; each trigger re-collects and replaces the pending queue.
- **Rate limit**: one single-threaded worker, default 2 thumbnails/s (`[thumbnails] prefetch_per_sec`, clamped 1..=20), paced per job so cheap cache-hit jobs still respect the rate. Consumption pauses while a media scan is running. Failed paths are dropped for the round (try-once; the next round re-enqueues).
- All generation goes through the engine's public `get_thumbnail` — admission control, cache-skip and single-flight apply unchanged; the worker adds no parallelism beyond its own task.

```toml
[thumbnails]
prefetch = true          # default; false = no worker, zero background activity
prefetch_per_sec = 2     # clamped 1..=20
```

## Configuration

```toml
[thumbnails]
mem_budget_mb = 2048   # decoded-pixel admission budget (clamped 128..16384)
lru_mb = 32             # hot in-memory thumbnail cache (clamped 1..2048)
prefetch = true         # background prefetcher (see above)
prefetch_per_sec = 2    # prefetch rate, clamped 1..=20
```

## Benchmarking

Real photographs live in `tmp-bench-media/` (gitignored; Wikimedia Commons camera JPEGs/PNG):

```bash
cargo test --release bench_thumb -- --ignored --nocapture --test-threads=1
```

- `bench_thumb_phases`: per-phase medians (the engine's real decode routing vs full decode, resize, encode) over the real photos × {256, 512, 1024}; asserts decode remains the dominant phase.
- `bench_thumb_engine_e2e`: cold vs warm engine latency per photo/target.
- `bench_thumb_concurrent`: 64-request coalescing, 8/32 **distinct** 12 MP files (true parallel decode throughput, page-cache-warmed), and a mixed load (big photos + passthrough icons).

Reference numbers (Mac, release, 2026-09): 12 MP → 512 px cold ≈ 180 ms (decode-bound), warm 0.1 ms; 32 distinct 12 MP files ≈ 115 thumbs/s; 64 identical requests = one generation (194 ms); mixed 43-request load ≈ 207 req/s.
