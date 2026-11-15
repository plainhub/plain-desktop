import { expect, it, vi } from 'vitest'
import { createRenderer } from 'vue'
import { addRecentDirectory, useRecentSelectedDirs } from '@/hooks/recent-selected-dirs'

const api = vi.hoisted(() => ({ context: 'device-a', read: vi.fn(), set: vi.fn() }))
vi.mock('@/lib/api/gql-client', () => ({
  bindGraphqlClient: () => api.read,
  getGraphqlContextKey: () => api.context,
}))
vi.mock('@/lib/prefs/core', () => ({ preferencesQuery: 'prefs', set: api.set }))

function mountHistory() {
  const renderer = createRenderer<object, object>({
    insert: vi.fn(), remove: vi.fn(), createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
    setText: vi.fn(), setElementText: vi.fn(), parentNode: () => null, nextSibling: () => null, patchProp: vi.fn(),
  })
  let history!: ReturnType<typeof useRecentSelectedDirs>
  const app = renderer.createApp({ setup() { history = useRecentSelectedDirs(); return () => null } })
  app.mount({})
  return { ...history, unmount: () => app.unmount() }
}


it('uses one newest-first list, preserves the path spelling, and caps history at eight', () => {
  const old = Array.from({ length: 8 }, (_, i) => `/archive/${i}`)
  expect(addRecentDirectory(old, '/recent')).toEqual(['/recent', ...old.slice(0, 7)])
  expect(addRecentDirectory(['/photos', '/Photos'], '/Photos')).toEqual(['/Photos', '/photos'])
})

it('coalesces Windows spellings and keeps a drive root absolute', () => {
  expect(addRecentDirectory(['D:/Photos', 'E:/Photos'], 'd:\\PHOTOS\\')).toEqual(['D:/PHOTOS', 'E:/Photos'])
  expect(addRecentDirectory([], 'E:\\')).toEqual(['E:/'])
})


it('reads only the approved key and never saves an old selection to a newly selected device', async () => {
  api.context = 'device-a'
  api.read.mockResolvedValue({ data: { prefs: { recent_selected_dirs: ['/old'] } } })
  api.set.mockResolvedValue(undefined)
  const history = mountHistory()
  try {
    await history.remember('/new')
    expect(api.read).toHaveBeenCalledExactlyOnceWith('prefs', { keys: ['recent_selected_dirs'] })
    expect(api.set).toHaveBeenCalledExactlyOnceWith('recent_selected_dirs', ['/new', '/old'])
    api.context = 'device-b'
    await history.remember('/not-on-device-b')
    expect(api.set).toHaveBeenCalledOnce()
  } finally { history.unmount() }
})
