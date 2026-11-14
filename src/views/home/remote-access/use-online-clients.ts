import { onUnmounted, ref } from 'vue'
import { Channel, invoke } from '@/lib/api/native-invoke'

export function useOnlineClients() {
  const count = ref<number | null>(null)
  const failed = ref(false)
  let disposed = false
  let registration: number | undefined
  const channel = new Channel<{ clientIds: string[] }>()
  channel.onmessage = (snapshot) => {
    if (!disposed) count.value = snapshot.clientIds.length
  }
  void invoke<number>('native_register_online_clients', { channel })
    .then((id) => {
      if (disposed) return invoke('native_unregister_online_clients', { id })
      registration = id
    })
    .catch((error) => {
      failed.value = true
      console.error('Online clients registration failed', error)
    })
  onUnmounted(() => {
    disposed = true
    if (registration !== undefined) void invoke('native_unregister_online_clients', { id: registration }).catch(console.error)
  })
  return { count, failed }
}
