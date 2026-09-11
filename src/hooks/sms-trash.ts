import { initMutation, trashSmsGQL, restoreSmsGQL } from '@/lib/api/mutation'
import emitter from '@/plugins/eventbus'

export const useSmsTrash = () => {
  const { mutate, onDone: onTrashed } = initMutation({
    document: trashSmsGQL,
  })

  onTrashed((r: any) => {
    const { query } = r.data.trashSms
    emitter.emit('media_items_actioned', { type: 'SMS', action: 'trash', query })
  })

  return {
    trash(query: string) {
      mutate({ query })
    },
  }
}

export const useSmsRestore = () => {
  const { mutate, onDone: onRestored } = initMutation({
    document: restoreSmsGQL,
  })

  onRestored((r: any) => {
    const { query } = r.data.restoreSms
    emitter.emit('media_items_actioned', { type: 'SMS', action: 'restore', query })
  })

  return {
    restore(query: string) {
      mutate({ query })
    },
  }
}
