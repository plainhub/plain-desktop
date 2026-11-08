import { deviceBaseUrl, nativePeerGraphqlUrl } from './api'
import { encryptedGqlPost, type GqlResult } from './gql-client'
import type { LoginPeer } from '../device/login-peers'
import { peerHost } from '../device/login-peers'

export function getPeerBaseUrl(peer: LoginPeer): string {
  return peer.address?.baseUrl || deviceBaseUrl(peerHost(peer))
}

// Uses the peer's own authenticated session, independently of the selected device.
export async function gqlFetchPeer<T = any>(
  peer: LoginPeer,
  query: string,
  variables?: Record<string, any>,
): Promise<GqlResult<T>> {
  const url = __IS_TAURI__ ? nativePeerGraphqlUrl(peer.id) : `${getPeerBaseUrl(peer)}/graphql`
  return encryptedGqlPost<T>(url, peer.token, query, variables)
}
