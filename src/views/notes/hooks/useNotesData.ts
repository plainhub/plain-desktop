import { computed, onActivated, onDeactivated, reactive, ref, watch } from 'vue'
import toast from '@/components/toaster'
import { notesGQL, initLazyQuery } from '@/lib/api/query'
import { useRoute } from 'vue-router'
import { replacePath } from '@/plugins/router'
import { useMainStore } from '@/stores/main'
import { useI18n } from 'vue-i18n'
import type { INote, IItemTagsUpdatedEvent, IItemsTagsUpdatedEvent, INotesActionedEvent, IFilter } from '@/lib/interfaces'
import { decodeBase64 } from '@/lib/strutil'
import { useSelectable } from '@/hooks/list'
import emitter from '@/plugins/eventbus'
import { useTags } from '@/hooks/tags'
import { DataType } from '@/lib/data'
import { useSearch } from '@/hooks/search'
import { useKeyEvents } from '@/hooks/key-events'

export function useNotesData(onDelete: () => void) {
  const mainStore = useMainStore()
  const { t } = useI18n()
  const { parseQ } = useSearch()
  const filter = reactive<IFilter>({ tagIds: [], trash: false })
  const dataType = DataType.NOTE
  const route = useRoute()
  const page = ref(1)
  const limit = computed(() => mainStore.pageSize)
  const q = ref('')
  const items = ref<INote[]>([])
  const isActive = ref(false)
  const { tags, fetch: fetchTags } = useTags(dataType)

  const selectable = useSelectable(items)

  const gotoPage = (p: number) => {
    const qVal = route.query.q
    replacePath(mainStore, qVal ? `/notes?page=${p}&q=${qVal}` : `/notes?page=${p}`)
  }

  function onChangePageSize(size: number) {
    mainStore.pageSize = size
    const qVal = route.query.q
    replacePath(mainStore, qVal ? `/notes?page=1&q=${qVal}` : `/notes?page=1`)
  }

  const { keyDown: listKeyDown, keyUp: pageKeyUp } = useKeyEvents(
    selectable.total, limit, page, selectable.selectAll, selectable.clearSelection, gotoPage, onDelete,
  )

  const isDetail = computed(() => !!route.params.id)
  const pageKeyDown = (e: KeyboardEvent) => {
    if (isDetail.value) return
    listKeyDown(e)
  }

  const { loading, fetch } = initLazyQuery({
    handle: (data: { notes: INote[]; noteCount: number }, error: string) => {
      if (error) {
        toast(t(error), 'error')
      } else if (data) {
        items.value = data.notes
        selectable.total.value = data.noteCount
      }
    },
    document: notesGQL,
    variables: () => ({
      offset: (page.value - 1) * limit.value,
      limit: limit.value,
      query: q.value,
    }),
  })

  function applyRouteQuery() {
    const nextPage = parseInt(route.query.page?.toString() ?? '1')
    page.value = Number.isFinite(nextPage) && nextPage > 0 ? nextPage : 1
    q.value = decodeBase64(route.query.q?.toString() ?? '')
    parseQ(filter, q.value)
    if (filter.trash === undefined) {
      filter.trash = false
    }
    fetch()
  }

  const itemsTagsUpdatedHandler = (event: IItemsTagsUpdatedEvent) => {
    if (event.type === dataType) { fetch() }
  }
  const itemTagsUpdatedHandler = (event: IItemTagsUpdatedEvent) => {
    if (event.type === dataType) { fetch() }
  }
  const refetchTagsHandler = (type: string) => { if (type === dataType) fetchTags() }
  const notesActionedHandler = (event: INotesActionedEvent) => {
    const note = event.note
    if (!note) {
      fetch()
      return
    }
    const idx = items.value.findIndex((it) => it.id === note.id)
    if (idx >= 0) {
      items.value[idx] = { ...items.value[idx], ...note }
    } else {
      fetch()
    }
  }

  watch(() => route.fullPath, () => { if (isActive.value) applyRouteQuery() })

  onActivated(() => {
    fetchTags()
    isActive.value = true
    applyRouteQuery()
    emitter.on('item_tags_updated', itemTagsUpdatedHandler)
    emitter.on('items_tags_updated', itemsTagsUpdatedHandler)
    emitter.on('refetch_tags', refetchTagsHandler)
    emitter.on('notes_actioned', notesActionedHandler)
    window.addEventListener('keyup', pageKeyUp)
  })

  onDeactivated(() => {
    isActive.value = false
    emitter.off('item_tags_updated', itemTagsUpdatedHandler)
    emitter.off('items_tags_updated', itemsTagsUpdatedHandler)
    emitter.off('refetch_tags', refetchTagsHandler)
    emitter.off('notes_actioned', notesActionedHandler)
    window.removeEventListener('keyup', pageKeyUp)
  })

  return {
    items, filter, page, limit, q, loading, fetch, tags, dataType, route,
    gotoPage, onChangePageSize, ...selectable,
  }
}
