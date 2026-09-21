import { computed } from 'vue'
import { storeToRefs } from 'pinia'
import { useTempStore } from '@/stores/temp'
import { initMutation, deleteSmsGQL } from '@/lib/api/mutation'
import emitter from '@/plugins/eventbus'

/**
 * Whether real (system-provider) SMS deletion is possible right now:
 * the phone reports the ADB (Shizuku) permission in its `app` query.
 */
export const useSmsDeleteAvailable = () => {
  const { app } = storeToRefs(useTempStore())
  const available = computed(() => app.value?.permissions?.includes('ADB') === true)

  return { available }
}

export const useSmsDelete = () => {
  const { mutate, onDone: onDeleted, onError } = initMutation({
    document: deleteSmsGQL,
  })

  const pending: string[] = []

  onDeleted(() => {
    const query = pending.shift()
    if (query === undefined) return
    emitter.emit('media_items_actioned', { type: 'SMS', action: 'delete', query })
  })

  return {
    delete(query: string) {
      pending.push(query)
      mutate({ query })
    },
    onError: onError,
  }
}
