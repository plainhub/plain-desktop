import { computed, reactive, ref, onDeactivated, onUnmounted, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { storeToRefs } from 'pinia'
import { useMainStore } from '@/stores/main'
import { type IFile, enrichFile } from '@/lib/file'
import { useSearch } from '@/hooks/files'
import { filesGQL, indexedFilesGQL } from '@/lib/api/query'
import { useRoute } from 'vue-router'
import { decodeBase64 } from '@/lib/strutil'
import type { IFileFilter } from '@/lib/interfaces'
import toast from '@/components/toaster'
import { gqlFetch, GqlError, getGraphqlContextKey } from '@/lib/api/gql-client'
import { searchFiles } from '@/lib/api/file-search'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'

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
  const rootDir = computed(() => filter.rootPath)
  const indexed = computed(() => !filter.parent && !filter.rootPath)

  function applyRouteQuery() {
    page.value = parseInt(route.query.page?.toString() ?? '1')
    q.value = decodeBase64(route.query.q?.toString() ?? '')
    parseQ(filter, q.value)
  }

  applyRouteQuery()

  const totalRef = ref(0)

  const loading = ref(false)
  const searchStatus = ref<'idle' | 'running' | 'completed' | 'incomplete'>('idle')
  let stopSearch: (() => void) | undefined
  let sequence = 0
  function cancel() {
    sequence++
    if (searchStatus.value === 'running') { searchStatus.value = 'incomplete'; settled() }
    stopSearch?.()
    stopSearch = undefined
    loading.value = false
  }
  function settled() {
    firstInit.value = false
    refreshing.value = false
    sorting.value = false
  }
  async function fetch(vars?: Record<string, any>, options?: { latest?: boolean; force?: boolean }) {
    cancel()
    const current = sequence
    searchStatus.value = 'idle'
    loading.value = true
    const limit = indexed.value || filter.text || q.value.includes('file_size:') ? 1000 : 10000
    const variables = vars ?? {
      ...(indexed.value ? {} : { root: rootDir.value }),
      offset: (page.value - 1) * limit, limit,
      query: q.value, sortBy: fileSortBy.value,
    }
    if (!indexed.value && (filter.text || q.value.includes('file_size:')) && !(filter.parent || rootDir.value).includes('!zip!/')) {
      searchStatus.value = 'running'
      items.value = []
      totalRef.value = 0
      stopSearch = searchFiles(variables as Parameters<typeof searchFiles>[0], result => {
        if (current !== sequence) return
        items.value = result.items.map(enrichFile)
        totalRef.value = Number(result.count)
        loading.value = !result.complete
        searchStatus.value = result.complete ? 'completed' : 'running'
        settled()
      }, error => {
        if (current !== sequence) return
        loading.value = false
        searchStatus.value = 'incomplete'
        settled()
        toast(t(error.message), 'error')
      })
      return
    }
    try {
      const result = await gqlFetch(indexed.value ? indexedFilesGQL : filesGQL, variables, { fresh: options?.force })
      if (current !== sequence) return
      if (result.errors?.length) throw new Error(result.errors[0].message)
      items.value = result.data.files.map(enrichFile)
      totalRef.value = Number(result.data.count ?? items.value.length)
    } catch (error) {
      if (current === sequence && !(error instanceof GqlError && error.cancelled)) toast(t(error instanceof Error ? error.message : String(error)), 'error')
    } finally {
      if (current === sequence) { loading.value = false; settled() }
    }
  }
  const stopAttached = graphqlSubscriptions.onAttached(() => { if (isActive.value) void fetch() })
  watch(isActive, active => { if (!active) cancel() })
  watch(getGraphqlContextKey, () => {
    cancel()
    items.value = []
    totalRef.value = 0
    if (isActive.value) void fetch()
  })
  onDeactivated(cancel)
  onUnmounted(() => { cancel(); stopAttached() })

  return {
    filter, q, items, loading, firstInit, refreshing, sorting,
    isActive, rootDir, indexed, fileSortBy, totalRef,
    parseQ, buildQ, applyRouteQuery, fetch, route, cancel, searchStatus,
  }
}

