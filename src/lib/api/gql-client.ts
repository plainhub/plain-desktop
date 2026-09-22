import { getApiBaseUrl, getApiHeaders, getLocalToken } from './api'
import { chachaEncrypt, chachaDecrypt, arrayBufferToBitArray, bitArrayToUint8Array } from './crypto'
import { tokenToKey } from './file'
import { wrapWithReplayProtection } from './time-sync'
import { getCurrentAuthToken, clearCurrentSession } from '../device/current'
import { httpRequest } from './http'
import { isLocalMode } from '../device/local-mode'
import type { GqlOperationName, GqlResultType, GqlVariablesType } from './graphql/operations'

const TIMEOUT = 30000

export interface GqlResult<T = any> {
  data: T
  errors?: Array<{ message: string; path?: string[] }>
}

// Deduplicate concurrent identical requests (same query + variables).
// If an identical request is already in-flight, callers share the same promise.
const pendingRequests = new Map<string, Promise<GqlResult<any>>>()

export interface GqlFetchOptions {
  dedupe?: boolean
  fresh?: boolean
  timeout?: number
}

export async function gqlFetch<T = any>(
  query: string,
  variables?: Record<string, any>,
  options: GqlFetchOptions = {},
): Promise<GqlResult<T>> {
  const dedupeKey = JSON.stringify({ query, variables })
  if (options.dedupe !== false && !options.fresh) {
    const pending = pendingRequests.get(dedupeKey)
    if (pending) return pending as Promise<GqlResult<T>>
  }

  const promise = doGqlFetch<T>(query, variables, options.timeout)
  if (options.dedupe !== false) pendingRequests.set(dedupeKey, promise)
  try {
    return await promise
  } finally {
    if (pendingRequests.get(dedupeKey) === promise) pendingRequests.delete(dedupeKey)
  }
}

/** Typed `gqlFetch` over the codegen'd operation table: pass the document
 *  const's exported name (the key in `GqlOperations`) next to the document
 *  itself, and the result/variables shapes come from the SDL-derived types.
 *  The name must match the document — the graphql contract test validates
 *  every document under its key, so a mismatch pair is a review-visible
 *  mistake, not a silent one. */
export async function gqlFetchOp<Name extends GqlOperationName>(
  name: Name,
  query: string,
  variables: GqlVariablesType<Name>,
  options?: GqlFetchOptions,
): Promise<GqlResult<GqlResultType<Name>>> {
  return gqlFetch<GqlResultType<Name>>(query, variables as Record<string, any> | undefined, options)
}

/** POSTs one XChaCha20-encrypted GraphQL request to `url` under `token` —
 *  the shared wire protocol for the current server (gqlFetch) and for login
 *  peers reached directly (gqlFetchPeer). */
export async function encryptedGqlPost<T = any>(
  url: string,
  token: string,
  query: string,
  variables?: Record<string, any>,
  timeout: number = TIMEOUT,
): Promise<GqlResult<T>> {
  const key = tokenToKey(token)
  const json = JSON.stringify({ query, variables })
  // Enabled automatically in dev mode (see main.ts); the flag check runs before
  // any string building so the disabled path is one property read.
  if (window.__PLAIN_LOG__) console.info(`[request] ${json}`)

  const startTime = performance.now()
  const body = bitArrayToUint8Array(chachaEncrypt(key, wrapWithReplayProtection(json)))
  const encryptTime = performance.now()

  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), timeout)

  try {
    const response = await httpRequest(url, {
      method: 'POST',
      headers: getApiHeaders(),
      body,
      signal: controller.signal,
    })

    if (response.status === 401) {
      throw new GqlError('unauthorized', 401)
    } else if (response.status === 403) {
      throw new GqlError('desktop_access_disabled', 403)
    }

    const arrayBuffer = await response.arrayBuffer()
    const apiEndTime = performance.now()
    const text = chachaDecrypt(key, arrayBufferToBitArray(arrayBuffer))
    const decryptEndTime = performance.now()

    if (window.__PLAIN_LOG__) {
      console.info(`[response] ${text}`)
      console.info(`[time] encrypt: ${encryptTime - startTime}ms, api: ${apiEndTime - encryptTime}ms, decrypt: ${decryptEndTime - apiEndTime}ms`)
    }

    return JSON.parse(text)
  } catch (e: any) {
    if (e instanceof GqlError) throw e
    if (e.name === 'AbortError') throw new GqlError('connection_timeout')
    throw new GqlError(e.message || 'network_error')
  } finally {
    clearTimeout(timer)
  }
}

async function doGqlFetch<T = any>(query: string, variables?: Record<string, any>, timeout: number = TIMEOUT): Promise<GqlResult<T>> {
  const url = `${getApiBaseUrl()}/graphql`
  const token = isLocalMode() ? getLocalToken() : getCurrentAuthToken()
  try {
    return await encryptedGqlPost<T>(url, token, query, variables, timeout)
  } catch (e) {
    // Web-mode 401: drop the stored session and hard-reload. Tauri is
    // excluded — local mode has no device session, so a reload would loop.
    if (e instanceof GqlError && e.status === 401 && !__IS_TAURI__) {
      clearCurrentSession()
      window.location.reload()
    }
    throw e
  }
}

export class GqlError extends Error {
  constructor(
    message: string,
    public status?: number,
  ) {
    super(message)
    this.name = 'GqlError'
  }
}
