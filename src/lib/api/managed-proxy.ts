import { ResourcePurpose } from './aad-purpose.generated'
import { getApiBaseUrl, getApiHeaders } from './api'
import { FileAction, fileQuery } from './file-action'
import { encryptedGqlPost } from './gql-client'
import { encryptResourcePayload, type Resource } from './resource-link'
import { getCurrentAuthToken } from '@/lib/device/current'
import { peerProxyPath } from '../url'

export function managedGraphql<T>(peerId: string, query: string, variables?: Record<string, unknown>) {
  if (__IS_TAURI__) throw new Error('Native peer-proxy GraphQL is unsupported')
  return encryptedGqlPost<T>(getApiBaseUrl() + peerProxyPath(peerId, 'graphql'), getCurrentAuthToken(), query, variables)
}

export function managedFileUrl(peerId: string, key: Uint8Array, resource: Resource, action = FileAction.Preview, query = ''): string {
  const scope = { mode: 'c' as const, id: getApiHeaders()['c-id'] }
  const id = encryptResourcePayload(key, scope, ResourcePurpose.Fs, JSON.stringify(resource))
  return getApiBaseUrl() + peerProxyPath(peerId, `fs?id=${encodeURIComponent(id)}${fileQuery(action, query)}`)
}
