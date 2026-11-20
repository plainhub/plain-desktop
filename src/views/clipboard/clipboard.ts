import { useListError } from '@/hooks/list-error'
import { showActionError } from '@/hooks/show-action-error'
import { ClipboardErrorCode } from '@/lib/api/clipboard-error-code'
import { computed, ref, watch } from 'vue'
import { storeToRefs } from 'pinia'
import emitter from '@/plugins/eventbus'
import { useMainStore } from '@/stores/main'
import { useTempStore } from '@/stores/temp'
import { initLazyQuery, clipboardItemsGQL } from '@/lib/api/query'
import { initMutation, deleteClipboardItemsGQL, setClipboardGQL } from '@/lib/api/mutation'
import type { IClipboardItem } from '@/lib/interfaces'

export function useClipboardData() {
  const mainStore = useMainStore()
  const { app } = storeToRefs(useTempStore())

  const page = ref(1)
  const limit = computed(() => mainStore.pageSize)
  const items = ref<IClipboardItem[]>([])
  const total = ref(0)
  const { listError, setListError } = useListError(items, total)
  /** Phone-side master switch — the CLIPBOARD API permission served by the resident app query. */
  const clipboardSync = computed(() => app.value?.access?.includes('CLIPBOARD') ?? false)

  const { loading, fetch } = initLazyQuery({
    handle: (data: { clipboardItems: IClipboardItem[]; clipboardItemCount: number }, error: string) => {
      if (error === ClipboardErrorCode.ClipboardSyncDisabled) {
        items.value = []
        total.value = 0
        setListError('clipboard_disabled_on_device')
        return
      }
      setListError(error)
      if (!error && data) {
        items.value = data.clipboardItems
        total.value = data.clipboardItemCount
      }
    },
    document: clipboardItemsGQL,
    variables: () => ({
      offset: (page.value - 1) * limit.value,
      limit: limit.value,
      query: '',
    }),
  })

  function load() {
    if (clipboardSync.value) fetch()
  }

  /** Panel opened: reload when enabled; otherwise refresh the app query so a
   *  phone-side toggle is picked up. */
  function open() {
    if (clipboardSync.value) {
      fetch()
    } else {
      emitter.emit('refetch_app')
    }
  }

  load()

  const gotoPage = (p: number) => {
    page.value = p
    fetch()
  }

  const onChangePageSize = (size: number) => {
    mainStore.pageSize = size
    page.value = 1
    fetch()
  }

  const { mutate: deleteClipboardItems } = initMutation({ document: deleteClipboardItemsGQL }, showActionError)

  const deleteItem = (item: IClipboardItem) => {
    items.value = items.value.filter((it) => it.id !== item.id)
    total.value--
    deleteClipboardItems({ query: `ids:${item.id}` })
  }

  const clipText = ref('')
  const clipTextError = ref(false)

  const { mutate: mutateSetClipboard, loading: setClipLoading, onDone: onSetClipDone } = initMutation({ document: setClipboardGQL }, showActionError)
  onSetClipDone(() => { clipText.value = '' })

  function pasteClipboardText() {
    navigator.clipboard.readText().then((text) => { clipText.value = text })
  }

  function sendToPhone() {
    if (!clipText.value) { clipTextError.value = true; return }
    mutateSetClipboard({ text: clipText.value })
  }

  watch(clipText, () => { clipTextError.value = false })

  return {
    app, items, listError, total, page, limit, loading, clipboardSync,
    load, open, gotoPage, onChangePageSize, deleteItem,
    clipText, clipTextError, setClipLoading, pasteClipboardText, sendToPhone,
  }
}
