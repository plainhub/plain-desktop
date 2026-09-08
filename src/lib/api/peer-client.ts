import { deviceBaseUrl } from './api'
import { encryptedGqlPost, type GqlResult } from './gql-client'
import type { LoginPeer } from '../device/login-peers'
import { peerHost } from '../device/login-peers'

/**
 * POSTs a GraphQL request to a login peer's own server, encrypted with that
 * peer's login token. Local mode only — the desktop server has no proxy for
 * peer features, so the client reaches each device directly (Tauri fetch
 * accepts the devices' self-signed certificates).
 */
export async function gqlFetchPeer<T = any>(
  peer: LoginPeer,
  query: string,
  variables?: Record<string, any>,
): Promise<GqlResult<T>> {
  return encryptedGqlPost<T>(`${deviceBaseUrl(peerHost(peer))}/graphql`, peer.token, query, variables)
}
