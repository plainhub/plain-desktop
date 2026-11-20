import { useListError } from '@/hooks/list-error'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'
import { computed, onActivated, onDeactivated, ref, watch } from 'vue'
import { packagesGQL, initLazyQuery, packageStatusesGQL } from '@/lib/api/query'
import { useRoute } from 'vue-router'
import { replacePath } from '@/plugins/router'
import { useMainStore } from '@/stores/main'
import { useTempStore, type IUploadItem } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import type { IPackageItem, IPackage, IPackageStatus } from '@/lib/interfaces'
import { decodeBase64 } from '@/lib/strutil'
import { useSelectable } from '@/hooks/list'
import { useKeyEvents } from '@/hooks/key-events'
import { getFileUrlByPath } from '@/lib/api/file'

export function useAppsData() {
  const mainStore = useMainStore()
  const { app, urlTokenKey } = storeToRefs(useTempStore())
  const { appSortBy } = storeToRefs(mainStore)
  const route = useRoute()
  const page = ref(parseInt(route.query.page?.toString() ?? '1'))
  const limit = computed(() => mainStore.pageSize)
  const q = ref('')
  const items = ref<IPackageItem[]>([])
  const isActive = ref(false)
  const sorting = ref(false)

  const selectable = useSelectable(items)
  const { listError, setListError } = useListError(items, selectable.total)

  const gotoPage = (p: number) => {
    const qVal = route.query.q
    replacePath(mainStore, qVal ? `/apps?page=${p}&q=${qVal}` : `/apps?page=${p}`)
  }

  function onChangePageSize(size: number) {
    mainStore.pageSize = size
    const qVal = route.query.q
    replacePath(mainStore, qVal ? `/apps?page=1&q=${qVal}` : `/apps?page=1`)
  }

  const { keyDown: pageKeyDown, keyUp: pageKeyUp } = useKeyEvents(
    selectable.total, limit, page, selectable.selectAll, selectable.clearSelection, gotoPage, () => {},
  )

  const { loading, fetch } = initLazyQuery({
    handle: (data: { packages: IPackage[]; packageCount: number }, error: string) => {
      sorting.value = false
      setListError(error)
      if (!error && data) {
        items.value = data.packages.map((it: IPackage) => ({
          ...it,
          isUninstalling: false,
          icon: getFileUrlByPath(urlTokenKey.value, 'pkgicon://' + it.id),
        }))
        selectable.total.value = data.packageCount
      }
    },
    document: packagesGQL,
    variables: () => ({
      offset: (page.value - 1) * limit.value,
      limit: limit.value,
      query: q.value,
      sortBy: appSortBy.value,
    }),
  })

  function sort(value: string) {
    if (appSortBy.value === value) return
    sorting.value = true
    appSortBy.value = value
    gotoPage(1)
  }

  function applyRouteQuery() {
    if (route.path !== '/apps' && !route.path.startsWith('/apps/')) return
    if (!isActive.value) return
    page.value = parseInt(route.query.page?.toString() ?? '1')
    q.value = decodeBase64(route.query.q?.toString() ?? '')
    fetch()
  }

  watch(() => route.fullPath, () => { applyRouteQuery() })

  const accessUpdatedHandler = () => { if (isActive.value) fetch() }
  let stopPermissionsUpdated: (() => void) | undefined

  onActivated(() => stopPermissionsUpdated = graphqlSubscriptions.subscribe('accessUpdated', accessUpdatedHandler))
  onDeactivated(() => stopPermissionsUpdated?.())

  return {
    items, listError, page, limit, q, loading, fetch, sorting, isActive,
    app, urlTokenKey, appSortBy, route,
    gotoPage, onChangePageSize, sort, applyRouteQuery,
    pageKeyDown, pageKeyUp,
    ...selectable,
  }
}
