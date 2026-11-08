import { getApiBaseUrl, getApiHeaders } from './api'
import { FileAction, fileQuery } from './file-action'
import { encryptedGqlPost } from './gql-client'
import { encryptResourcePayload, type Resource } from './resource-link'
import { getCurrentAuthToken } from '@/lib/device/current'

export function managedPath(peerId: string, path: string): string {
  return `/proxy/${encodeURIComponent(peerId)}/${path}`
}

export function managedGraphql<T>(peerId: string, query: string, variables?: Record<string, unknown>) {
  return encryptedGqlPost<T>(getApiBaseUrl() + managedPath(peerId, 'graphql'), getCurrentAuthToken(), query, variables)
}

export function managedFileUrl(peerId: string, key: Uint8Array, resource: Resource, action = FileAction.Preview, query = ''): string {
  const scope = { mode: 'c' as const, id: getApiHeaders()['c-id'] }
  const id = encryptResourcePayload(key, scope, `managed:${peerId}:fs:${action}`, JSON.stringify(resource))
  return getApiBaseUrl() + managedPath(peerId, `fs?id=${encodeURIComponent(id)}${fileQuery(action, query)}`)
}
