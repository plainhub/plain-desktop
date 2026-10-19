import { afterEach, expect, it, vi } from 'vitest'
import { createRenderer, h, KeepAlive, nextTick, ref } from 'vue'
import { useFilesEvents } from '@/views/files/hooks/useFilesEvents'

afterEach(() => vi.unstubAllGlobals())

it('does not replace the file filter or fetch when navigation leaves the file page before deactivation', async () => {
  vi.stubGlobal('window', { addEventListener: vi.fn(), removeEventListener: vi.fn() })
  const renderer = createRenderer<any, any>({
    insert: vi.fn(), remove: vi.fn(), createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
    setText: vi.fn(), setElementText: vi.fn(), parentNode: () => null, nextSibling: () => null, patchProp: vi.fn(),
  })
  const route = ref<string | null>('/files?q=owned')
  const sort = ref('DATE_DESC')
  const applyRouteQuery = vi.fn()
  const fetch = vi.fn()
  const FilePage = {
    setup() {
      useFilesEvents({
        isActive: ref(false), fileSortBy: sort, routeFullPath: () => route.value, applyRouteQuery, fetch,
        refetchMounts: vi.fn(), refreshing: ref(false), sorting: ref(false), onDeleted: vi.fn(),
        pageKeyDown: vi.fn(), pageKeyUp: vi.fn(), parentDir: () => '/owned',
      })
      return () => null
    },
  }
  const app = renderer.createApp({ render: () => h(KeepAlive, null, () => h(FilePage)) })
  app.mount({})
  try {
    expect(fetch).toHaveBeenCalledTimes(1)
    fetch.mockClear()
    applyRouteQuery.mockClear()
    route.value = null
    sort.value = 'NAME_ASC'
    await nextTick()
    expect(applyRouteQuery).not.toHaveBeenCalled()
    expect(fetch).not.toHaveBeenCalled()
    route.value = '/files?q=another-owned-directory'
    await nextTick()
    expect(applyRouteQuery).toHaveBeenCalledTimes(1)
    expect(fetch).toHaveBeenCalledTimes(1)
  } finally {
    app.unmount()
  }
})
