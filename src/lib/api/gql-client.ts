import { AccessErrorCode } from '@/lib/api/access-error-code'
import { HttpErrorCode } from '@/lib/api/http-error-code'
import { NativeErrorCode } from '@/lib/api/native-error-code'
import { PeerConnectionErrorCode } from '@/lib/api/peer-connection-error-code'
import { GqlError, type GraphqlFailure } from './gql-error'
import { shallowRef } from 'vue'
import { isLocalMode } from '@/lib/device/local-mode'
import { getApiBaseUrl, getApiHeaders } from './api'
import { chachaEncrypt, httpRequestAad, decryptGraphqlResponse } from './crypto'
import { tokenToKey } from './file'
import { graphqlRequestPayload } from './graphql-request'
import { getCurrentAuthToken, clearCurrentSession } from '../device/current'
import { getLoginPeerRevision } from '../device/login-peers'
import { getActiveClientId, getRemoteClientId } from '../device/client-id'
import { httpRequest } from './http'
import { getDeviceRequestPath, getWebManagedPeerId } from './device-path'
import { managedGraphqlDocument } from './managed-document'
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
  errors?: GraphqlFailure[]
}

const pendingRequests = new Map<string, Promise<GqlResult<any>>>()

export interface GqlFetchOptions {
  dedupe?: boolean
  fresh?: boolean
  timeout?: number
  origin?: boolean
}

function graphqlContext(origin = false) {
  const native = __IS_TAURI__
  const local = isLocalMode()
  const peer = origin ? '' : getWebManagedPeerId()
  const base = native ? '' : getApiBaseUrl()
  const clientId = getActiveClientId()
  const target: NativeTarget | undefined = native ? local ? { kind: 'LOCAL' } : { kind: 'PEER', peerId: getRemoteClientId() } : undefined
  return {
    native,
    clientId,
    target,
    url: native ? '' : base + getDeviceRequestPath('/graphql', peer),
    originUrl: native ? '' : base + '/graphql',
    peer,
    origin,
    token: native ? '' : getCurrentAuthToken(),
    authRevision: target?.kind === 'PEER' ? getLoginPeerRevision(target.peerId) : 0,
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
  const context = graphqlContext(options.origin)
  const contextKey = JSON.stringify(context)
  const dedupeKey = JSON.stringify({ context, query, variables })
  if (options.dedupe !== false && !options.fresh) {
    const pending = pendingRequests.get(dedupeKey)
    if (pending) return pending as Promise<GqlResult<T>>
  }

  const promise = doGqlFetch<T>(context, query, variables, options.timeout).then((result) => {
    if (contextKey !== JSON.stringify(graphqlContext(options.origin))) throw GqlError.fromCode(HttpErrorCode.RequestContextChanged, undefined, true)
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
  if (!token) throw GqlError.fromCode(AccessErrorCode.Unauthorized, 401)
  const key = tokenToKey(token)
  const json = graphqlRequestPayload(query, variables)
  const op = operationName(query)
  const host = (() => {
    try {
      return new URL(url).host
    } catch {
      return url
    }
  })()
  // Vite dev mode and desktop debug builds enable this flag before startup.
  if (window.__PLAIN_LOG__) {
    console.info(`[gql] → ${op} @${host} ${json}`)
  }

  const startTime = performance.now()
  const body = chachaEncrypt(key, json, httpRequestAad('/graphql'))
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
      throw GqlError.fromCode(AccessErrorCode.Unauthorized, 401)
    } else if (response.status === 403) {
      throw GqlError.fromCode(AccessErrorCode.RemoteAccessDisabled, 403)
    } else if (!response.ok) {
      throw GqlError.fromCode(HttpErrorCode.NetworkError, response.status)
    }

    const arrayBuffer = await response.arrayBuffer()
    const apiEndTime = performance.now()
    const text = decryptGraphqlResponse(key, body, new Uint8Array(arrayBuffer))
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
    if (e.name === 'AbortError') throw GqlError.fromCode(PeerConnectionErrorCode.ConnectionTimeout)
    throw new GqlError(e.message || HttpErrorCode.NetworkError, undefined, false, HttpErrorCode.NetworkError)
  } finally {
    clearTimeout(timer)
  }
}

// Core resolves the target's identity and credentials; the WebView supplies no URL or key.
export async function nativeGqlFetch<T = any>(query: string, variables?: Record<string, any>, timeout: number = TIMEOUT, target: NativeTarget = { kind: 'LOCAL' }, beforeDispatch?: () => void): Promise<GqlResult<T>> {
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
    if (e instanceof GqlError) throw e
    const message = e?.message || NativeErrorCode.NativeError
    const code: string = message || NativeErrorCode.NativeError
    if (window.__PLAIN_LOG__) console.warn(`[gql] ✗ ${op} @native ${message}`)
    throw new GqlError(message, code === AccessErrorCode.Unauthorized ? 401 : code === AccessErrorCode.RemoteAccessDisabled ? 403 : undefined, code === HttpErrorCode.RequestContextChanged, code)
  }
}

export const managedPeerFailure = shallowRef<{ peerId: string; peerName?: string; originName?: string }>()
let checkedManagedContext = ''

async function checkManagedPeer(context: ReturnType<typeof graphqlContext>, status: 401 | 502) {
  const key = JSON.stringify(context)
  if (!context.token || checkedManagedContext === key || key !== getGraphqlContextKey()) return
  checkedManagedContext = key
  if (status === 502) {
    managedPeerFailure.value = { peerId: context.peer }
    return
  }
  let shown = false
  try {
    const { data } = await gqlFetch<{ app?: { deviceName?: string }; peers?: { id: string; name: string }[] }>(
      'query { app { deviceName } peers { id name } }', undefined, { origin: true },
    )
    if (key !== getGraphqlContextKey() || !data?.app) return
    shown = true
    managedPeerFailure.value = {
      peerId: context.peer,
      peerName: data.peers?.find(peer => peer.id === context.peer)?.name,
      originName: data.app.deviceName,
    }
  } catch {
    // The origin request handles its own 401; an inconclusive check must not imply broken pairing.
  } finally {
    if (checkedManagedContext === key && !shown) checkedManagedContext = ''
  }
}

async function doGqlFetch<T = any>(context: ReturnType<typeof graphqlContext>, query: string, variables?: Record<string, any>, timeout: number = TIMEOUT): Promise<GqlResult<T>> {
  try {
    if (context.native) return await nativeGqlFetch<T>(query, variables, timeout, context.target)
    return await webGqlFetch<T>(context, query, variables, timeout, true)
  } catch (e) {
    if (e instanceof GqlError && e.status === 401 && !context.peer && (context.native || context.token) &&
      context.target?.kind !== 'LOCAL' && JSON.stringify(context) === JSON.stringify(graphqlContext(context.origin))) {
      await clearCurrentSession()
      window.location.reload()
    }
    throw e
  }
}

export function bindGraphqlClient(origin = false) {
  const context = graphqlContext(origin)
  const target = context.target
  const beforeDispatch = target?.kind === 'PEER' ? () => {
    if (context.authRevision !== getLoginPeerRevision(target.peerId)) throw GqlError.fromCode(HttpErrorCode.RequestContextChanged, undefined, true)
  } : undefined
  return <T = any>(query: string, variables?: Record<string, any>, timeout = TIMEOUT): Promise<GqlResult<T>> =>
    context.native ? nativeGqlFetch<T>(query, variables, timeout, context.target, beforeDispatch) : webGqlFetch<T>(context, query, variables, timeout)
}

let originResourceToken: { scope: string; promise: Promise<string> } | undefined

async function webGqlFetch<T>(context: ReturnType<typeof graphqlContext>, query: string, variables: Record<string, unknown> | undefined, timeout: number, checkManagedFailure = false): Promise<GqlResult<T>> {
  const document = context.peer ? managedGraphqlDocument(query) : query
  const result = await encryptedGqlPost<T>(context.url, context.token, document, variables, timeout, context.headers).catch((error: unknown) => {
    if (checkManagedFailure && context.peer && error instanceof GqlError && (error.status === 401 || error.status === 502)) {
      void checkManagedPeer(context, error.status)
    }
    throw error
  })
  if (!context.peer || document === query || result.errors?.length) return result
  const app = (result.data as { app?: { urlToken?: string } } | undefined)?.app
  if (app && query.includes('...AppFragment')) {
    // Existing resource IDs are sealed for this origin; only the gateway sees the peer's key.
    const scope = JSON.stringify([context.originUrl, context.token, context.headers['c-id']])
    if (originResourceToken?.scope !== scope) {
      const promise = encryptedGqlPost<{ app: { urlToken: string } }>(
        context.originUrl, context.token, '{ app { urlToken } }', undefined, timeout, context.headers,
      ).then(response => {
        const key = response.data?.app?.urlToken
        if (!key) throw response.errors?.length ? GqlError.fromGraphql(response.errors[0]) : GqlError.fromCode(AccessErrorCode.Unauthorized)
        return key
      }).catch(error => {
        if (originResourceToken?.promise === promise) originResourceToken = undefined
        throw error
      })
      originResourceToken = { scope, promise }
    }
    app.urlToken = await originResourceToken.promise
  }
  return result
}
