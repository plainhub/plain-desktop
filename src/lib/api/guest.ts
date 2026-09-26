import { getApiBaseUrl, getProxyUrl } from './api'
import { httpRequest } from './http'
import {
  chachaEncrypt,
  chachaDecrypt,
  arrayBufferToBitArray,
  bitArrayToUint8Array,
  bitArrayToBase64,
} from './crypto'
import { wrapWithReplayProtection } from './time-sync'

/**
 * Client for the shared-file-link (`/s`) page.
 *
 * Talks to the plain-app **guest** endpoints:
 * - `POST /guest_graphql` — request body is ChaCha20-encrypted with the
 *   link's `shared_token` (from the URL fragment), `c-id` carries the public
 *   `shared_id`. Exposes only the `sharedInfo` query.
 * - `GET /fs` — file bytes; the `id` query param is `{sharedId, virtualPath}`
 *   ChaCha20-encrypted with the share's dedicated `url_token`, and `sid`
 *   carries the `shared_id`.
 * - `GET /zip/dir` — download a shared directory as a zip (same auth).
 *
 * Everything is isolated from the main desktop session (different key + id).
 */

const TIMEOUT = 30000

export interface SharedFile {
  name: string
  virtualPath: string
  isDir: boolean
  size: number
  mimeType: string
  hasThumb: boolean
}

export interface SharedInfo {
  name: string
  readOnly: boolean
  requiresPassword: boolean
  expiresAt: number | null
  urlToken: string
  entries: SharedFile[]
}

export interface GuestResult<T = any> {
  data: T
  errors?: Array<{ message: string }>
}

/**
 * Decode a base64url-encoded `shared_token` (unpadded, `-`/`_`) into the raw
 * 32-byte ChaCha20 key used for `/guest_graphql`.
 */
export function sharedTokenToKey(token: string): Uint8Array {
  const b64 = token.replace(/-/g, '+').replace(/_/g, '/')
  const pad = '='.repeat((4 - (b64.length % 4)) % 4)
  const bin = atob(b64 + pad)
  return Uint8Array.from(bin, (c) => c.charCodeAt(0))
}

export const sharedInfoGQL = `
  query sharedInfo($virtualPath: String) {
    sharedInfo(virtualPath: $virtualPath) {
      name
      readOnly
      requiresPassword
      expiresAt
      urlToken
      entries {
        name
        virtualPath
        isDir
        size
        mimeType
        hasThumb
      }
    }
  }
`

export async function guestFetch<T = any>(
  sharedId: string,
  key: Uint8Array,
  query: string,
  variables?: Record<string, any>,
): Promise<GuestResult<T>> {
  const url = `${getApiBaseUrl()}/guest_graphql`
  const payload = wrapWithReplayProtection(JSON.stringify({ query, variables }))
  const body = bitArrayToUint8Array(chachaEncrypt(key, payload))

  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), TIMEOUT)
  try {
    const response = await httpRequest(url, {
      method: 'POST',
      headers: { 'Content-Type': 'multipart/form-data', 'c-id': sharedId },
      body,
      signal: controller.signal,
    })
    if (response.status === 401) {
      throw new Error('unauthorized')
    } else if (response.status === 403) {
      throw new Error('forbidden')
    } else if (!response.ok) {
      throw new Error('bad_request')
    }
    const arrayBuffer = await response.arrayBuffer()
    const text = chachaDecrypt(key, arrayBufferToBitArray(arrayBuffer))
    return JSON.parse(text)
  } catch (e: any) {
    if (e.name === 'AbortError') throw new Error('connection_timeout')
    throw e
  } finally {
    clearTimeout(timer)
  }
}

/**
 * Encrypt the `{sharedId, virtualPath}` payload into the guest file id.
 */
export function getSharedFileId(urlToken: string, sharedId: string, virtualPath: string): string {
  const key = Uint8Array.from(atob(urlToken), (c) => c.charCodeAt(0))
  return bitArrayToBase64(chachaEncrypt(key, JSON.stringify({ sharedId, virtualPath })))
}

/** The `id=…&sid=…` query string shared by guest `/fs` and `/zip/dir`. */
function sharedFileQuery(urlToken: string, sharedId: string, virtualPath: string): string {
  return `id=${encodeURIComponent(getSharedFileId(urlToken, sharedId, virtualPath))}&sid=${encodeURIComponent(sharedId)}`
}

/**
 * Build a `/fs` URL for a shared file. `query` may add thumbnail params
 * (`&w=80&h=80`) or force download (`&dl=1`).
 */
export function getSharedFileUrl(
  urlToken: string,
  sharedId: string,
  virtualPath: string,
  query: string = '',
): string {
  return getProxyUrl(`/fs?${sharedFileQuery(urlToken, sharedId, virtualPath)}${query}`)
}

/**
 * Build a `/zip/dir` URL that downloads a shared directory as a zip.
 */
export function getSharedDirUrl(urlToken: string, sharedId: string, virtualPath: string): string {
  return getProxyUrl(`/zip/dir?${sharedFileQuery(urlToken, sharedId, virtualPath)}`)
}
