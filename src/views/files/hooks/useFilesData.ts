import { computed, reactive, ref, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { storeToRefs } from 'pinia'
import { useMainStore } from '@/stores/main'
import { type IFile, enrichFile } from '@/lib/file'
import { useSearch } from '@/hooks/files'
import { filesGQL, initLazyQuery } from '@/lib/api/query'
import { useRoute } from 'vue-router'
import { decodeBase64 } from '@/lib/strutil'
import type { IFileFilter } from '@/lib/interfaces'
import toast from '@/components/toaster'

export function useFilesData() {
  const { t } = useI18n()
  const mainStore = useMainStore()
  const { fileSortBy } = storeToRefs(mainStore)
  const route = useRoute()
  const { parseQ, buildQ } = useSearch()

  const filter = reactive<IFileFilter>({
    type: '',
    rootPath: '',
    showHidden: false,
    text: '',
    parent: '',
  })

  const q = ref('')
  const items = ref<IFile[]>([])
  const refreshing = ref(false)
  const sorting = ref(false)
  const firstInit = ref(true)
  const isActive = ref(false)
  const page = ref(1)
  const limit = 10000
  const rootDir = computed(() => filter.rootPath)

  function applyRouteQuery() {
    page.value = parseInt(route.query.page?.toString() ?? '1')
    q.value = decodeBase64(route.query.q?.toString() ?? '')
    parseQ(filter, q.value)
  }

  applyRouteQuery()

  const totalRef = ref(0)

  const { loading, fetch } = initLazyQuery({
    handle: async (data: any, error: string) => {
      firstInit.value = false
      refreshing.value = false
      sorting.value = false
      if (error) {
        toast(t(error), 'error')
      } else {
        const dirs = mainStore.excludedDirs
        const list: IFile[] = []
        for (const item of data.files) {
          const f = enrichFile(item)
          if (dirs.length && dirs.some((d) => f.path.startsWith(d))) continue
          list.push(f)
        }
        items.value = list
        totalRef.value = list.length
      }
    },
    document: filesGQL,
    variables: () => ({
      root: rootDir.value,
      offset: (page.value - 1) * limit,
      limit,
      query: q.value,
      sortBy: fileSortBy.value,
    }),
    options: { fetchPolicy: 'cache-and-network' },
  })

  return {
    filter, q, items, loading, firstInit, refreshing, sorting,
    isActive, rootDir, fileSortBy, totalRef,
    parseQ, buildQ, applyRouteQuery, fetch, route,
  }
}

