import { encryptedGqlPost, nativeGqlFetch, type GqlResult } from './gql-client'
import type { LoginPeer } from '../device/login-peers'
import { getPeerBaseUrl } from '../device/login-peers'

// Uses the peer's own authenticated session, independently of the selected device.
export async function gqlFetchPeer<T = any>(
  peer: LoginPeer,
  query: string,
  variables?: Record<string, any>,
): Promise<GqlResult<T>> {
  return __IS_TAURI__
    ? nativeGqlFetch<T>(query, variables, undefined, { kind: 'peer', peerId: peer.id })
    : encryptedGqlPost<T>(`${getPeerBaseUrl(peer)}/graphql`, peer.token, query, variables)
}
