# Media Items

This document explains how PlainNAS **media items** are stored, how their IDs (UUIDs) are generated, how indexing works, and which business logic / GraphQL entry points are related to media items.

Terminology:

- **media item**: a record represented by the Rust struct `MediaFile` in `src/media_scan.rs` (one file in the media library).
- **UUID**: the primary identifier for a media item.
- **fjall**: the project's embedded KV store (see `src/db/`).
- **media search index**: the on-disk inverted index under `DATA_DIR/searchidx_media/` (tantivy-based).
- **type secondary indexes**: fjall keys under the `media:type:` prefix that enable fast listing/sorting/filtering by type/trash/mtime/name/size.

---

## 1. Data model: `MediaFile`

Media items are stored in fjall as JSON.

Key fields (see `src/media_scan.rs`):

- `UUID`: primary key.
- `FSUUID/Ino/Ctime`: a file-identity tuple derived from the filesystem UUID + inode + ctime.
- `Path`: current physical path (when trashed, this becomes the trash path).
- `OriginalPath`: original path before moving to trash (used for restore and bucket grouping).
- `Name/Size/ModifiedAt/Type`: file name, size, mtime, inferred media type (audio/video/image/other).
- `DurationSec/DurationRefMod/DurationRefSize`: best-effort cached duration for audio/video.
- `IsTrash/TrashPath/DeletedAt`: trash state.

`Type` is inferred from the filename extension by `inferType()`.

---

## 2. How the ID (UUID) is generated

### 2.1 File identity source

On Linux, media-item UUID generation is based on:

- **filesystem UUID** (FSUUID): resolved from the mount’s block device via `/dev/disk/by-uuid` (best-effort)
- `ino`: inode number (`st.Ino`)
- `ctimeSec`: inode change time in seconds (`st.Ctim.Sec`)

Implementation: `src/media_uuid.rs`.

### 2.2 UUID algorithm

`GenerateUUIDFromPath(path)` calls `uuidFromTriplet(fsUUID, ino, ctime)`:

1. Build a string: `"fsuuid:ino:ctime"`
2. Compute: `SHA1(namespace || tripletString)`
3. Take the first 16 bytes of the digest
4. Set RFC4122 variant bits and set version bits to 5
5. Format as `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`

This behaves similarly to UUIDv5 conceptually, but it is implemented directly in code with a custom namespace and SHA1 truncation.

### 2.3 Is this UUID stable?

Stability depends on whether `fsuuid/ino/ctime` stays stable:

- Rename/move within the same filesystem: `fsuuid` and `ino` stay, but `ctime` can change (rename updates inode ctime), so the UUID may change.
- Copy: inode changes, UUID changes.
- Move across filesystems: `fsuuid` changes, UUID changes.
- Metadata changes (permissions/owner): can update `ctime`, UUID may change.

PlainNAS also persists `FSUUID/Ino/Ctime` and keeps an `FID -> UUID` mapping (below) to preserve UUIDs when a file identity is already known.

### 2.4 Why do we also call `FindUUIDByFID`

Both `scan_file()` and `start_walk_and_scan()` generate a UUID, then do:

- `ex = find_uuid_by_fid(db, fsuuid, ino, ctime)`
- if `ex.is_some() && ex != id`, use `ex` instead

Call sites: `src/media_scan.rs`, `src/watcher.rs`.

This keeps the UUID stable for an already-known file identity (e.g., when historical data exists or if generation behavior changes).

---

## 3. Persistence layout (fjall KV)

Media items store primary records and several secondary/lookup keys.

### 3.1 Primary record

- Key: `media:uuid:<uuid>`
- Value: JSON-encoded `MediaFile`

Write path: `src/media_scan.rs` (`upsert_internal()`).

### 3.2 Lookup mappings

These allow fast lookups from path or file identity:

- Path -> UUID
  - Key: `media:path:<path>`
  - Value: `<uuid>`

- FID -> UUID
  - Key: `media:fid:<hash(fsuuid)>:<ino>:<ctime>`
  - Value: `<uuid>`

Lookup helpers:

- `FindByPath(path)`
- `FindUUIDByFID(fsuuid, ino, ctime)`

### 3.3 Type secondary indexes (fast listing/sorting)

`UpsertMedia()` maintains keys under `media:type:` (empty values) to support fast iteration by `type + trash + sortKey`.

Examples:

- `media:type:audio:trash:0:mod:00000000017000000000:<uuid>`
- `media:type:audio:trash:0:moddesc:...:<uuid>`
- `media:type:audio:trash:0:name:<normalizedName>:<uuid>`
- `media:type:audio:trash:0:namedesc:<byteInvertedName>:<uuid>`
- `media:type:audio:trash:0:size:<paddedSize>:<uuid>`
- `media:type:audio:trash:0:sizedesc:<invertedSize>:<uuid>`

Implementation: `src/media_scan.rs` (type index keys maintained in `upsert_internal()`).

Type indexes are maintained inline during upsert operations.

---

## 4. Indexing for media items (search + listing)

PlainNAS has two primary indexing mechanisms for media items:

1. **Type secondary indexes (inside fjall)**: fast path for empty-query list/count/sort.
2. **On-disk inverted search index (tantivy)**: used by `MediaSearchIndex::search()` for full-text search over name/path/artist/title.

### 4.0 Scan exclusions (what never enters the media library)

The media scan (`rebuildMediaIndex`, watcher events, uploads) refuses a set of
paths via `media_scan::is_media_excluded` — system/program files must not
swamp the images/videos/audios views:

- **System roots** (never descended from `/`): `/proc /sys /dev /run /tmp /snap /usr /etc /var /boot /opt /srv /lib /lib32 /lib64 /bin /sbin`.
- **Hidden entries**: any path component starting with `.` (`.git`, `.cache`, the app's own `.nas-trash`, …) — same rule the watcher already applied to events.
- **Program dir names** anywhere in the tree (case-insensitive): `node_modules target dist build vendor`.
- **The app's own `DATA_DIR` and cache dir** (kills the thumbnail-cache feedback loop).
- **Config extras**: `[media_scan] excluded_dirs = "/path/one,/path/two"` in `config.toml` (absolute paths, comma-separated), merged at startup.

The files manager (`files` query) lists the live filesystem directly, so
excluded paths remain fully browsable there — they just never join the media
library, its counts, or its buckets. Exclusions apply at scan time; a
`rebuildMediaIndex` is needed to drop rows indexed by older versions.

### 4.1 Type secondary indexes (fast path)

Typical usage: `src/gql/query.rs` (media list/count queries).

When `text == ""` and no `ids:` filter is present, the code can:

- Build a prefix with `media:type:<type>:<uuid>`
- Iterate with fjall `scan_prefix(prefix)` (natural key order)
- Extract UUID from the key suffix
- Load the full record via `get_by_uuid(db, uuid)`

This avoids scanning and unmarshalling the full `media:uuid:` corpus.

### 4.2 Search inverted index (`searchidx_media/` — tantivy)

Index directory: `DATA_DIR/searchidx_media/`.

Engine: [tantivy](https://crates.io/crates/tantivy) — a Rust full-text search engine library (Lucene-compatible).

Schema fields: `uuid`, `name`, `path`, `dir` (tokenized components, `excluded_dir:` filter), `parent` (exact parent dir, per-bucket queries), `media_type`, `size`/`modified` (FAST, sortable), `duration`, `artist`, `title`, `is_trash`.

Build entry point: `src/media/search_index.rs` (`MediaSearchIndex::build_from_db()`).

Build summary:

- Iterate all `media:uuid:` records from fjall
- For each record: add a tantivy document with the schema fields
- Commit and reload the reader

Query entry point: `src/media/search_index.rs` (`MediaSearchIndex::search()`).

Query strategy:

- Parse query text via tantivy's `QueryParser` (searches name, path, artist, title)
- Apply optional `media_type` and `is_trash` filters as term queries
- Combine with boolean query
- Return paginated results

Supported filters:

- `type`: audio/video/image/other
- `trash`: true/false
- `path_prefix`: can contain multiple prefixes separated by `|`

### 4.3 When indexes are built

- On startup: `src/watcher.rs` (`build_missing_indexes()`)
  - If `MediaSearchIndex::exists()` is false:
    - run `build_from_db()` to index all media records from fjall

- Via GraphQL: `rebuildMediaIndex(root)`
  - Calls `reset_all()` (clears fjall media data)
  - Starts `start_walk_and_scan()` to repopulate fjall

---

## 5. Media-item business logic (by feature)

### 5.1 Scan and sync

- `start_walk_and_scan()`: walks directories, upserts items, reports progress, and cleans up missing files.
- Progress event: `consts::EVENT_MEDIA_SCAN_PROGRESS` via `eventbus`.
- Source-dir whitelist: `db::media_source::get()`; when set, only paths under these prefixes are indexed.
- Explicitly skipped:
  - `.nas-trash` (unified trash directory)
  - system dirs like `/proc`, `/sys`, `/dev`, etc.
  - hidden directories/files (`.` prefix)

### 5.2 Single-file updates (watcher / uploads)

- `scan_file(db, path)`: builds a `MediaFile` and calls `upsert_internal()`.
- The watcher (`src/watcher.rs`) calls `scan_file` on create/modify events.

Typical call sites:

- after copy/move: `src/api/fs.rs`
- after upload merge: `src/chunked_upload.rs`

### 5.3 Trash / restore / delete

GraphQL batch actions: `trashMediaItems`, `restoreMediaItems`, `deleteMediaItems`.

Implementation:

- Trash: moves file into `.nas-trash`, updates `path`, `is_trash=true`, `deleted_at`
- Restore: moves file back to `original_path`
- Delete permanently: deletes the file, then calls `delete_by_uuid()` to remove metadata and secondary keys

Note: `delete_by_uuid()` removes fjall entries; tantivy index entries are removed on next index rebuild.

### 5.4 Listing, counting, sorting

GraphQL list/count for audios/videos/images is primarily implemented in `src/gql/query.rs`:

- Empty query: scan `media:type:` prefix in fjall
- Text query: use `MediaSearchIndex::search()` (tantivy-backed)
- Sorting:
  - fjall prefix scan is naturally sorted by key encoding
  - tantivy results are scored by relevance

### 5.5 Buckets (directory grouping)

- Bucket ID: FNV-1a 32-bit hash of the parent directory path
- Bucket list: `src/gql/query.rs` (`media_buckets`)
  - when mediaType is specified: iterates the type index so `topItems` tend to be recent
  - for default: scans all `media:uuid:` (excluding trash)

### 5.6 Duration caching

- `media::metadata::ensure_duration(mf)`: best-effort extracts audio/video duration via `lofty`, caches into `duration_sec`, and persists via `upsert_internal()`.
- In list views, duration probing is deferred to only the final paginated items to avoid expensive full-corpus probing.

### 5.7 Encrypted “fileId” for URLs (not the UUID)

`src/crypto.rs` provides `GenerateEncryptedFileID(path)`:

- Uses the global `urlToken` as the key (`db::UrlToken::new().ensure()` ensures it exists)
- Encrypts the file path with ChaCha20 and returns a base64 string

This is an opaque ID for sharing/URLs; it is not the media item UUID.

---

## 6. Troubleshooting

### 6.1 Search is slow / fallback is used

Likely cause: `searchidx_media/` is missing or corrupted, so `MediaSearchIndex::exists()` returns false.

What to do:

- Restart the service so watcher startup rebuilds it (`src/watcher.rs::build_missing_indexes()`), or
- Trigger GraphQL `rebuildMediaIndex(root)` to repopulate fjall and rebuild the tantivy index.

### 6.2 Listing/sorting/counting is not using the fast path

Likely cause: missing `media:type:` secondary indexes.

- Startup and upsert operations maintain type indexes inline in `upsert_internal()`.
- In development, deleting the fjall DB and rebuilding is acceptable.

---

## 7. Quick reference (code entry points)

- Data model: `src/media_scan.rs` (`MediaFile` struct)
- UUID generation (Linux): `src/media_uuid.rs`
- Upsert/Delete/mappings: `src/media_scan.rs` (`upsert_internal()`, `delete_by_uuid()`)
- Scan/sync: `src/media_scan.rs` (`start_walk_and_scan()`, `scan_file()`)
- Trash: `src/trash.rs`
- Type secondary indexes: `src/media_scan.rs` (inline in `upsert_internal()`)
- Search inverted index: `src/media/search_index.rs` (tantivy)
- File search index: `src/search_index.rs` (tantivy)
- Metadata extraction: `src/media/metadata.rs` (lofty)
- Cover art: `src/media/cover.rs` (lofty + sidecar)
- Thumbnails: `src/media/thumbnail.rs` (image + fast_image_resize)
- File watcher: `src/watcher.rs` (notify)
- PDF preview: `src/pdf_preview.rs` (LibreOffice)
- GraphQL queries: `src/gql/query.rs`
- GraphQL mutations: `src/gql/mutation.rs`
