import { ref } from 'vue'
import { usePageScope } from '@/hooks/page-scope'
import { Channel, invoke } from '@/lib/api/native-invoke'

export function useOnlineClients() {
  const count = ref<number | null>(null)
  const failed = ref(false)
  usePageScope(scope => {
    const channel = new Channel<{ clientIds: string[] }>()
    channel.onmessage = snapshot => {
      if (scope.active) { count.value = snapshot.clientIds.length; failed.value = false }
    }
    void invoke<number>('native_register_online_clients', { channel })
      .then(id => {
        scope.onDispose(() => { void invoke('native_unregister_online_clients', { id }).catch(console.error) })
      })
      .catch(error => {
        if (scope.active) failed.value = true
        console.error('Online clients registration failed', error)
      })
  })
  return { count, failed }
}
