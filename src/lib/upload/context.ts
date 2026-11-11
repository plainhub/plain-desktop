import type { IUploadItem } from '@/stores/temp'
import { getApiBaseUrl, proxyUrlFor } from '@/lib/api/api'
import { bindGraphqlClient, getGraphqlContextKey, GqlError } from '@/lib/api/gql-client'
import { nativeViewScope, type NativeTarget } from '@/lib/api/native-client'
import { getDesktopClientId, getRemoteClientId } from '@/lib/device/client-id'
import { getLoginPeerRevision } from '@/lib/device/login-peers'
import { getCurrentAuthToken } from '@/lib/device/current'
import { isLocalMode } from '@/lib/device/local-mode'

interface UploadContextBase {
  key: string
  graphql: ReturnType<typeof bindGraphqlClient>
  scope?: Promise<string>
}

export interface WebUploadContext extends UploadContextBase {
  native: false
  token: string
  clientId: string
  uploadUrl: string
  chunkUrl: string
}

type UploadContext = WebUploadContext | (UploadContextBase & { native: true; target: NativeTarget; authRevision: number })

const contexts = new WeakMap<IUploadItem, UploadContext>()

export function getUploadContext(upload: IUploadItem): UploadContext {
  let context = contexts.get(upload)
  if (!context) {
    const base = { key: getGraphqlContextKey(), graphql: bindGraphqlClient() }
    if (__IS_TAURI__) {
      const target: NativeTarget = isLocalMode() ? { kind: 'local' } : { kind: 'peer', peerId: getRemoteClientId() }
      context = { ...base, native: true, target, authRevision: target.kind === 'peer' ? getLoginPeerRevision(target.peerId) : 0 }
    } else {
      const baseUrl = getApiBaseUrl()
      context = {
        ...base,
        native: false,
        token: getCurrentAuthToken(),
        clientId: getDesktopClientId(),
        uploadUrl: proxyUrlFor(baseUrl, '/upload'),
        chunkUrl: proxyUrlFor(baseUrl, '/upload_chunk'),
      }
    }
    contexts.set(upload, context)
  }
  return context
}

export function isUploadContextCurrent(upload: IUploadItem): boolean {
  return getUploadContext(upload).key === getGraphqlContextKey()
}

export function assertUploadContext(upload: IUploadItem): void {
  if (!isUploadContextCurrent(upload)) throw new GqlError('request_context_changed', undefined, true)
}

export function getUploadNativeScope(upload: IUploadItem): Promise<string> {
  const context = getUploadContext(upload)
  return context.scope ??= nativeViewScope()
}

export function uploadChunkOwner(upload: IUploadItem, fileId = upload.fileId): string {
  const context = getUploadContext(upload)
  return JSON.stringify([context.native ? context.target : { kind: 'web', url: context.chunkUrl }, fileId])
}

export function isUploadAuthCurrent(upload: IUploadItem): boolean {
  const context = getUploadContext(upload)
  return !context.native || context.target.kind === 'local' || context.authRevision === getLoginPeerRevision(context.target.peerId)
}

const cancellations = new WeakMap<IUploadItem, Set<() => void>>()

export function registerUploadCancellation(upload: IUploadItem, cancel: () => void): () => void {
  let callbacks = cancellations.get(upload)
  if (!callbacks) cancellations.set(upload, callbacks = new Set())
  callbacks.add(cancel)
  return () => {
    callbacks.delete(cancel)
    if (!callbacks.size) cancellations.delete(upload)
  }
}

export function cancelUploadOperations(upload: IUploadItem): void {
  for (const cancel of cancellations.get(upload) ?? []) cancel()
}
