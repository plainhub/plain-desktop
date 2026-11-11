import { isLocalMode } from '@/lib/device/local-mode'
import { getApiBaseUrl, getApiHeaders } from './api'
import { chachaEncrypt, chachaDecrypt, httpRequestAad, httpResponseAad, arrayBufferToBitArray, bitArrayToUint8Array } from './crypto'
import { tokenToKey } from './file'
import { wrapWithReplayProtection } from './time-sync'
import { getCurrentAuthToken, clearCurrentSession } from '../device/current'
import { getLoginPeerRevision } from '../device/login-peers'
import { getActiveClientId, getRemoteClientId } from '../device/client-id'
import { httpRequest } from './http'
import { nativeGraphql, type NativeTarget } from './native-client'
import type { GqlOperationName, GqlResultType, GqlVariablesType } from './graphql/operations'

const TIMEOUT = 30000

/** Operation label for the [gql] log lines: the operation's own name when it
 *  has one (`query foo(` / `mutation bar(`), otherwise its first top-level
 *  field (`query { app { … } }` → `app`). */
function operationName(query: string): string {
  const named = query.match(/\b(?:query|mutation|subscription)\s+([A-Za-z_][\w]*)/)
  if (named) return named[1]
  const kw = query.match(/\b(?:query|mutation|subscription)/)
  const body = kw ? query.slice(kw.index!) : query
  const field = body.match(/\{\s*(?:\.\.\.\s*)?([A-Za-z_][\w]*)/)
  return field ? field[1] : '(anonymous)'
}

export interface GqlResult<T = any> {
  data: T
  errors?: Array<{ message: string; path?: string[] }>
}

const pendingRequests = new Map<string, Promise<GqlResult<any>>>()

export interface GqlFetchOptions {
  dedupe?: boolean
  fresh?: boolean
  timeout?: number
}

function graphqlContext() {
  const native = __IS_TAURI__
  const local = isLocalMode()
  const clientId = getActiveClientId()
  const target: NativeTarget | undefined = native ? local ? { kind: 'local' } : { kind: 'peer', peerId: getRemoteClientId() } : undefined
  return {
    native,
    clientId,
    target,
    url: native ? '' : `${getApiBaseUrl()}/graphql`,
    token: native ? '' : getCurrentAuthToken(),
    authRevision: target?.kind === 'peer' ? getLoginPeerRevision(target.peerId) : 0,
    headers: { ...getApiHeaders() },
  }
}

export function getGraphqlContextKey(): string {
  return JSON.stringify(graphqlContext())
}

export async function gqlFetch<T = any>(
  query: string,
  variables?: Record<string, any>,
  options: GqlFetchOptions = {},
): Promise<GqlResult<T>> {
  const context = graphqlContext()
  const contextKey = JSON.stringify(context)
  const dedupeKey = JSON.stringify({ context, query, variables })
  if (options.dedupe !== false && !options.fresh) {
    const pending = pendingRequests.get(dedupeKey)
    if (pending) return pending as Promise<GqlResult<T>>
  }

  const promise = doGqlFetch<T>(context, query, variables, options.timeout).then((result) => {
    if (contextKey !== getGraphqlContextKey()) throw new GqlError('request_context_changed', undefined, true)
    return result
  })
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

// Browser requests share the encrypted protocol for the selected server and direct peers.
export async function encryptedGqlPost<T = any>(
  url: string,
  token: string,
  query: string,
  variables?: Record<string, any>,
  timeout: number = TIMEOUT,
  headers: Record<string, string> = getApiHeaders(),
): Promise<GqlResult<T>> {
  const key = tokenToKey(token)
  const json = JSON.stringify({ query, variables })
  const op = operationName(query)
  const host = (() => {
    try {
      return new URL(url).host
    } catch {
      return url
    }
  })()
  // Enabled automatically in dev mode (see main.ts); the flag check runs before
  // any string building so the disabled path is one property read.
  if (window.__PLAIN_LOG__) {
    console.info(`[gql] → ${op} @${host} ${json}`)
  }

  const startTime = performance.now()
  const body = bitArrayToUint8Array(chachaEncrypt(key, wrapWithReplayProtection(json), httpRequestAad('/graphql')))
  const responseAad = httpResponseAad(body)
  const encryptTime = performance.now()

  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), timeout)

  try {
    const response = await httpRequest(url, {
      method: 'POST',
      headers,
      body,
      signal: controller.signal,
    })

    if (response.status === 401) {
      throw new GqlError('unauthorized', 401)
    } else if (response.status === 403) {
      throw new GqlError('remote_access_disabled', 403)
    }

    const arrayBuffer = await response.arrayBuffer()
    const apiEndTime = performance.now()
    const text = chachaDecrypt(key, arrayBufferToBitArray(arrayBuffer), responseAad)
    const decryptEndTime = performance.now()

    if (window.__PLAIN_LOG__) {
      console.info(
        `[gql] ← ${op} (${Math.round(apiEndTime - encryptTime)}ms·enc ${Math.round(encryptTime - startTime)}ms·dec ${Math.round(decryptEndTime - apiEndTime)}ms) ${text}`,
      )
    }

    return JSON.parse(text)
  } catch (e: any) {
    if (window.__PLAIN_LOG__) {
      console.warn(`[gql] ✗ ${op} @${host} ${e instanceof GqlError ? `${e.message} (${e.status ?? 'network'})` : e.name || e.message}`)
    }
    if (e instanceof GqlError) throw e
    if (e.name === 'AbortError') throw new GqlError('connection_timeout')
    throw new GqlError(e.message || 'network_error')
  } finally {
    clearTimeout(timer)
  }
}

// Core resolves the target's identity and credentials; the WebView supplies no URL or key.
export async function nativeGqlFetch<T = any>(query: string, variables?: Record<string, any>, timeout: number = TIMEOUT, target: NativeTarget = { kind: 'local' }, beforeDispatch?: () => void): Promise<GqlResult<T>> {
  const op = operationName(query)
  if (window.__PLAIN_LOG__) {
    console.info(`[gql] → ${op} @native ${JSON.stringify({ query, variables })}`)
  }

  const startTime = performance.now()
  try {
    const result = await nativeGraphql<GqlResult<T>>({ query, variables }, timeout, target, beforeDispatch)
    if (window.__PLAIN_LOG__) {
      console.info(`[gql] ← ${op} (${Math.round(performance.now() - startTime)}ms) ${JSON.stringify(result)}`)
    }
    return result
  } catch (e: any) {
    const message = e?.message || 'native_error'
    if (window.__PLAIN_LOG__) console.warn(`[gql] ✗ ${op} @native ${message}`)
    throw new GqlError(message, message === 'unauthorized' ? 401 : message === 'remote_access_disabled' ? 403 : undefined, message === 'request_context_changed')
  }
}

async function doGqlFetch<T = any>(context: ReturnType<typeof graphqlContext>, query: string, variables?: Record<string, any>, timeout: number = TIMEOUT): Promise<GqlResult<T>> {
  try {
    if (context.native) return await nativeGqlFetch<T>(query, variables, timeout, context.target)
    return await encryptedGqlPost<T>(context.url, context.token, query, variables, timeout, context.headers)
  } catch (e) {
    if (e instanceof GqlError && e.status === 401 && context.target?.kind !== 'local' && JSON.stringify(context) === getGraphqlContextKey()) {
      await clearCurrentSession()
      window.location.reload()
    }
    throw e
  }
}

export function bindGraphqlClient() {
  const context = graphqlContext()
  const target = context.target
  const beforeDispatch = target?.kind === 'peer' ? () => {
    if (context.authRevision !== getLoginPeerRevision(target.peerId)) throw new GqlError('request_context_changed', undefined, true)
  } : undefined
  return <T = any>(query: string, variables?: Record<string, any>, timeout = TIMEOUT): Promise<GqlResult<T>> =>
    context.native ? nativeGqlFetch<T>(query, variables, timeout, context.target, beforeDispatch) : encryptedGqlPost<T>(context.url, context.token, query, variables, timeout, context.headers)
}

export class GqlError extends Error {
  constructor(
    message: string,
    public status?: number,
    public cancelled = false,
  ) {
    super(message)
    this.name = 'GqlError'
  }
}
