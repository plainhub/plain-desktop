# Storage & Indexing

This document describes PlainNAS data storage layout and the search indexing design.

## Disk Manager

Disk enumeration and whole-disk formatting behavior is documented in [docs/disk-manager.md](docs/disk-manager.md).

## Data directory

All paths below are under the data directory (see `src/consts.rs` for defaults).

## Database (fjall)

- Engine: [fjall](https://crates.io/crates/fjall) (embedded LSM KV, since 2026-09)
- Path: `DATA_DIR/fjall`
- Open/init logic: see `src/db/mod.rs`
- Usage: sessions and tokens, recents, tags, and media metadata mappings (UUID/path/FID lookup). Relevant files:
	- Sessions: `src/db/session.rs`
	- Tags: `src/db/tags.rs`
	- Recents: `src/db/recent.rs`
	- URL Token: `src/db/url_token.rs`
	- Media mappings: `src/media_scan.rs`

## Cache (fjall)

- Thumbnails: generated on demand and cached in the KV store using a content-derived key (prefix `thumb:`) composed from path, size, and file metadata.
	- Read/write: `src/api/media_thumb.rs`
	- Generation and cache key helper: `src/media/thumbnail.rs`
	- Thumbnails are not stored as separate files on disk.

## File search index (tantivy)

- Path/name index: `DATA_DIR/searchidx_files/` (tantivy index).
- Engine: [tantivy](https://crates.io/crates/tantivy) — Rust full-text search (Lucene-compatible).
- Schema: `path`, `name`, `ext`, `size`, `modified`, `is_dir`.
- Query semantics: plain text queries search basenames via tantivy's QueryParser. Supports DSL filters for ext, size range, dir, path prefix.
- Implementation: `src/search_index.rs`
- Principles: KV store is source of truth; index is discardable/rebuildable.

## Media search index (tantivy)

- Path: `DATA_DIR/searchidx_media/` (tantivy index).
- Schema: `uuid`, `name`, `path`, `media_type`, `size`, `modified`, `duration`, `artist`, `title`, `is_trash`.
- Implementation: `src/media/search_index.rs`
