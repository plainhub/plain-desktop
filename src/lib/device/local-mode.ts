import { getRemoteClientId } from './client-id'

export function isLocalMode(): boolean {
  return __IS_TAURI__ && !getRemoteClientId()
}
