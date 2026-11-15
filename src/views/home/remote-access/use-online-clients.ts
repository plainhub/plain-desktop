import { onActivated, onDeactivated, onUnmounted, ref } from 'vue'
import { Channel, invoke } from '@/lib/api/native-invoke'

export function useOnlineClients() {
  const count = ref<number | null>(null)
  const failed = ref(false)
  let active = false
  let generation = 0
  let registration: number | undefined

  function activate() {
    if (active) return
    active = true
    const current = ++generation
    const channel = new Channel<{ clientIds: string[] }>()
    channel.onmessage = (snapshot) => {
      if (active && current === generation) {
        count.value = snapshot.clientIds.length
        failed.value = false
      }
    }
    void invoke<number>('native_register_online_clients', { channel })
      .then((id) => {
        if (!active || current !== generation) return invoke('native_unregister_online_clients', { id })
        registration = id
      })
      .catch((error) => {
        if (active && current === generation) failed.value = true
        console.error('Online clients registration failed', error)
      })
  }
  function deactivate() {
    active = false
    generation++
    if (registration !== undefined) {
      const id = registration
      registration = undefined
      void invoke('native_unregister_online_clients', { id }).catch(console.error)
    }
  }
  activate()
  onActivated(activate)
  onDeactivated(deactivate)
  onUnmounted(deactivate)
  return { count, failed }
}
