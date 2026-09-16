import { computed, onActivated, onDeactivated, ref, watch } from 'vue'
import toast from '@/components/toaster'
import { packagesGQL, initLazyQuery, packageStatusesGQL } from '@/lib/api/query'
import { useRoute } from 'vue-router'
import { replacePath } from '@/plugins/router'
import { useMainStore } from '@/stores/main'
import { useTempStore, type IUploadItem } from '@/stores/temp'
import { useI18n } from 'vue-i18n'
import { storeToRefs } from 'pinia'
import type { IPackageItem, IPackage, IPackageStatus } from '@/lib/interfaces'
import { decodeBase64 } from '@/lib/strutil'
import { useSelectable } from '@/hooks/list'
import { useKeyEvents } from '@/hooks/key-events'
import emitter from '@/plugins/eventbus'
import { getFileUrlByPath } from '@/lib/api/file'

export function useAppsData() {
  const mainStore = useMainStore()
  const { app, urlTokenKey } = storeToRefs(useTempStore())
  const { t } = useI18n()
  const { appSortBy } = storeToRefs(mainStore)
  const route = useRoute()
  const page = ref(parseInt(route.query.page?.toString() ?? '1'))
  const limit = computed(() => mainStore.pageSize)
  const q = ref('')
  const items = ref<IPackageItem[]>([])
  const isActive = ref(false)
  const sorting = ref(false)

  const selectable = useSelectable(items)

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
      if (error) {
        toast(t(error), 'error')
      } else if (data) {
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
    if (!isActive.value) return
    page.value = parseInt(route.query.page?.toString() ?? '1')
    q.value = decodeBase64(route.query.q?.toString() ?? '')
    fetch()
  }

  watch(() => route.fullPath, () => { applyRouteQuery() })

  const permissionsUpdatedHandler = () => { if (isActive.value) fetch() }
  onActivated(() => emitter.on('permissions_updated', permissionsUpdatedHandler))
  onDeactivated(() => emitter.off('permissions_updated', permissionsUpdatedHandler))

  return {
    items, page, limit, q, loading, fetch, sorting, isActive,
    app, urlTokenKey, appSortBy, route,
    gotoPage, onChangePageSize, sort, applyRouteQuery,
    pageKeyDown, pageKeyUp,
    ...selectable,
  }
}
