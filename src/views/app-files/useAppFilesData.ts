import { useListError } from '@/hooks/list-error'
import { ref } from 'vue'
import { appFilesGQL, initLazyQuery } from '@/lib/api/query'

export interface IAppFile {
  id: string
  size: number
  mimeType: string
  fileName: string
  createdAt: string
  updatedAt: string
}

/**
 * Build the fid URI suffix for an app file.
 * Uses "{id}.{ext}" where ext is the lowercase extension of fileName,
 * falling back to bare "{id}" if fileName has no extension.
 */
export function getAppFileFid(item: IAppFile): string {
  const dot = item.fileName.lastIndexOf('.')
  if (dot === -1) return item.id
  const ext = item.fileName.substring(dot + 1).toLowerCase()
  return ext ? `${item.id}.${ext}` : item.id
}

const PAGE_SIZE = 50

export function useAppFilesData() {

  const items = ref<IAppFile[]>([])
  const total = ref(0)
  const { listError, setListError } = useListError(items, total)
  const page = ref(1)
  const noMore = ref(false)
  const firstInit = ref(true)

  const { loading, fetch } = initLazyQuery({
    handle: (data: { appFiles: IAppFile[]; appFileCount: number }, error: string) => {
      firstInit.value = false
      setListError(error)
      if (!error && data) {
        const newItems = data.appFiles ?? []
        if (page.value > 1) {
          items.value = items.value.concat(newItems)
        } else {
          items.value = newItems
        }
        total.value = data.appFileCount
        noMore.value = newItems.length < PAGE_SIZE
      }
    },
    document: appFilesGQL,
    variables: () => ({
      offset: (page.value - 1) * PAGE_SIZE,
      limit: PAGE_SIZE,
    }),
    options: { fetchPolicy: 'no-cache' },
  })

  function loadMore() {
    if (noMore.value || loading.value) return
    page.value++
    fetch()
  }

  function refresh() {
    page.value = 1
    noMore.value = false
    fetch()
  }

  return { items, listError, total, loading, firstInit, noMore, fetch, loadMore, refresh }
}
