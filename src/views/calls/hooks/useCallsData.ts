import { computed, onActivated, onDeactivated, reactive, ref, watch } from 'vue'
import toast from '@/components/toaster'
import { callsGQL, initLazyQuery } from '@/lib/api/query'
import { useRoute } from 'vue-router'
import { replacePath } from '@/plugins/router'
import { useMainStore } from '@/stores/main'
import { useTempStore } from '@/stores/temp'
import { useI18n } from 'vue-i18n'
import { storeToRefs } from 'pinia'
import type { ICall, IFilter, IItemTagsUpdatedEvent, IItemsTagsUpdatedEvent } from '@/lib/interfaces'
import { decodeBase64 } from '@/lib/strutil'
import { useSelectable, useDelete } from '@/hooks/list'
import { useSearch } from '@/hooks/search'
import { useTags } from '@/hooks/tags'
import emitter from '@/plugins/eventbus'
import { deleteCallsGQL } from '@/lib/api/mutation'
import { DataType } from '@/lib/data'
import { useKeyEvents } from '@/hooks/key-events'

export function useCallsData() {
  const mainStore = useMainStore()
  const { app } = storeToRefs(useTempStore())
  const { t } = useI18n()
  const { parseQ } = useSearch()
  const filter = reactive<IFilter>({ tagIds: [] })
  const dataType = DataType.CALL
  const route = useRoute()
  const page = ref(1)
  const limit = computed(() => mainStore.pageSize)
  const q = ref('')
  const items = ref<ICall[]>([])
  const isActive = ref(false)
  const { tags, fetch: fetchTags } = useTags(dataType)

  const selectable = useSelectable(items)

  const { deleteItems, confirmingDelete, deleteCount, deleteLoading, doDeleteItems, cancelDeleteItems } = useDelete(deleteCallsGQL, () => {
    selectable.clearSelection()
    fetch()
    emitter.emit('refetch_tags', dataType)
    emitter.emit('calls_deleted')
  })

  const gotoPage = (p: number) => {
    const qVal = route.query.q
    replacePath(mainStore, qVal ? `/calls?page=${p}&q=${qVal}` : `/calls?page=${p}`)
  }

  function onChangePageSize(size: number) {
    mainStore.pageSize = size
    const qVal = route.query.q
    replacePath(mainStore, qVal ? `/calls?page=1&q=${qVal}` : `/calls?page=1`)
  }

  const { keyDown: pageKeyDown, keyUp: pageKeyUp } = useKeyEvents(
    selectable.total, limit, page, selectable.selectAll, selectable.clearSelection, gotoPage,
    () => { deleteItems(selectable.selectedIds.value, selectable.realAllChecked.value, selectable.total.value, q.value) },
  )

  const { loading, fetch } = initLazyQuery({
    handle: (data: { calls: ICall[]; callCount: number }, error: string) => {
      if (error) {
        toast(t(error), 'error')
      } else if (data) {
        items.value = data.calls
        selectable.total.value = data.callCount
      }
    },
    document: callsGQL,
    variables: () => ({
      offset: (page.value - 1) * limit.value,
      limit: limit.value,
      query: q.value,
    }),
  })

  const itemsTagsUpdatedHandler = (event: IItemsTagsUpdatedEvent) => {
    if (event.type === dataType) { fetch() }
  }
  const itemTagsUpdatedHandler = (event: IItemTagsUpdatedEvent) => {
    if (event.type === dataType) { fetch() }
  }
  const refetchTagsHandler = (type: string) => { if (type === dataType) fetchTags() }
  const permissionsUpdatedHandler = () => fetch()

  function applyRouteQuery() {
    const nextPage = parseInt(route.query.page?.toString() ?? '1')
    page.value = Number.isFinite(nextPage) && nextPage > 0 ? nextPage : 1
    q.value = decodeBase64(route.query.q?.toString() ?? '')
    parseQ(filter, q.value)
    fetch()
  }

  watch(() => route.fullPath, () => { if (isActive.value) applyRouteQuery() })

  onActivated(() => {
    fetchTags()
    isActive.value = true
    applyRouteQuery()
    emitter.on('item_tags_updated', itemTagsUpdatedHandler)
    emitter.on('items_tags_updated', itemsTagsUpdatedHandler)
    emitter.on('refetch_tags', refetchTagsHandler)
    emitter.on('permissions_updated', permissionsUpdatedHandler)
    window.addEventListener('keydown', pageKeyDown)
    window.addEventListener('keyup', pageKeyUp)
  })

  onDeactivated(() => {
    isActive.value = false
    emitter.off('item_tags_updated', itemTagsUpdatedHandler)
    emitter.off('items_tags_updated', itemsTagsUpdatedHandler)
    emitter.off('refetch_tags', refetchTagsHandler)
    emitter.off('permissions_updated', permissionsUpdatedHandler)
    window.removeEventListener('keydown', pageKeyDown)
    window.removeEventListener('keyup', pageKeyUp)
  })

  return {
    items, filter, page, limit, q, loading, fetch, tags, dataType, app,
    deleteItems, confirmingDelete, deleteCount, deleteLoading, doDeleteItems, cancelDeleteItems,
    gotoPage, onChangePageSize,
    ...selectable,
  }
}
