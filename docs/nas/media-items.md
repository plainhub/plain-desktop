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

### Android MediaStore compatibility: what each media page shows

Desktop and NAS do not have Android MediaStore. Their shared scanner must reproduce the category and visibility rules that PlainApp Android users see. The category comes from the MIME type recognized for a file, not from its parent directory. `Pictures`, `Movies`, `Music`, and `Download` are common storage locations, not category filters. A file in Downloads can appear in any matching media page.

### Android directory traversal scope

Android's MediaProvider scans storage volumes it manages (the primary shared-storage volume and available external volumes); it does not define Images/Videos/Audios/Docs as scans limited to `DCIM`, `Pictures`, `Movies`, `Music`, or `Download`. Those are conventional public destinations and receive useful path metadata, but other visible directories on a scanned volume can also contribute media. The app's own Kotlin MediaStore helpers query indexed collections; the platform MediaScanner and volume lifecycle determine traversal scope.

The portable rule is **source scope → directory visibility → file MIME → PlainApp page filter**. The same rule must run on macOS, Windows, Linux, and NAS; only the discovery of the user's source scope and each platform's private directories differs. The outcome must not depend on whether a file is under a folder named `Pictures`, `Downloads`, or `Movies`.

| Host | Default source scope for the intended behavior | Private/system paths outside media scope |
|---|---|---|
| macOS | The current user's home directory, including standard and custom visible folders; add OS-resolved user content locations such as iCloud Drive when they live outside the visible home tree. | `~/Library` except a separately resolved user content source, `~/.Trash`, hidden folders, app bundles and photo-library package internals; never scan `/System`, `/Library`, `/Applications`, or the whole startup volume as a default root. |
| Windows | The current user's profile directory, including Known Folders and custom visible folders, using their resolved locations when redirected outside the profile. | `AppData`, profile/system hidden folders, Windows and Program Files trees, recycle bin and system volume metadata; never scan a whole drive by default. |
| Linux | The current user's home directory, including XDG user directories and custom visible folders. | Dot-prefixed directories such as `.cache`, `.config`, `.local`, and user application-private locations such as the profile's `snap` data; never scan `/`, `/proc`, `/sys`, `/var`, or `/usr` as default roots. |
| NAS | Configured user-data shares or media source roots. | Server OS, application data, package, cache, and trash roots. |

User-added source roots can include other local disks, removable media, redirected folders, or network shares. Resolve and deduplicate nested roots so each file is indexed once. A mount or drive is not a source merely because it exists: unlike Android, desktop operating systems do not expose one uniform MediaStore-managed set of shared-storage volumes. This is the explicit cross-platform mapping of Android's managed storage scope.

Within each source root, scan visible directories recursively. Apply these rules consistently on every platform:

- **Do not descend** into a subtree hidden by `.nomedia`; files beneath it must not enter media collections. Dot-prefixed paths are hidden from media categories as well.
- **Do not descend** into OS-hidden/system directories or application-private data roots, even when their names do not start with a dot. On Android these include `Android/data` and `Android/obb`; on desktop use the corresponding platform locations in the table above. Do not exclude any ordinary folder merely because it is called `data` or `obb`.
- **Honor native hidden metadata** as well as dot-prefixed names: macOS Finder/Unix hidden flags and Windows hidden or system attributes. An unreadable directory is logged and skipped.
- **Do not index** Android's internal thumbnail-cache directories such as `.thumbnails` under `Movies`, `Music`, and `Pictures` as user media.
- **Do not index** a recognized application's generated profile cache within an otherwise visible source. For example, `~/Movies/CapCut/User Data/Cache/effect/.../blusher.png` belongs to an application cache, analogous to Android app-private or `.nomedia` content. Recognize the app-private path structure (`User Data/Cache`, `User Data/Code Cache`, `User Data/GPUCache`, `User Data/CacheStorage`) or an explicitly configured excluded root; do not discard every user folder whose final component is `Cache`.
- **Scan other visible directories recursively**, including `Downloads` and user-created folders, and classify each file by MIME.
- **Do not follow symlinks, junctions, or reparse points** during recursive discovery. An explicitly selected target can be scanned as its own source after canonicalization; this prevents cycles and accidental traversal into private or unrelated volumes.
- Android has well-known volume-root exceptions and OS-version-specific handling for `.nomedia` in public paths. Reproduce the visible result for supported Android behavior without treating the names of public folders as an allowlist or blindly applying Android absolute paths on desktop/NAS.

This directory traversal scope is separate from the later UI query: MediaStore may retain a general Files row while marking the file's media type as none, and PlainApp's Docs query applies its own MIME and size conditions.

Examples for all desktop hosts: `Pictures/trip.jpg` and `Downloads/screenshot.png` enter Images; a visible `Projects/assets/logo.png` also enters Images; `Documents/report.pdf` enters Docs; `Pictures/Cache/edited.png` enters Images if `Cache` is just a user folder; `Pictures/Private/.nomedia` hides media beneath `Private`; `Movies/CapCut/User Data/Cache/effect/blusher.png` stays out of Images; a photo-library package's internal thumbnails stay out of Images. These examples use the same decision order regardless of host OS.

Desktop startup adds the home directory and OS-resolved standard user folders to the configured source roots, canonicalizes them, and removes nested duplicates. On macOS it also adds iCloud Drive when present. A changed source set starts a scan even when the index already has rows. A full `rebuildMediaIndex` applies the current rules to the configured roots; users upgrading from earlier scanning rules should rebuild once.

| PlainApp page | Android source | Include when | Exclude when |
|---|---|---|---|
| Images | `MediaStore.Images` (`image/*`) | MediaScanner recognizes an `image/*` MIME and the path is visible. Examples include JPEG, PNG, GIF, WebP, BMP, HEIF/HEIC. | Not `image/*`; hidden path or `.nomedia` subtree; Android-recognized dedicated album-art artwork. Do not limit to camera/DCIM/Pictures: public Downloads images can appear. |
| Videos | `MediaStore.Video` (`video/*`) | MediaScanner recognizes `video/*` and the path is visible. Examples include MP4, 3GP, WebM, Matroska. | Not `video/*` or hidden by the common visibility rules. Zero duration alone is not a reason to exclude; PlainApp Android has no `DURATION > 0` filter. |
| Audios | `MediaStore.Audio` (`audio/*`) | MediaScanner recognizes `audio/*` and the path is visible. Examples include MP3, M4A/AAC, WAV, Ogg/Opus, FLAC. | Not `audio/*` or hidden by the common visibility rules. Zero duration alone is not a reason to exclude; PlainApp Android sends zero-duration items to its duration repair flow. |
| Docs | PlainApp query over `MediaStore.Files` | MIME is `text/*`, or exactly in the extra MIME allowlist below, and size is greater than zero. | Zero-byte files and MIME outside those conditions. Unknown types remain browseable in Files. |
| Files | `MediaStore.Files` | General file browsing; media classification does not remove the file from Files. | Hidden names by default; PlainApp's explicit `show_hidden` query can include them. |

PlainApp Android's extra Docs MIME allowlist is `application/pdf`, `application/msword`, `application/vnd.openxmlformats-officedocument.wordprocessingml.document`, `application/vnd.openxmlformats-officedocument.spreadsheetml.sheet`, and `application/javascript`. This comes from `DocMediaStoreHelper.extraDocumentMimeTypes`. Do not treat every `application/*`, archive, executable, installer, or unknown MIME as a document. JSON/XML qualify only when Android resolves their MIME to `text/*`; they are not extra allowlist entries.

All four media pages follow the platform scanner visibility contract before MIME classification: dot-prefixed hidden files/directories are not shown; `.nomedia` hides media in that directory tree; Android-protected locations such as `Android/data` and `Android/obb` are not ordinary visible media. Android's image scanner also omits files it recognizes as dedicated album artwork; implement the corresponding filename rule rather than excluding all small images or all images outside camera folders.

Do not exclude arbitrary directories solely because their names are `Cache` or `Caches`: that is not a general Android MediaStore rule. Application-private profile caches and explicitly excluded roots are excluded by their location and purpose. Media exclusions affect media pages, counts, buckets, and media search only; they must not prevent normal file browsing.

The Android platform MediaScanner supplies the hidden-directory, `.nomedia`, and MIME recognition behavior; PlainApp's Kotlin query helpers do not implement those path rules themselves. Relevant sources in the Android app are `ImageMediaStoreHelper.kt`, `VideoMediaStoreHelper.kt`, `AudioMediaStoreHelper.kt`, `DocMediaStoreHelper.kt`, and `FileMediaStoreHelper.kt`.

Shared scanner, watcher updates, upload indexing, and full rebuilds must use the same classification and visibility predicate. If rules change, a rebuild must remove old rows and derived search/bucket data that no longer qualify. A non-empty index does not prove that every configured source directory has been scanned.

References: [Android MediaStore](https://developer.android.com/reference/android/provider/MediaStore), [MediaStore FileColumns](https://developer.android.com/reference/android/provider/MediaStore.Files.FileColumns), [shared media storage](https://developer.android.com/training/data-storage/shared/media), and [AOSP ModernMediaScanner](https://android.googlesource.com/platform/packages/providers/MediaProvider/+/f2abe4aec018f0522b4b1303fb25351db0604eb5/src/com/android/providers/media/scan/ModernMediaScanner.java).

PlainNAS has two primary indexing mechanisms for media items:

1. **Type secondary indexes (inside fjall)**: fast path for empty-query list/count/sort.
2. **On-disk inverted search index (tantivy)**: used by `MediaSearchIndex::search()` for full-text search over name/path/artist/title.

### Scan exclusions (what never enters the media library)

The media scan (`rebuildMediaIndex`, watcher events, uploads) refuses a set of
paths via `media_scan::is_media_excluded` — system/program files must not
swamp the images/videos/audios views:

- **System roots** (never descended from `/`): `/proc /sys /dev /run /tmp /snap /usr /etc /var /boot /opt /srv /lib /lib32 /lib64 /bin /sbin`.
- **Hidden entries**: any path component starting with `.` (`.git`, `.cache`, the app's own `.nas-trash`, …) — same rule the watcher already applied to events.
- **Application dependency tree**: `node_modules`.
- **Application profile cache**: `User Data/Cache`, `User Data/Caches`, `User Data/GPUCache`, `User Data/Code Cache`, `User Data/CacheStorage` (case-insensitive). A user folder named only `Cache` remains eligible.
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
