import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { storeToRefs } from 'pinia'
import emitter from '@/plugins/eventbus'
import { useMainStore } from '@/stores/main'
import { useTempStore } from '@/stores/temp'
import { initLazyQuery, clipboardGQL } from '@/lib/api/query'
import { initMutation, cancelClipboardGQL, setClipGQL } from '@/lib/api/mutation'
import type { IClipboard } from '@/lib/interfaces'
import toast from '@/components/toaster'

export function useClipboardData() {
  const mainStore = useMainStore()
  const { app } = storeToRefs(useTempStore())
  const { t } = useI18n()

  const page = ref(1)
  const limit = computed(() => mainStore.pageSize)
  const items = ref<IClipboard[]>([])
  const total = ref(0)
  /** Phone-side master switch — the CLIPBOARD API permission served by the resident app query. */
  const clipboardSync = computed(() => app.value?.permissions?.includes('CLIPBOARD') ?? false)

  const { loading, fetch } = initLazyQuery({
    handle: (data: { clipboard: IClipboard[]; clipboardCount: number }, error: string) => {
      if (error === 'clipboard_sync_disabled') {
        // Switch turned off after load; the app query state reflects it, stay quiet.
      } else if (error) {
        toast(t(error), 'error')
      } else if (data) {
        items.value = data.clipboard
        total.value = data.clipboardCount
      }
    },
    document: clipboardGQL,
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

  const { mutate: cancelClipboard } = initMutation({ document: cancelClipboardGQL })

  const deleteItem = (item: IClipboard) => {
    items.value = items.value.filter((it) => it.id !== item.id)
    total.value--
    cancelClipboard({ ids: [item.id] })
  }

  const clipText = ref('')
  const clipTextError = ref(false)

  const { mutate: mutateSetClip, loading: setClipLoading, onDone: onSetClipDone } = initMutation({ document: setClipGQL })
  onSetClipDone(() => { clipText.value = '' })

  function pasteClipboardText() {
    navigator.clipboard.readText().then((text) => { clipText.value = text })
  }

  function sendToPhone() {
    if (!clipText.value) { clipTextError.value = true; return }
    mutateSetClip({ text: clipText.value })
  }

  watch(clipText, () => { clipTextError.value = false })

  return {
    app, items, total, page, limit, loading, clipboardSync,
    load, open, gotoPage, onChangePageSize, deleteItem,
    clipText, clipTextError, setClipLoading, pasteClipboardText, sendToPhone,
  }
}
