# File Upload System

## Overview

The file upload system supports both direct (small file) and chunked (large file) uploads with parallel transfer, resume capability, progress tracking, and integrity verification.

## Architecture

```
┌────────────────────────────────┐
│        Upload Queue            │  src/lib/upload/upload-queue.ts
│  Max 3 concurrent file uploads │
│  pause / resume / retry / cancel│
└──────────┬─────────────────────┘
           │
           ▼
┌────────────────────────────────┐
│      Upload Logic              │  src/lib/upload/upload.ts
│  ≤ 5MB → direct upload        │
│  > 5MB → chunked upload       │
└──────────┬─────────────────────┘
           │
     ┌─────┴──────┐
     ▼            ▼
 POST /upload   POST /upload_chunk   (backend: local/server/upload.rs)
 (single file)  (per-chunk, 5MB each)
                     │
                     ▼
               mergeChunks           (GraphQL mutation: local/graphql/schema/file_upload.rs)
               (concatenate + verify)
```

## Key Constants

| Constant | Value | Location |
|----------|-------|----------|
| `CHUNK_SIZE` | 5 MB (5,242,880 bytes) | `upload.ts` |
| `PARALLEL_CHUNKS` | 3 | `upload.ts` |
| `MAX_CONCURRENT_CHUNKS` | 4 | `upload.ts` |
| `maxConcurrent` (queue) | 3 | `upload-queue.ts` |
| Max retries per chunk | 5 | `uploadChunkWithRetry()` |
| Retry backoff | 1s, 2s, 4s, 8s, 10s (capped) | `uploadChunkWithRetry()` |
| Progress update interval | 500ms | `upload.ts` |

## Protocol

### 1. File ID Generation

Each file gets a deterministic ID based on `SHA-256(name + size + lastModified + first 2MB of content)`, truncated to 32 hex chars. This ID enables resume — the same file always gets the same ID.

**Function**: `generateFileId()` in `upload.ts`

### 2. Direct Upload (files ≤ 5MB)

- `POST /upload` with multipart form data
- Fields: `info` (ChaCha20-encrypted JSON: `{ dir, replace, isAppFile, size }`), `file` (the file blob)
- Server writes to a temp file, syncs to disk, atomically renames to final path
- Response: `201` with filename on success

### 3. Chunked Upload (files > 5MB)

#### Step 1: Query Existing Chunks (Resume Support)

- GraphQL query: `uploadedChunks(fileId)` → `["index:size", ...]`
- Backend lists files in `upload_tmp/{fileId}/` directory
- Frontend verifies each chunk's size matches expected: `min(CHUNK_SIZE, fileSize - chunkStart)`
- Chunks with size mismatch are skipped and re-uploaded

#### Step 2: Upload Chunks in Parallel

- 3 parallel workers pick chunks from a shared pending queue
- Each chunk: `POST /upload_chunk` with multipart: `info` (encrypted `{ fileId, index, size }`) + `file` (chunk blob)
- Backend writes to temp file, syncs, atomically renames to `chunk_{index}`
- Response: `201` with `"index:savedSize"`
- Frontend verifies `savedSize == chunkData.chunk.size`

#### Step 3: Merge

- GraphQL mutation: `mergeChunks(fileId, totalChunks, path, replace, isAppFile)`
- Backend concatenates all chunk files using `FileChannel.transferTo`
- Writes to a temp merge file, syncs, verifies `mergedSize == sum(chunkSizes)`
- Atomically renames to final output path
- Deletes chunk directory
- Response: `"filename:mergedSize"` or `"sha256hash:mergedSize"` (for app files)
- Frontend verifies `serverSize == file.size`

## Data Integrity Guarantees

| Layer | Check | What it prevents |
|-------|-------|------------------|
| Chunk upload (server) | `bytes.size == chunkInfo.size` | Truncated network transfer |
| Chunk write (server) | Atomic temp-file + rename | Partial writes from concurrent retries |
| Chunk write (server) | `fos.channel.position() == bytes.size` | Silent disk write failure |
| Chunk resume (client) | Size verification on `uploadedChunks` | Corrupt chunks from interrupted uploads |
| Chunk upload (client) | `savedSize == chunk.size` in response | Server-side write anomalies |
| Merge (server) | `mergedSize == sum(chunkSizes)` | Concatenation errors |
| Merge (client) | `serverSize == file.size` | End-to-end file size integrity |

## Concurrency Model

### Upload Queue (`upload-queue.ts`)

- Up to 3 files upload simultaneously
- Each file task: `pending → running → completed/failed/paused`
- Pause: aborts ALL active XHRs (via `xhrs` Set), sets task status to `paused`
- Resume: re-queries existing chunks, uploads remaining ones
- Retry: resets progress, re-enters queue

### Parallel Chunk Workers (`upload.ts`)

- Up to 3 chunk workers per file, but capped at 4 total concurrent chunk uploads globally (`MAX_CONCURRENT_CHUNKS`)
- Workers acquire a global chunk slot before each upload via `acquireChunkSlot()`
- Workers pull from a shared `pendingIndices` array via atomic cursor increment
- Each worker runs `uploadNextChunk()` in a loop until all chunks are done
- All active XHRs are tracked in `upload.xhrs` (a `Set<XMLHttpRequest>`)
- On pause/cancel, ALL XHRs in the set are aborted

## Progress Tracking

- Per-chunk progress via XHR `upload.progress` events
- `chunkProgress` Map tracks in-flight bytes for each chunk
- `completedBytes` tracks bytes from fully uploaded chunks
- `recalcProgress()` sums both for total progress
- Speed calculation: `(sizeDiff / timeDiff)` with 500ms minimum interval
- Forced updates on chunk boundaries for UI accuracy

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Network error | Retry with exponential backoff (up to 5 attempts) |
| Chunk size mismatch | Re-upload the corrupt chunk |
| All retries exhausted | File marked as `error`, user can retry manually |
| Merge fails | Error shown, chunks kept on server for retry |
| Pause during upload | All XHRs aborted, chunks kept for resume |
| Browser tab close | Chunks kept on server, resume on next visit |

## Security

- Upload info is encrypted with ChaCha20 using the session token
- Client ID header (`c-id`) required for authentication
- File paths are controlled server-side (no path traversal)

## Known Issues & Mitigations (Historical)

### Partial chunk writes (Fixed)

**Problem**: When multiple files were uploaded simultaneously and a pause/retry occurred, the server could end up with partially-written chunk files. This was caused by:

1. **Single XHR reference**: `upload.xhr` stored only the last of 3 parallel XHR objects. Pausing only aborted 1 of 3 active uploads, leaving 2 running. If the connection dropped mid-transfer, the server had partial data.

2. **Non-atomic chunk file writes**: The server wrote chunk data directly to `chunk_{index}`. If a retry request arrived while the original was still writing, both wrote to the same file, producing corrupt data.

**Fix**:
- Frontend: Track all active XHRs in a `Set<XMLHttpRequest>` (`upload.xhrs`). Pause/cancel aborts ALL of them.
- Backend: Write chunks to a temp file (`.tmp_chunk_{index}_{nanoTime}`), sync to disk, then atomically rename to `chunk_{index}`.

### Progress spike on chunk boundary

**Mitigation**: Forced progress updates on chunk boundaries skip speed recalculation to avoid unrealistically high speed spikes from tiny time deltas.

### Server renameTo returning false after moving file (Fixed)

**Problem**: On some Android file systems (FUSE), `File.renameTo()` can succeed (move the file) but return `false`. The fallback `File.copyTo()` then threw `NoSuchFileException` because the source file was already moved. This caused chunk uploads to fail with "The source file doesn't exist."

**Fix**:
- Backend (`HttpModule.kt`, `FileUploadGraphQL.kt`): Before calling `copyTo()` fallback, check if the source temp file still exists. If it was already moved (rename succeeded silently), verify the destination exists instead of failing.

### Progress bar going backward on chunk retry (Fixed)

**Problem**: When a chunk upload failed and retried, `onProgress(0)` was called to reset the chunk's progress. This subtracted the chunk's in-flight bytes from the total, causing the progress bar to briefly go backward before the retry started making progress.

**Fix**:
- Frontend (`upload.ts`): Removed `onProgress(0)` between retry attempts. The retry's XHR progress events naturally overwrite the old chunk progress value without any backward movement.

### Server overwhelmed by concurrent batch uploads (Fixed)

**Problem**: With 3 concurrent file uploads × 3 parallel chunk workers = 9 simultaneous HTTP requests, each carrying up to 5MB of data, the phone server's memory and I/O could be overwhelmed, causing timeouts and cascading retry storms.

**Fix**:
- Frontend (`upload.ts`): Added a global chunk upload concurrency limiter (`MAX_CONCURRENT_CHUNKS = 4`). Workers acquire a slot before each chunk upload and release it when done. This caps total in-flight chunk uploads across all files, regardless of how many files are uploading simultaneously.

### Post-rename chunk size verification (Added)

**Problem**: Under edge conditions, a chunk file could end up with incorrect size after the rename step (e.g., filesystem race, silent corruption).

**Fix**:
- Backend (`HttpModule.kt`): Added a post-rename verification that checks `chunkFile.length()` matches the expected byte count. If mismatched, the corrupt chunk file is deleted and the upload returns an error, forcing the client to re-upload the chunk.

## File Locations

| Component | Path |
|-----------|------|
| Upload core logic | `src/lib/upload/upload.ts` |
| Upload queue | `src/lib/upload/upload-queue.ts` |
| Upload hooks (UI) | `src/hooks/upload.ts` |
| Upload task controls | `src/hooks/upload-task.ts` |
| Upload list UI | `src/views/uploads/UploadList.vue` |
| IUploadItem type | `src/stores/temp.ts` |
| GraphQL queries | `src/lib/api/query.ts` (uploadedChunksGQL) |
| GraphQL mutations | `src/lib/api/mutation.ts` (mergeChunksGQL) |
| Backend HTTP endpoints | `plain-rs/src/server/upload.rs` (/upload, /upload_chunk) |
| Backend GraphQL schema | `plain-rs/src/api/schema/file_upload.rs` |
