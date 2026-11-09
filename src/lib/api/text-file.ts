import { getDesktopClientId } from '@/lib/device/client-id'
import { getUrlToken } from './api'
import { isLocalMode } from '@/lib/device/local-mode'
import { getFileId, tokenToKey } from './file'

export function getTextFileId(key: Uint8Array | null, path: string, local = isLocalMode()): string {
  if (!local) return getFileId(key, path)
  return getFileId(tokenToKey(getUrlToken()), path, '', { mode: 'c', id: getDesktopClientId() })
}
