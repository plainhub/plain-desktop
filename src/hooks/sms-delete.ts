import { ref } from 'vue'
import { initMutation, deleteSmsGQL } from '@/lib/api/mutation'
import { initLazyQuery, smsDeleteAvailableGQL } from '@/lib/api/query'
import emitter from '@/plugins/eventbus'

const available = ref<boolean | null>(null)
let probe: ReturnType<typeof initLazyQuery<{ smsDeleteAvailable: boolean }>> | null = null

/**
 * Whether real (system-provider) SMS deletion is possible right now:
 * Shizuku installed, running and granted. Probed lazily once per session;
 * `null` means the probe has not completed yet.
 */
export const useSmsDeleteAvailable = () => {
  if (!probe) {
    probe = initLazyQuery<{ smsDeleteAvailable: boolean }>({
      handle: (data, error) => {
        if (!error) available.value = data?.smsDeleteAvailable === true
        else available.value = false
      },
      document: smsDeleteAvailableGQL,
    })
  }

  return {
    available,
    probe() {
      void probe!.fetch(undefined, { force: true, latest: true })
    },
  }
}

export const useSmsDelete = () => {
  const { mutate, onDone: onDeleted, onError } = initMutation({
    document: deleteSmsGQL,
  })

  onDeleted((r: any) => {
    const { query } = r.data.deleteSms
    emitter.emit('media_items_actioned', { type: 'SMS', action: 'delete', query })
  })

  return {
    delete(query: string) {
      mutate({ query })
    },
    onError: onError,
  }
}
