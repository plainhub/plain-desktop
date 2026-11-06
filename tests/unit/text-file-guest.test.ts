import { afterEach, expect, it, vi } from 'vitest'
import { effectScope, reactive, ref } from 'vue'
import { resourceId } from '@/lib/api/resource-link'
import { runMutation } from '@/lib/api/mutation'

vi.hoisted(() => vi.stubGlobal('__IS_TAURI__', false))
const io = vi.hoisted(() => ({ route: { name: 'text-file', query: {} as Record<string, string> }, fetch: vi.fn(), token: '', cached: false }))
vi.mock('vue', async original => ({ ...await original<typeof import('vue')>(), onMounted: vi.fn(), onBeforeUnmount: vi.fn() }))
vi.mock('vue-router', () => ({ useRoute: () => io.route, useRouter: () => ({ push: vi.fn() }) }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('pinia', () => ({ storeToRefs: (value: unknown) => value }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ app: ref(null), urlTokenKey: ref(io.cached ? new Uint8Array(32) : null) }) }))
vi.mock('@/hooks/markdown', () => ({ useMarkdown: () => ({ render: async (value: string) => value }), useSafeMarkdown: () => ({ render: async (value: string) => '<safe>' + value }) }))
vi.mock('@/lib/native-file-actions', () => ({ revealLocalFile: vi.fn(), saveTextFileAs: vi.fn() }))
vi.mock('@/lib/api/api', () => ({ getProxyUrl: (value: string) => value }))
vi.mock('@/lib/api/mutation', () => ({ initMutation: () => ({ mutate: vi.fn(), loading: ref(false) }), runMutation: vi.fn(), writeTextFileGQL: 'write' }))
vi.mock('@/lib/api/query', () => ({ appGQL: 'app' }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: io.fetch }))
vi.mock('@/lib/api/file', () => ({ download: vi.fn(), getFileUrl: (id: string, _query = '', action = 'preview') => '/fs?id=' + encodeURIComponent(id) + '&action=' + action, tokenToKey: (value: string) => Uint8Array.from(atob(value), char => char.charCodeAt(0)) }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: () => io.token }))
import { useTextFile } from '@/views/text-file/text-file'

afterEach(() => vi.unstubAllGlobals())

it.each([{ guest: true, cached: false }, { guest: false, cached: false }, { guest: true, cached: true }])('loads a text resource with the required authorization only ($guest, cached=$cached)', async ({ guest, cached }) => {
  io.cached = cached
  const id = resourceId(new Uint8Array(32), { mode: 'c', id: 'owner-session' }, { type: 'FILE', id: '/owned/note.txt' })
  io.route = reactive({ name: 'text-file', query: guest ? { id: 'scoped-file', sid: 'owned-share' } : { id } })
  io.token = guest && !cached ? '' : 'owner-session'
  io.fetch.mockReset().mockResolvedValue({ data: { app: { urlToken: btoa(String.fromCharCode(...new Uint8Array(32))) } } })
  const fetch = vi.fn().mockResolvedValue(new Response('Shared text 中文\n', { headers: { 'content-disposition': 'inline; filename="note.txt"' } }))
  vi.stubGlobal('fetch', fetch)
  vi.stubGlobal('document', { title: '' })
  const scope = effectScope()
  try {
    const text = scope.run(() => useTextFile())!
    await vi.waitFor(() => expect(text.loading.value).toBe(false))
    expect(text.content.value).toBe('Shared text 中文\n')
    expect(text.canEdit.value).toBe(!guest)
    expect(fetch).toHaveBeenCalledExactlyOnceWith(guest ? '/fs?id=scoped-file&action=preview' : '/fs?id=' + encodeURIComponent(id) + '&action=preview')
    expect(io.fetch).toHaveBeenCalledTimes(guest ? 0 : 1)
    vi.mocked(runMutation).mockReset().mockResolvedValue(true)
    io.route.name = 'text-edit'
    vi.stubGlobal('window', { setTimeout: vi.fn().mockReturnValue(0), clearTimeout: vi.fn() })
    text.draft.value = 'Edited source 中文\n'
    await text.save()
    if (guest) expect(runMutation).not.toHaveBeenCalled()
    else expect(runMutation).toHaveBeenCalledExactlyOnceWith(expect.any(Function), { path: '/owned/note.txt', content: 'Edited source 中文\n', overwrite: true })
  } finally { scope.stop() }
})

it.each([false, true])('renders document Markdown and downloads original bytes (guest=%s)', async (guest) => {
  io.cached = false
  io.token = guest ? '' : 'owner-session'
  const id = resourceId(new Uint8Array(32), { mode: 'c', id: 'owner-session' }, { type: 'FILE', id: '/owned/report.docx' })
  io.route = reactive({ name: 'text-file', query: guest ? { id: 'scoped-file', sid: 'owned-share' } : { id } })
  io.fetch.mockReset().mockResolvedValue({ data: { app: { urlToken: btoa(String.fromCharCode(...new Uint8Array(32))) } } })
  const headers = { 'content-disposition': 'inline; filename="report.docx"' }
  const fetch = vi.fn()
  fetch.mockResolvedValueOnce(new Response('# Document preview', { headers: { ...headers, 'content-type': 'text/markdown; charset=utf-8' } }))
  vi.stubGlobal('fetch', fetch)
  vi.stubGlobal('document', { title: '' })
  const scope = effectScope()
  try {
    const text = scope.run(() => useTextFile())!
    await vi.waitFor(() => expect(text.loading.value).toBe(false))
    expect(text.error.value).toBe('')
    expect(text.renderedMarkdown.value).toBe('<safe># Document preview')
    expect(text.canEdit.value).toBe(false)
    const base = guest ? '/fs?id=scoped-file&action=preview' : '/fs?id=' + encodeURIComponent(id) + '&action=preview'
    expect(fetch).toHaveBeenLastCalledWith(base)
    expect(fetch).toHaveBeenCalledTimes(1)
    const { download } = await import('@/lib/api/file')
    vi.mocked(download).mockClear()
    text.downloadFile()
    expect(download).toHaveBeenCalledWith(guest ? '/fs?id=scoped-file&action=download' : '/fs?id=' + encodeURIComponent(id) + '&action=download', 'report.docx')
  } finally { scope.stop() }
})
