import { initMutation, trashSmsGQL, restoreSmsGQL } from '@/lib/api/mutation'
import emitter from '@/plugins/eventbus'

export const useSmsTrash = () => {
  const { mutate, onDone: onTrashed } = initMutation({
    document: trashSmsGQL,
  })

  const pending: string[] = []

  onTrashed(() => {
    const query = pending.shift()
    if (query === undefined) return
    emitter.emit('media_items_actioned', { type: 'SMS', action: 'trash', query })
  })

  return {
    trash(query: string) {
      pending.push(query)
      mutate({ query })
    },
  }
}

export const useSmsRestore = () => {
  const { mutate, onDone: onRestored } = initMutation({
    document: restoreSmsGQL,
  })

  const pending: string[] = []

  onRestored(() => {
    const query = pending.shift()
    if (query === undefined) return
    emitter.emit('media_items_actioned', { type: 'SMS', action: 'restore', query })
  })

  return {
    restore(query: string) {
      pending.push(query)
      mutate({ query })
    },
  }
}
