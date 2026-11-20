import { showActionError } from '@/hooks/show-action-error'
import { ref, type Ref } from 'vue'
import type { ICall, ITag, ISim } from '@/lib/interfaces'
import { callGQL, deleteCallsGQL, initMutation } from '@/lib/api/mutation'
import { simsGQL, initQuery } from '@/lib/api/query'
import emitter from '@/plugins/eventbus'
import { DataType } from '@/lib/data'

interface UseCallsActionsOptions {
  items: Ref<ICall[]>
  tags: Ref<ITag[]>
  total: Ref<number>
}

export function useCallsActions(opts: UseCallsActionsOptions) {
  const { items, tags, total } = opts
  const dataType = DataType.CALL

  const callId = ref('')
  const sims = ref<ISim[]>([])
  const { mutate: mutateCall, loading: callLoading } = initMutation({ document: callGQL }, showActionError)

  initQuery({
    document: simsGQL,
    handle(data: any, error: string) {
      if (!error) sims.value = data?.sims ?? []
    },
  })

  function call(item: ICall) {
    callId.value = item.id
    mutateCall({ number: item.number, showDialer: false })
  }

  const { mutate: doDeleteCall, onDone: onDeleteCallDone } = initMutation({ document: deleteCallsGQL }, showActionError)
  const pendingDeleteCall = ref<ICall | null>(null)

  onDeleteCallDone(() => {
    const item = pendingDeleteCall.value
    if (item) {
      items.value = items.value.filter((it) => it.id !== item.id)
      total.value--
      if (item.tags.length) emitter.emit('refetch_tags', dataType)
      emitter.emit('calls_deleted')
      pendingDeleteCall.value = null
    }
  })

  function deleteItem(item: ICall) {
    pendingDeleteCall.value = item
    doDeleteCall({ query: `ids:${item.id}` })
  }

  return { callId, callLoading, call, deleteItem }
}
