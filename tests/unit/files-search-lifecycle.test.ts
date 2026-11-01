import { afterEach, expect, it, vi } from 'vitest'
import { createRenderer, nextTick } from 'vue'

const state = vi.hoisted(() => ({ streams: [] as any[], context: undefined as any }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('pinia', () => ({ storeToRefs: (store: unknown) => store }))
vi.mock('@/stores/main', async () => {
  const { ref } = await import('vue')
  return { useMainStore: () => ({ fileSortBy: ref('NAME_ASC') }) }
})
vi.mock('vue-router', () => ({ useRoute: () => ({ query: { q: 'needle' } }) }))
vi.mock('@/lib/strutil', () => ({ decodeBase64: (value: string) => value }))
vi.mock('@/hooks/files', () => ({ useSearch: () => ({ parseQ: (filter: any, text: string) => Object.assign(filter, { parent: '/documents', rootPath: '/documents', text }), buildQ: vi.fn() }) }))
vi.mock('@/lib/api/query', () => ({ filesGQL: 'files', indexedFilesGQL: 'indexedFiles' }))
vi.mock('@/lib/api/gql-client', async () => {
  const { ref } = await import('vue')
  state.context = ref('first')
  return { gqlFetch: vi.fn(), GqlError: class extends Error {}, getGraphqlContextKey: () => state.context.value }
})
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: { onAttached: () => () => {} } }))
vi.mock('@/lib/api/file-search', () => ({ searchFiles: (variables: unknown, next: unknown, failed: unknown) => {
  const cancel = vi.fn()
  state.streams.push({ variables, next, failed, cancel })
  return cancel
} }))
vi.mock('@/lib/file', () => ({ enrichFile: (file: any) => ({ ...file, id: file.path }) }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
import { useFilesData } from '@/views/files/hooks/useFilesData'

let unmount: (() => void) | undefined
afterEach(() => { unmount?.(); state.streams = []; state.context.value = 'first' })
function mount() {
  const renderer = createRenderer<any, any>({
    insert: vi.fn(), remove: vi.fn(), createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
    setText: vi.fn(), setElementText: vi.fn(), parentNode: () => null, nextSibling: () => null, patchProp: vi.fn(),
  })
  let data!: ReturnType<typeof useFilesData>
  const app = renderer.createApp({ setup() { data = useFilesData(); return () => null } })
  app.mount({})
  unmount = () => app.unmount()
  return data
}
const page = { items: [{ path: '/documents/needle.txt' }], count: 1, complete: false }

it('cancels traversal, settles refresh controls and rejects late results', async () => {
  const data = mount()
  data.refreshing.value = true
  await data.fetch()
  const stream = state.streams[0]
  expect(data.loading.value).toBe(true)
  data.cancel()
  expect(stream.cancel).toHaveBeenCalledOnce()
  expect(data.loading.value).toBe(false)
  expect(data.refreshing.value).toBe(false)
  expect(data.firstInit.value).toBe(false)
  expect(data.searchStatus.value).toBe('incomplete')
  stream.next(page)
  expect(data.items.value).toEqual([])
})

it('closes the previous search and accepts only the latest query results', async () => {
  const data = mount()
  await data.fetch()
  const previous = state.streams[0]
  data.q.value = 'other'
  await data.fetch()
  expect(previous.cancel).toHaveBeenCalledOnce()
  previous.next(page)
  expect(data.items.value).toEqual([])
  state.streams[1].next({ ...page, complete: true })
  expect(data.items.value[0].path).toBe('/documents/needle.txt')
  expect(data.loading.value).toBe(false)
  expect(data.searchStatus.value).toBe('completed')
})

it('cancels and clears the old device result before fetching in the new context', async () => {
  const data = mount()
  data.isActive.value = true
  await data.fetch()
  const previous = state.streams[0]
  previous.next(page)
  expect(data.items.value).toHaveLength(1)
  state.context.value = 'second'
  await nextTick()
  expect(previous.cancel).toHaveBeenCalledOnce()
  expect(data.items.value).toEqual([])
  expect(state.streams).toHaveLength(2)
})
