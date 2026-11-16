import { getRemoteClientId } from '@/lib/device/client-id'
import { peerProxyPath } from '@/lib/url'

export function getWebManagedPeerId(): string {
  return __IS_TAURI__ ? '' : getRemoteClientId()
}

export function getDeviceRequestPath(path: string, peerId = getWebManagedPeerId()): string {
  if (peerId === '.' || peerId === '..') throw new Error('Invalid managed peer')
  return peerId && !path.startsWith('/proxy/') ? peerProxyPath(peerId, path) : path
}
