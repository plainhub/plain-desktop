import { beforeEach, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { exportBookmarksHtml } from '@/lib/bookmarks-format'
import { useBookmarksStore, type Bookmark } from '@/stores/bookmarks'

beforeEach(() => setActivePinia(createPinia()))

const created: Bookmark = {
  id: 'saved-link',
  url: 'https://example.com/page',
  title: 'https://example.com/page',
  faviconPath: '',
  groupId: null,
  pinned: false,
  clickCount: 0,
  lastClickedAt: null,
  sortOrder: 0,
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-01T00:00:00Z',
}
const fetched: Bookmark = { ...created, title: 'Fetched title', faviconPath: 'fid:fixture.ico' }

it.each([false, true])('retains asynchronous metadata when update arrives first: %s', (updateFirst) => {
  const store = useBookmarksStore()
  if (updateFirst) {
    store.updateBookmark(fetched)
    store.addBookmarks([created])
  } else {
    store.addBookmarks([created])
    store.updateBookmark(fetched)
  }
  expect(store.bookmarks).toEqual([fetched])
})

it('keeps the bookmarks of a deleted group in the ungrouped list and HTML export', async () => {
  const store = useBookmarksStore()
  store.addBookmarks([{ ...created, groupId: 'links' }])
  store.deleteGroup('links')
  expect(store.ungroupedBookmarks).toEqual([created])
  const capture = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:fixture')
  vi.stubGlobal('document', { createElement: () => ({ click: vi.fn() }) })
  try {
    exportBookmarksHtml(store.bookmarks, store.groups)
    const html = await (capture.mock.calls[0][0] as Blob).text()
    expect(html).toContain('HREF="' + created.url + '"')
    expect(html).not.toContain('<H3>')
  } finally { capture.mockRestore(); vi.unstubAllGlobals() }
})
