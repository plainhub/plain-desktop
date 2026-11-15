import { beforeEach, expect, it } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useBookmarksStore, type Bookmark } from '@/stores/bookmarks'

beforeEach(() => setActivePinia(createPinia()))

const created: Bookmark = {
  id: 'saved-link',
  url: 'https://example.com/page',
  title: 'https://example.com/page',
  faviconPath: '',
  groupId: '',
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
