import { afterEach, expect, it, vi } from 'vitest'
import { effectScope, ref } from 'vue'

vi.hoisted(() => vi.stubGlobal('__IS_TAURI__', false))
const io = vi.hoisted(() => ({ route: { name: 'text-file', query: {} as Record<string, string> }, fetch: vi.fn(), token: '', cached: false }))
vi.mock('vue', async original => ({ ...await original<typeof import('vue')>(), onMounted: vi.fn(), onBeforeUnmount: vi.fn() }))
vi.mock('vue-router', () => ({ useRoute: () => io.route, useRouter: () => ({ push: vi.fn() }) }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('pinia', () => ({ storeToRefs: (value: unknown) => value }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ app: ref(null), urlTokenKey: ref(io.cached ? new Uint8Array(32) : null) }) }))
vi.mock('@/hooks/markdown', () => ({ useMarkdown: () => ({ render: async (value: string) => value }) }))
vi.mock('@/lib/native-file-actions', () => ({ revealLocalFile: vi.fn(), saveTextFileAs: vi.fn() }))
vi.mock('@/lib/api/api', () => ({ getProxyUrl: (value: string) => value }))
vi.mock('@/lib/api/mutation', () => ({ initMutation: () => ({ mutate: vi.fn(), loading: ref(false) }), runMutation: vi.fn(), writeTextFileGQL: 'write' }))
vi.mock('@/lib/api/query', () => ({ appGQL: 'app' }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: io.fetch }))
vi.mock('@/lib/api/file', () => ({ tokenToKey: () => new Uint8Array(32) }))
vi.mock('@/lib/api/crypto', () => ({ chachaDecrypt: () => '/owned/note.txt' }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/lib/feature', () => ({ hasFileReveal: () => false }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: () => io.token }))
import { useTextFile } from '@/views/text-file/text-file'

afterEach(() => vi.unstubAllGlobals())

it.each([{ guest: true, cached: false }, { guest: false, cached: false }, { guest: true, cached: true }])('loads a text resource with the required authorization only ($guest, cached=$cached)', async ({ guest, cached }) => {
  io.cached = cached
  io.route.query = guest ? { id: 'scoped-file', sid: 'owned-share' } : { id: 'owned-file' }
  io.token = guest && !cached ? '' : 'owner-session'
  io.fetch.mockReset().mockResolvedValue({ data: { app: { urlToken: 'owner-resource-key' } } })
  const fetch = vi.fn().mockResolvedValue(new Response('Shared text 中文\n', { headers: { 'content-disposition': 'inline; filename="note.txt"' } }))
  vi.stubGlobal('fetch', fetch)
  vi.stubGlobal('document', { title: '' })
  const scope = effectScope()
  try {
    const text = scope.run(() => useTextFile())!
    await vi.waitFor(() => expect(text.loading.value).toBe(false))
    expect(text.content.value).toBe('Shared text 中文\n')
    expect(text.canEdit.value).toBe(!guest)
    expect(fetch).toHaveBeenCalledExactlyOnceWith(guest ? '/fs?id=scoped-file&sid=owned-share' : '/fs?id=owned-file')
    expect(io.fetch).toHaveBeenCalledTimes(guest ? 0 : 1)
  } finally { scope.stop() }
})
