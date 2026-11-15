import { Channel, invoke } from '@/lib/api/native-invoke'
import { isLocalMode } from './local-mode'

export async function subscribeOnlineClients(receive: (clientIds: string[]) => void) {
  if (!isLocalMode()) throw new Error('native_only')
  const channel = new Channel<{ clientIds: string[] }>()
  channel.onmessage = ({ clientIds }) => receive(clientIds)
  const id = await invoke<number>('native_register_online_clients', { channel })
  return () => invoke<void>('native_unregister_online_clients', { id })
}
