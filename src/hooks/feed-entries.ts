import { useListError } from '@/hooks/list-error'
import { feedEntriesGQL, initLazyQuery } from '@/lib/api/query'
import type { IFeedEntry } from '@/lib/interfaces'
import { ref, type Ref } from 'vue'

export const useList = (items: Ref<IFeedEntry[]>, q: Ref<string>, total: Ref<number>) => {
  const { listError, setListError } = useListError(items, total)

  const page = ref(1)
  const limit = 100
  const noMore = ref(false)
  const { loading, fetch } = initLazyQuery({
    handle: (data: { items: IFeedEntry[]; total: number }, error: string) => {
      setListError(error)
      if (!error && data) {
        if (data.items.length < limit) {
          noMore.value = true
        }
        const newItems = data.items
        if (page.value === 1) {
          items.value = newItems
        } else {
          items.value = items.value.concat(newItems)
        }
        total.value = data.total
      }
    },
    document: feedEntriesGQL,
    variables: () => ({
      offset: (page.value - 1) * limit,
      limit,
      query: q.value,
    }),
  })

  return {
    listError, page,
    noMore,
    loading,
    fetch,
    loadMore: () => {
      if (noMore.value || loading.value) {
        return
      }
      page.value++
    },
  }
}
