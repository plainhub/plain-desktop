import { ref } from 'vue'
import type { IFeedEntry, ITag } from '@/lib/interfaces'
import type { Ref, ComputedRef } from 'vue'
import { useI18n } from 'vue-i18n'
import toast from '@/components/toaster'
import { useDelete } from '@/hooks/list'
import { deleteFeedEntriesGQL, initMutation, saveFeedEntriesToNotesGQL, syncFeedsGQL, deleteFeedEntryGQL } from '@/lib/api/mutation'
import { useFeeds } from '@/hooks/feeds'
import { useMainStore } from '@/stores/main'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import { DataType } from '@/lib/data'
import router, { replacePath } from '@/plugins/router'
import emitter from '@/plugins/eventbus'

interface UseFeedEntriesActionsOptions {
  selectedIds: Ref<string[]>
  realAllChecked: Ref<boolean>
  q: Ref<string>
  total: Ref<number>
  tags: Ref<ITag[]>
  items: Ref<IFeedEntry[]>
  clearSelection: () => void
  fetch: () => void
}

export function useFeedEntriesActions(opts: UseFeedEntriesActionsOptions) {
  const { selectedIds, realAllChecked, q, total, tags, items, clearSelection, fetch } = opts
  const mainStore = useMainStore()
  const { feedsSyncing } = storeToRefs(useTempStore())
  const { t } = useI18n()
  const dataType = DataType.FEED_ENTRY

  const { viewFeed } = useFeeds(mainStore)

  const { deleteItems, confirmingDelete, deleteCount, deleteLoading, doDeleteItems, cancelDeleteItems } = useDelete(deleteFeedEntriesGQL, () => {
    clearSelection()
    fetch()
    if (items.value.some((it) => it.tags.length)) {
      emitter.emit('refetch_tags', dataType)
    }
    emitter.emit('feed_entries_deleted')
  })

  const { mutate: saveToNotes, loading: savingNotes, onDone: onSaveToNotesDone } = initMutation({
    document: saveFeedEntriesToNotesGQL,
  })

  onSaveToNotesDone(() => { toast(t('saved')) })

  const { mutate: doSyncFeeds } = initMutation({
    document: syncFeedsGQL,
  })

  function saveFeedsToNotes() {
    if (!realAllChecked.value) {
      if (selectedIds.value.length === 0) {
        toast(t('select_first'), 'error')
        return
      }
      saveToNotes({ query: `ids:${selectedIds.value.join(',')}` })
    } else {
      saveToNotes({ query: q.value })
    }
  }

  function syncFeeds() {
    feedsSyncing.value = true
    doSyncFeeds({ id: null })
  }

  const { mutate: doDeleteEntry, onDone: onDeleteEntryDone } = initMutation({ document: deleteFeedEntryGQL })
  const pendingDeleteEntry = ref<IFeedEntry | null>(null)

  onDeleteEntryDone(() => {
    const item = pendingDeleteEntry.value
    if (item) {
      items.value = items.value.filter((it) => it.id !== item.id)
      clearSelection()
      total.value--
      if (item.tags.length) emitter.emit('refetch_tags', dataType)
      pendingDeleteEntry.value = null
    }
  })

  function deleteItem(item: IFeedEntry) {
    pendingDeleteEntry.value = item
    doDeleteEntry({ query: `ids:${item.id}` })
  }


  function viewUrl(item: IFeedEntry) {
    const qVal = router.currentRoute.value.query.q
    return qVal ? `/feeds/${item.feedId}/entries/${item.id}?q=${qVal}` : `/feeds/${item.feedId}/entries/${item.id}`
  }

  function view(item: IFeedEntry) {
    replacePath(mainStore, viewUrl(item))
  }

  function backToList() {
    const qVal = router.currentRoute.value.query.q
    replacePath(mainStore, qVal ? `/feeds?q=${qVal}` : '/feeds')
  }

  return {
    deleteItems, confirmingDelete, deleteCount, deleteLoading, doDeleteItems, cancelDeleteItems, deleteItem,
    saveFeedsToNotes, savingNotes, syncFeeds,
    viewUrl, view, viewFeed, backToList,
  }
}
