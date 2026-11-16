import { clearRemoteClientId, setRemoteClientId } from './client-id'

export function switchDevice(id: string) {
  if (id) setRemoteClientId(id)
  else clearRemoteClientId()
  if (__IS_TAURI__) window.location.href = '/'
  else window.location.reload()
}
