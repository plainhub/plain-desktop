import { computed, onActivated, onDeactivated, reactive, ref, watch } from 'vue'
import toast from '@/components/toaster'
import { contactsGQL, contactSourcesGQL, initLazyQuery, initQuery } from '@/lib/api/query'
import { useRoute } from 'vue-router'
import { replacePath } from '@/plugins/router'
import { useMainStore } from '@/stores/main'
import { useTempStore } from '@/stores/temp'
import { useI18n } from 'vue-i18n'
import { storeToRefs } from 'pinia'
import type { IContact, IContactSource, IFilter, IItemTagsUpdatedEvent, IItemsTagsUpdatedEvent } from '@/lib/interfaces'
import { decodeBase64 } from '@/lib/strutil'
import { useSelectable, useDelete } from '@/hooks/list'
import { useSearch } from '@/hooks/search'
import { useTags } from '@/hooks/tags'
import emitter from '@/plugins/eventbus'
import { deleteContactsGQL } from '@/lib/api/mutation'
import { DataType } from '@/lib/data'
import { useKeyEvents } from '@/hooks/key-events'

export function useContactsData() {
  const mainStore = useMainStore()
  const { app } = storeToRefs(useTempStore())
  const { t } = useI18n()
  const { parseQ } = useSearch()
  const filter = reactive<IFilter>({ tagIds: [] })
  const dataType = DataType.CONTACT
  const route = useRoute()
  const page = ref(1)
  const sources = ref<IContactSource[]>([])
  const limit = computed(() => mainStore.pageSize)
  const q = ref('')
  const items = ref<IContact[]>([])
  const isActive = ref(false)
  const { tags, fetch: fetchTags } = useTags(dataType)

  const selectable = useSelectable(items)

  const { deleteItems, confirmingDelete, deleteCount, deleteLoading, doDeleteItems, cancelDeleteItems } = useDelete(deleteContactsGQL, () => {
    selectable.clearSelection()
    fetch()
    emitter.emit('refetch_tags', dataType)
  })

  const gotoPage = (p: number) => {
    const qVal = route.query.q
    replacePath(mainStore, qVal ? `/contacts?page=${p}&q=${qVal}` : `/contacts?page=${p}`)
  }

  function onChangePageSize(size: number) {
    mainStore.pageSize = size
    const qVal = route.query.q
    replacePath(mainStore, qVal ? `/contacts?page=1&q=${qVal}` : `/contacts?page=1`)
  }

  const { keyDown: pageKeyDown, keyUp: pageKeyUp } = useKeyEvents(
    selectable.total, limit, page, selectable.selectAll, selectable.clearSelection, gotoPage,
    () => { deleteItems(selectable.selectedIds.value, selectable.realAllChecked.value, selectable.total.value, q.value) },
  )

  const { loading, fetch } = initLazyQuery({
    handle: (data: { contacts: IContact[]; contactCount: number }, error: string) => {
      if (data) {
        items.value = data.contacts
        selectable.total.value = data.contactCount
      }
    },
    document: contactsGQL,
    variables: () => ({
      offset: (page.value - 1) * limit.value,
      limit: limit.value,
      query: q.value,
    }),
  })

  initQuery({
    handle: (data: { contactSources: IContactSource[] }, error: string) => {
      if (data) {
        sources.value = data.contactSources
      }
    },
    document: contactSourcesGQL,
    variables: null,
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
    items, filter, page, limit, q, loading, fetch, tags, dataType, app, sources,
    deleteItems, confirmingDelete, deleteCount, deleteLoading, doDeleteItems, cancelDeleteItems,
    gotoPage, onChangePageSize,
    ...selectable,
  }
}
