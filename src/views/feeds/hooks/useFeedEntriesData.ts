import { computed, onActivated, onDeactivated, reactive, ref, watch } from 'vue'
import toast from '@/components/toaster'
import { feedsTagsGQL, initLazyQuery } from '@/lib/api/query'
import { useRoute } from 'vue-router'
import { replacePath } from '@/plugins/router'
import { useMainStore } from '@/stores/main'
import { useTempStore } from '@/stores/temp'
import { useI18n } from 'vue-i18n'
import { storeToRefs } from 'pinia'
import type { ITag, IFeedEntry, IFeed, IItemsTagsUpdatedEvent, IItemTagsUpdatedEvent, IFilter } from '@/lib/interfaces'
import { decodeBase64 } from '@/lib/strutil'
import { useSelectable } from '@/hooks/list'
import emitter from '@/plugins/eventbus'
import { DataType } from '@/lib/data'
import { useList } from '@/hooks/feed-entries'
import { useSearch } from '@/hooks/search'
import { useKeyEvents } from '@/hooks/key-events'

export function useFeedEntriesData(onDelete: () => void) {
  const mainStore = useMainStore()
  const { feedsSyncing } = storeToRefs(useTempStore())
  const { t } = useI18n()
  const { parseQ } = useSearch()
  const filter = reactive<IFilter>({ tagIds: [] })
  const dataType = DataType.FEED_ENTRY
  const route = useRoute()
  const page = ref(1)
  const limit = 100
  const tags = ref<ITag[]>([])
  const feeds = ref<IFeed[]>([])
  const feedsMap = computed(() => {
    const map: Record<string, IFeed> = {}
    feeds.value.forEach((it) => { map[it.id] = it })
    return map
  })
  const items = ref<IFeedEntry[]>([])
  const q = ref('')
  const isActive = ref(false)

  const selectable = useSelectable(items)
  const { page: listPage, loading: listLoading, loadMore, fetch: fetchList, noMore } = useList(items, q, selectable.total)

  const gotoPage = (value: number) => {
    page.value = value
    const qVal = route.query.q
    replacePath(mainStore, qVal ? `/feeds?page=${value}&q=${qVal}` : `/feeds?page=${value}`)
  }

  const { keyDown: pageKeyDown, keyUp: pageKeyUp } = useKeyEvents(
    selectable.total, limit, page, selectable.selectAll, selectable.clearSelection, gotoPage, onDelete,
  )

  const fetch = () => {
    listPage.value = 1
    fetchList()
  }

  const { fetch: fetchFeedsTags } = initLazyQuery({
    handle: async (data: { tags: ITag[]; feeds: IFeed[] }, error: string) => {
      if (error) {
        toast(t(error), 'error')
      } else if (data) {
        tags.value = data.tags
        feeds.value = data.feeds
      }
    },
    document: feedsTagsGQL,
    variables: { type: dataType },
  })

  const itemsTagsUpdatedHandler = (event: IItemsTagsUpdatedEvent) => {
    if (event.type === dataType) { fetch() }
  }
  const itemTagsUpdatedHandler = (event: IItemTagsUpdatedEvent) => {
    if (event.type === dataType) { fetch() }
  }
  const refetchTagsHandler = (type: string) => { if (type === dataType) fetchFeedsTags() }

  function applyRouteQuery() {
    const nextPage = parseInt(route.query.page?.toString() ?? '1')
    page.value = Number.isFinite(nextPage) && nextPage > 0 ? nextPage : 1
    q.value = decodeBase64(route.query.q?.toString() ?? '')
    parseQ(filter, q.value)
    fetch()
  }

  watch(() => route.fullPath, () => { if (isActive.value) applyRouteQuery() })

  const feedsFetchedHandler = (data: any) => {
    feedsSyncing.value = false
    fetch()
    fetchFeedsTags()
    if (data.error) {
      toast(data.error, 'error')
    } else {
      toast(t('feeds_synced'))
    }
  }

  onActivated(() => {
    const scroller = document.getElementsByClassName('scroller')?.[0]
    if (scroller) scroller.scrollTop = 0
    fetchFeedsTags()
    isActive.value = true
    applyRouteQuery()
    emitter.on('item_tags_updated', itemTagsUpdatedHandler)
    emitter.on('items_tags_updated', itemsTagsUpdatedHandler)
    emitter.on('refetch_tags', refetchTagsHandler)
    emitter.on('feeds_fetched', feedsFetchedHandler)
    window.addEventListener('keydown', pageKeyDown)
    window.addEventListener('keyup', pageKeyUp)
  })

  onDeactivated(() => {
    isActive.value = false
    listPage.value = 1
    noMore.value = false
    emitter.off('item_tags_updated', itemTagsUpdatedHandler)
    emitter.off('items_tags_updated', itemsTagsUpdatedHandler)
    emitter.off('refetch_tags', refetchTagsHandler)
    emitter.off('feeds_fetched', feedsFetchedHandler)
    window.removeEventListener('keydown', pageKeyDown)
    window.removeEventListener('keyup', pageKeyUp)
  })

  return {
    items, filter, page, limit, q, tags, feeds, feedsMap, feedsSyncing,
    listLoading, loadMore, noMore, fetch, dataType, route,
    ...selectable,
  }
}
