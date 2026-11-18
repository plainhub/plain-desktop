import { computed, ref, watch } from 'vue'
import { useMainStore } from '@/stores/main'
import { getLoginPeerRevision } from '@/lib/device/login-peers'
import { bindGraphqlClient } from '@/lib/api/gql-client'
import { gqlFetchPeer } from '@/lib/api/peer-client'
import { clipboardItemsGQL } from '@/lib/api/query'
import { deleteClipboardItemsGQL } from '@/lib/api/mutation'
import { ClipboardErrorCode } from '@/lib/api/clipboard-error-code'
import { clipboardPeers, findClipboardPeer, onClipboardHistoryChanged } from '@/lib/peer/local-clipboard-data'
import type { IClipboardItem } from '@/lib/interfaces'

export function useLocalClipboard() {
  const store = useMainStore()
  const peerId = ref('')
  const limit = computed(() => store.pageSize)
  const page = ref(1)
  const items = ref<IClipboardItem[]>([])
  const total = ref(0)
  const status = ref<'loading' | 'ready' | 'disabled' | 'offline' | 'error'>('loading')
  let refresh: (() => Promise<void>) | undefined

  watch(clipboardPeers, peers => {
    if (peerId.value && !peers.some(peer => peer.id === peerId.value)) peerId.value = ''
  })

  watch([peerId, () => store.quick === 'clipboard', () => getLoginPeerRevision(peerId.value)], ([id, open], _, onCleanup) => {
    refresh = undefined
    if (!open) return
    const peer = id ? findClipboardPeer(id) : undefined
    const request = peer ? <T>(doc: string, vars: Record<string, unknown>) => gqlFetchPeer<T>(peer, doc, vars)
      : bindGraphqlClient()
    let active = true
    let version = 0
    const load = async () => {
      const revision = ++version
      const result = await request<{ clipboardItems: IClipboardItem[]; clipboardItemCount: number }>(
        clipboardItemsGQL, { offset: (page.value - 1) * limit.value, limit: limit.value, query: '' },
      ).catch(() => undefined)
      if (!active || revision !== version || (id && !findClipboardPeer(id))) return
      if (!result || result.errors?.length) {
        const error = result?.errors?.[0]
        status.value = error?.extensions?.code === ClipboardErrorCode.ClipboardSyncDisabled ||
          error?.message === ClipboardErrorCode.ClipboardSyncDisabled ? 'disabled' : !result && peer ? 'offline' : 'error'
        items.value = []
        total.value = 0
        return
      }
      status.value = 'ready'
      items.value = result.data.clipboardItems
      total.value = result.data.clipboardItemCount
    }
    onCleanup(() => { active = false })
    items.value = []
    total.value = 0
    page.value = 1
    if (id && !peer) {
      status.value = 'offline'
      return
    }
    status.value = 'loading'
    refresh = load
    onCleanup(onClipboardHistoryChanged(id, load))
    void load()
  }, { immediate: true })

  function gotoPage(value: number) {
    page.value = value
    void refresh?.()
  }

  function changePageSize(value: number) {
    store.pageSize = value
    gotoPage(1)
  }

  async function deleteItem(item: IClipboardItem) {
    const id = peerId.value
    const peer = id ? findClipboardPeer(id) : undefined
    if (id && !peer) throw new Error('Clipboard device is no longer available')
    const vars = { query: `ids:${item.id}` }
    const result = peer ? await gqlFetchPeer(peer, deleteClipboardItemsGQL, vars)
      : await bindGraphqlClient()(deleteClipboardItemsGQL, vars)
    if (result.errors?.length || !result.data?.deleteClipboardItems) throw new Error('Clipboard item could not be deleted')
    if (id === peerId.value && refresh) {
      if (page.value > 1 && items.value.length === 1) page.value--
      await refresh()
    }
  }

  return { peerId, clipboardPeers, items, total, status, page, limit, gotoPage, changePageSize, deleteItem }
}
