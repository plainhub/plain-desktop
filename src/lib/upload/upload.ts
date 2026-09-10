import type { IUploadItem } from '@/stores/temp'
import emitter from '@/plugins/eventbus'
import { sha256 } from '@noble/hashes/sha2.js'
import { bytesToHex } from '@noble/hashes/utils.js'
import { getApiBaseUrl, getLocalToken, proxyUrlFor } from '../api/api'
import { chachaEncrypt, bitArrayToUint8Array } from '../api/crypto'
import { tokenToKey } from '../api/file'
import { uploadedChunksGQL } from '../api/query'
import { mergeChunksGQL, deleteChunksGQL } from '../api/mutation'
import { gqlFetch } from '../api/gql-client'
import { getCurrentAuthToken } from '../device/current'
import { get as prefsGet } from '../prefs'
import { isLocalMode } from '../device/local-mode'

const CHUNK_SIZE = 10 * 1024 * 1024 // 10MB — fewer chunks, lower HTTP overhead; per-chunk retry cost acceptable on weak networks
const PARALLEL_CHUNKS = 3 // Upload 3 chunks in parallel per file
const UPDATE_INTERVAL = 500 // 500ms
const MAX_CONCURRENT_CHUNKS = 4 // Global limit across ALL concurrent file uploads

// Global chunk upload concurrency limiter.
// When multiple files upload simultaneously (3 files × 3 workers = 9 requests),
// the phone server can be overwhelmed. This caps total in-flight chunk uploads.
let activeChunkUploads = 0

async function acquireChunkSlot(upload: IUploadItem): Promise<boolean> {
  while (activeChunkUploads >= MAX_CONCURRENT_CHUNKS) {
    if (upload.status === 'paused' || upload.status === 'canceled') return false
    await new Promise<void>((resolve) => setTimeout(resolve, 200))
  }
  activeChunkUploads++
  return true
}

function releaseChunkSlot() {
  activeChunkUploads = Math.max(0, activeChunkUploads - 1)
}

// Uploads go through XHR (progress events), so the self-signed-cert
// workaround is the `_pt`-tagged local proxy URL, not tauriFetch.
export function getUploadUrl() {
  return proxyUrlFor(getApiBaseUrl(), '/upload')
}

export function getUploadChunkUrl() {
  return proxyUrlFor(getApiBaseUrl(), '/upload_chunk')
}

interface IUploadChunk {
  index: number
  chunk: Blob
}

// Unified progress update function
function updateUploadProgress(upload: IUploadItem, newSize: number, forceUpdate: boolean = false) {
  const currentTime = Date.now()

  // `forceUpdate` is used to push the latest size to the UI even if we're
  // inside the update interval (e.g. on chunk boundaries). We intentionally
  // do NOT compute speed for these forced updates to avoid unrealistically
  // high spikes from tiny time deltas.
  const shouldUpdateSpeed = !!upload.lastUpdateTime && currentTime - upload.lastUpdateTime >= UPDATE_INTERVAL
  const shouldUpdateSize = forceUpdate || !upload.lastUpdateTime || shouldUpdateSpeed

  if (shouldUpdateSize) {
    upload.uploadedSize = newSize
  }

  if (!upload.lastUpdateTime) {
    upload.lastUpdateTime = currentTime
    upload.lastUploadedSize = newSize
    upload.uploadSpeed = 0
    return
  }

  if (shouldUpdateSpeed) {
    const timeDiffSec = (currentTime - upload.lastUpdateTime!) / 1000
    const sizeDiff = newSize - (upload.lastUploadedSize || 0)

    upload.uploadSpeed = sizeDiff > 0 && timeDiffSec > 0 ? Math.round(sizeDiff / timeDiffSec) : 0
    upload.lastUpdateTime = currentTime
    upload.lastUploadedSize = newSize
    return
  }

  if (forceUpdate) {
    // Keep baselines in sync so the next timed speed update is stable.
    upload.lastUpdateTime = currentTime
    upload.lastUploadedSize = newSize
  }
}

// Initialize upload status
function initializeUpload(upload: IUploadItem) {
  upload.uploadSpeed = 0
  upload.lastUploadedSize = upload.uploadedSize || 0
  upload.lastUpdateTime = Date.now()
}

// SHA-256 via @noble/hashes is pure JS, so the id is identical on insecure
// origins (plain HTTP over LAN) where crypto.subtle is undefined. The id only
// needs to be unique per file, not secret — 32 hex chars give 128 bits.
export async function generateFileId(file: File) {
  const meta = new TextEncoder().encode(`${file.name}:${file.size}:${file.lastModified}`)
  const sample = new Uint8Array(await file.slice(0, Math.min(2 * 1024 * 1024, file.size)).arrayBuffer())
  const input = new Uint8Array(meta.length + sample.length)
  input.set(meta, 0)
  input.set(sample, meta.length)
  return bytesToHex(sha256(input)).substring(0, 32)
}

// Uploads sharing a fileId share one server-side chunk directory, and the
// first merge deletes that directory. Never let two of them run at once.
const lastRunByFileId = new Map<string, Promise<unknown>>()

async function runExclusively<T>(fileId: string, action: () => Promise<T>): Promise<T> {
  const previous = lastRunByFileId.get(fileId)
  const run = (previous ? previous.catch(() => {}) : Promise.resolve()).then(action)
  lastRunByFileId.set(fileId, run)
  try {
    return await run
  } finally {
    if (lastRunByFileId.get(fileId) === run) lastRunByFileId.delete(fileId)
  }
}

// mergeChunks gained a totalSize argument in app 3.3.22. Servers before that
// rejected the whole mutation over the unknown argument, but version-gating on
// the app query is unreliable (deployed servers reported the version code as a
// number, not the version name), so the client always sends totalSize and
// expects a paired app release.

export async function upload(upload: IUploadItem, replace: boolean) {
  // In local mode the local server is the only client, so it has no
  // per-session token — the upload `info` part is encrypted with the
  // local server's URL token (same key the server uses to decrypt).
  const token = isLocalMode() ? getLocalToken() : getCurrentAuthToken()
  const key = tokenToKey(token)

  // Initialize upload status
  initializeUpload(upload)

  try {
    if (upload.file.size > CHUNK_SIZE) {
      // Chunked upload with parallel transfers and resume support
      return await uploadWithChunks(upload, replace, key)
    } else {
      // Small files: direct upload (no chunking overhead)
      return await uploadDirect(upload, replace, key)
    }
  } catch (error: any) {
    if (error.name === 'AbortError' || upload.status === 'paused') {
      console.log('Upload was paused/aborted')
      return { error: 'Upload paused' }
    }
    throw error
  }
}

async function uploadDirect(upload: IUploadItem, replace: boolean, key: Uint8Array) {
  try {
    const data = new FormData()
    const v = bitArrayToUint8Array(chachaEncrypt(key, JSON.stringify({ dir: upload.dir, replace, isAppFile: upload.isAppFile ?? false, size: upload.file.size })))
    data.append('info', new Blob([v]))
    // Explicitly pass the base filename to prevent browsers from including
    // webkitRelativePath in the Content-Disposition filename parameter.
    const baseName = upload.file.name.split('/').pop() || upload.file.name
    data.append('file', upload.file, baseName)

    return new Promise((resolve, reject) => {
      const xhr = new XMLHttpRequest()

      xhr.upload.addEventListener(
        'progress',
        (e) => {
          if (e.lengthComputable) {
            updateUploadProgress(upload, e.loaded)
            emitter.emit('upload_progress', upload)
          }
        },
        false
      )

      xhr.upload.addEventListener(
        'load',
        () => {
          upload.uploadedSize = upload.file.size
          upload.status = 'saving'
        },
        false
      )

      xhr.onreadystatechange = function () {
        if (xhr.readyState === 4) {
          if (xhr.status === 201) {
            if (upload.isAppFile) {
              // Server returns SHA-256 hash for app files; keep original local filename
              upload.fileHash = xhr.responseText
            } else {
              upload.fileName = xhr.responseText
            }
            upload.status = 'done'
            resolve({ fileName: xhr.responseText })
            emitter.emit('upload_progress', upload)
          } else if (xhr.status === 0) {
            resolve({ error: 'Upload aborted' })
            emitter.emit('upload_progress', upload)
          } else {
            upload.status = 'error'
            upload.error = xhr.responseText
            resolve({ error: xhr.responseText })
            emitter.emit('upload_progress', upload)
          }
        }
      }

      xhr.onerror = () => {
        upload.status = 'error'
        upload.error = 'Network error'
        resolve({ error: 'Network error' })
        emitter.emit('upload_progress', upload)
      }

      xhr.onabort = () => {
        console.log('Upload aborted')
        resolve({ error: 'Upload aborted' })
        emitter.emit('upload_progress', upload)
      }

      try {
        xhr.open('POST', getUploadUrl(), true)
        xhr.setRequestHeader('c-id', prefsGet('client_id', ''))
        upload.xhr = xhr
        xhr.send(data)
      } catch (ex: any) {
        upload.status = 'error'
        upload.error = ex.message
        resolve({ error: ex.message })
      }
    })
  } catch (error: any) {
    upload.status = 'error'
    upload.error = error.message || 'Upload failed'
  }
}

async function uploadWithChunks(upload: IUploadItem, replace: boolean, key: Uint8Array) {
  // Generate file ID
  if (!upload.fileId) {
    upload.fileId = await generateFileId(upload.file)
  }
  const fileId = upload.fileId

  return runExclusively(fileId, () => uploadChunkedFile(upload, fileId, replace, key))
}

async function uploadChunkedFile(upload: IUploadItem, fileId: string, replace: boolean, key: Uint8Array) {
  try {
    if (upload.status === 'paused') {
      return { error: 'Upload paused' }
    }

    upload.isChunked = true
    const totalChunks = Math.ceil(upload.file.size / CHUNK_SIZE)

    // Query uploaded chunks with sizes for verification
    const verifiedChunks = await getUploadedChunks(fileId, upload.file.size, totalChunks)
    if (upload.status === 'paused') {
      return { error: 'Upload paused' }
    }

    upload.uploadedChunks = [...verifiedChunks]

    // Calculate initial progress from already-uploaded chunks
    let completedBytes = 0
    for (const idx of verifiedChunks) {
      const chunkEnd = Math.min((idx + 1) * CHUNK_SIZE, upload.file.size)
      const chunkStart = idx * CHUNK_SIZE
      completedBytes += chunkEnd - chunkStart
    }
    updateUploadProgress(upload, completedBytes, true)
    emitter.emit('upload_progress', upload)

    // Build list of chunks that still need uploading
    const pendingIndices: number[] = []
    for (let i = 0; i < totalChunks; i++) {
      if (!verifiedChunks.includes(i)) {
        pendingIndices.push(i)
      }
    }

    // Track per-chunk uploaded bytes for accurate progress
    const chunkProgress = new Map<number, number>()

    const recalcProgress = () => {
      let total = completedBytes
      for (const bytes of chunkProgress.values()) {
        total += bytes
      }
      updateUploadProgress(upload, total)
      emitter.emit('upload_progress', upload)
    }

    // Upload pending chunks with parallel workers
    let cursor = 0
    const errors: string[] = []

    const uploadNextChunk = async (): Promise<void> => {
      while (cursor < pendingIndices.length) {
        if (upload.status === 'canceled' || upload.status === 'paused') return

        const myIdx = cursor++
        if (myIdx >= pendingIndices.length) return

        // Wait for a global chunk upload slot to avoid overwhelming the server
        if (!(await acquireChunkSlot(upload))) return

        const chunkIndex = pendingIndices[myIdx]
        const chunkData = createChunk(upload.file, chunkIndex, CHUNK_SIZE)

        const onProgress = (bytes: number) => {
          // Only increase — never decrease. When a chunk retry starts, the
          // new XHR reports progress from 0. Without this guard the total
          // would drop, making the progress bar visibly go backward.
          const prev = chunkProgress.get(chunkIndex) || 0
          if (bytes > prev) {
            chunkProgress.set(chunkIndex, bytes)
            recalcProgress()
          }
        }

        try {
          const success = await uploadChunkWithRetry(upload, chunkData, key, onProgress)

          if (!success) {
            if (upload.status === 'paused') return
            errors.push(`chunk ${chunkIndex}`)
            return
          }

          // Chunk completed — move its bytes to completedBytes
          chunkProgress.delete(chunkIndex)
          const chunkSize = chunkData.chunk.size
          completedBytes += chunkSize
          updateUploadProgress(upload, completedBytes, true)
          emitter.emit('upload_progress', upload)

          verifiedChunks.push(chunkIndex)
          upload.uploadedChunks = [...verifiedChunks]
        } finally {
          releaseChunkSlot()
        }
      }
    }

    // Launch parallel workers
    const workers = Math.min(PARALLEL_CHUNKS, pendingIndices.length)
    const workerPromises: Promise<void>[] = []
    for (let w = 0; w < workers; w++) {
      workerPromises.push(uploadNextChunk())
    }
    await Promise.all(workerPromises)

    if (upload.status === 'canceled' || upload.status === 'paused') {
      return { error: 'Upload paused' }
    }

    if (errors.length > 0) {
      upload.status = 'error'
      upload.error = `Failed to upload: ${errors.join(', ')}`
      return
    }

    // All chunks uploaded — merge on server
    upload.status = 'saving'
    const baseName = upload.file.name.split('/').pop() || upload.file.name
    const filePath = upload.dir.endsWith('/') ? upload.dir + baseName : upload.dir + '/' + baseName

    const result = await gqlFetch(mergeChunksGQL, {
      fileId,
      totalChunks,
      path: filePath,
      replace: replace,
      isAppFile: upload.isAppFile ?? false,
      totalSize: upload.file.size,
    })

    if (result?.data?.mergeChunks) {
      const returned = result.data.mergeChunks as string
      // Server returns "path_or_hash:size" — verify merged size matches original
      const colonIdx = returned.lastIndexOf(':')
      const serverValue = colonIdx > 0 ? returned.substring(0, colonIdx) : returned
      const serverSize = colonIdx > 0 ? parseInt(returned.substring(colonIdx + 1), 10) : 0

      if (serverSize > 0 && serverSize !== upload.file.size) {
        upload.status = 'error'
        upload.error = `Server merged size ${serverSize} != expected ${upload.file.size}`
        return
      }

      if (upload.isAppFile) {
        upload.fileHash = serverValue
      } else {
        upload.fileName = serverValue
      }
      upload.status = 'done'
    } else {
      upload.status = 'error'
      upload.error = 'Failed to merge chunks'
    }
  } catch (error: any) {
    if (error.name === 'AbortError' || upload.status === 'paused') {
      return { error: 'Upload paused' }
    }
    upload.status = 'error'
    upload.error = error.message || 'Upload failed'
  }
}

// Get list of uploaded chunks, verified by size
async function getUploadedChunks(fileId: string, fileSize: number, totalChunks: number): Promise<number[]> {
  try {
    const result = await gqlFetch(uploadedChunksGQL, { fileId })
    const raw: string[] = result.data?.uploadedChunks ? [...result.data.uploadedChunks] : []
    if (raw.length === 0) return []

    const verified: number[] = []
    let mismatchCount = 0
    for (const entry of raw) {
      // Format: "index:size"
      const parts = entry.split(':')
      const index = parseInt(parts[0], 10)
      const serverSize = parts.length > 1 ? parseInt(parts[1], 10) : -1
      if (isNaN(index) || index < 0 || index >= totalChunks) {
        mismatchCount++
        continue
      }

      // Calculate expected chunk size
      const chunkStart = index * CHUNK_SIZE
      const expectedSize = Math.min(CHUNK_SIZE, fileSize - chunkStart)
      if (serverSize > 0 && serverSize !== expectedSize) {
        console.warn(`Chunk ${index} size mismatch: server=${serverSize}, expected=${expectedSize}. Will re-upload.`)
        mismatchCount++
        continue
      }
      verified.push(index)
    }

    // All server chunks are stale (e.g. from a previous upload with a different
    // chunk size). Delete them to free disk space and avoid confusion.
    if (verified.length === 0 && mismatchCount > 0) {
      console.warn(`All ${mismatchCount} server chunks are stale for ${fileId}. Deleting.`)
      await gqlFetch(deleteChunksGQL, { fileId })
    }

    return verified
  } catch (error) {
    console.error('Failed to query uploaded chunks:', error)
    return []
  }
}

// Create chunk data
function createChunk(file: File, index: number, chunkSize: number): IUploadChunk & { start: number; end: number } {
  const start = index * chunkSize
  const end = Math.min(start + chunkSize, file.size)
  return {
    index,
    chunk: file.slice(start, end),
    start,
    end,
  }
}

async function uploadChunkWithRetry(upload: IUploadItem, chunkData: IUploadChunk & { start: number; end: number }, key: Uint8Array, onProgress: (bytes: number) => void, maxRetries: number = 5): Promise<boolean> {
  for (let attempt = 1; attempt <= maxRetries; attempt++) {
    if (upload.status === 'canceled' || upload.status === 'paused') {
      return false
    }

    try {
      const success = await uploadChunk(upload, chunkData, key, onProgress)
      if (success) {
        return true
      }

      if (upload.status === 'paused') {
        return false
      }

      console.warn(`Chunk ${chunkData.index} upload failed on attempt ${attempt}`)

      // Wait before retry with exponential backoff
      if (attempt < maxRetries) {
        // Don't reset onProgress(0) here — it causes the progress bar to go
        // backward while waiting. The retry's XHR progress events will
        // naturally overwrite the old chunk progress value.
        await waitWithPauseCheck(upload, Math.min(1000 * Math.pow(2, attempt - 1), 10000))
      }
    } catch (error: any) {
      if (upload.status === 'paused') {
        return false
      }

      console.warn(`Chunk ${chunkData.index} upload error on attempt ${attempt}:`, error.message)

      if (attempt < maxRetries) {
        await waitWithPauseCheck(upload, Math.min(1000 * Math.pow(2, attempt - 1), 10000))
      }
    }
  }

  console.error(`Failed to upload chunk ${chunkData.index} after ${maxRetries} attempts`)
  return false
}

// Interruptible wait function
async function waitWithPauseCheck(upload: IUploadItem, delay: number): Promise<void> {
  const startTime = Date.now()
  while (Date.now() - startTime < delay) {
    if (upload.status === 'paused') {
      return
    }
    await new Promise((resolve) => setTimeout(resolve, 100))
  }
}

async function uploadChunk(upload: IUploadItem, chunkData: IUploadChunk & { start: number; end: number }, key: Uint8Array, onProgress: (bytes: number) => void): Promise<boolean> {
  return new Promise((resolve) => {
    const data = new FormData()
    const info = JSON.stringify({
      fileId: upload.fileId,
      index: chunkData.index,
      size: chunkData.chunk.size,
    })
    const v = bitArrayToUint8Array(chachaEncrypt(key, info))
    data.append('info', new Blob([v]))
    data.append('file', chunkData.chunk, upload.file.name)

    const xhr = new XMLHttpRequest()
    let excludeSize = 0

    xhr.upload.addEventListener(
      'progress',
      (e) => {
        if (e.lengthComputable) {
          if (excludeSize === 0) {
            excludeSize = e.total - chunkData.chunk.size
          }
          if (e.loaded > excludeSize) {
            onProgress(e.loaded - excludeSize)
          }
        }
      },
      false
    )

    xhr.onreadystatechange = function () {
        if (xhr.readyState === 4) {
          upload.xhrs?.delete(xhr)
          if (xhr.status === 201) {
            // Server returns "index:savedSize" — verify
            const resp = xhr.responseText
            const parts = resp.split(':')
            if (parts.length === 2) {
              const savedSize = parseInt(parts[1], 10)
              if (savedSize !== chunkData.chunk.size) {
                console.warn(`Chunk ${chunkData.index} size mismatch after upload: server=${savedSize}, expected=${chunkData.chunk.size}`)
                resolve(false)
                return
              }
            }
            resolve(true)
          } else if (xhr.status === 0) {
            console.log(`Chunk ${chunkData.index} upload was aborted`)
            resolve(false)
          } else {
            console.warn(`Chunk ${chunkData.index} upload failed with status ${xhr.status}: ${xhr.responseText}`)
            resolve(false)
          }
        }
    }

    xhr.onerror = () => {
      upload.xhrs?.delete(xhr)
      console.warn(`Chunk ${chunkData.index} upload network error`)
      resolve(false)
    }

    xhr.onabort = () => {
      upload.xhrs?.delete(xhr)
      console.log(`Chunk ${chunkData.index} upload was aborted`)
      resolve(false)
    }

    try {
      xhr.open('POST', getUploadChunkUrl(), true)
      xhr.setRequestHeader('c-id', prefsGet('client_id', ''))
      // Track this XHR in the set BEFORE sending, so pause can abort it
      if (!upload.xhrs) upload.xhrs = new Set()
      upload.xhrs.add(xhr)
      upload.xhr = xhr
      xhr.send(data)
    } catch (ex: any) {
      upload.xhrs?.delete(xhr)
      console.warn(`Chunk ${chunkData.index} upload exception:`, ex.message)
      resolve(false)
    }
  })
}
