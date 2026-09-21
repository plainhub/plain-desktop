import { defineStore } from 'pinia'
import { compareLocale } from '@/lib/array'

export type Bookmark = {
  id: string
  url: string
  title: string
  faviconPath: string
  groupId: string
  pinned: boolean
  clickCount: number
  lastClickedAt: string | null
  sortOrder: number
  createdAt: string
  updatedAt: string
}

export type BookmarkGroup = {
  id: string
  name: string
  collapsed: boolean
  sortOrder: number
  itemCount: number
  createdAt: string
  updatedAt: string
}

export type BookmarkSortOrder = 'AZ' | 'RECENT_CLICK'

export type BookmarksState = {
  bookmarks: Bookmark[]
  groups: BookmarkGroup[]
  sortOrder: BookmarkSortOrder
}

export const useBookmarksStore = defineStore('bookmarks', {
  state: (): BookmarksState => ({
    bookmarks: [],
    groups: [],
    sortOrder: 'AZ',
  }),

  getters: {
    /** Bookmarks sorted and pinned first within each group */
    sortedBookmarks(state): Bookmark[] {
      const list = [...state.bookmarks]
      list.sort((a, b) => {
        // pinned always first
        if (a.pinned !== b.pinned) return a.pinned ? -1 : 1
        if (state.sortOrder === 'AZ') {
          return compareLocale(a.title, b.title)
        } else {
          // most recently clicked first; items never clicked go to end
          const ta = a.lastClickedAt ? new Date(a.lastClickedAt).getTime() : 0
          const tb = b.lastClickedAt ? new Date(b.lastClickedAt).getTime() : 0
          return tb - ta
        }
      })
      return list
    },

    ungroupedBookmarks(state): Bookmark[] {
      return this.sortedBookmarks.filter((b: Bookmark) => !b.groupId)
    },

    bookmarksByGroupId(state) {
      return (groupId: string): Bookmark[] => {
        return this.sortedBookmarks.filter((b: Bookmark) => b.groupId === groupId)
      }
    },

    sortedGroups(state): BookmarkGroup[] {
      return [...state.groups].sort((a, b) => a.sortOrder - b.sortOrder || compareLocale(a.name, b.name))
    },
  },

  actions: {
    setData(bookmarks: Bookmark[], groups: BookmarkGroup[]) {
      this.bookmarks = bookmarks.map((b) => ({ ...b }))
      this.groups = groups.map((g) => ({ ...g }))
    },

    addBookmarks(bookmarks: Bookmark[]) {
      this.bookmarks.push(...bookmarks.map((b) => ({ ...b })))
    },

    updateBookmark(updated: Partial<Bookmark> & { id: string }) {
      const idx = this.bookmarks.findIndex((b) => b.id === updated.id)
      if (idx !== -1) {
        this.bookmarks[idx] = { ...this.bookmarks[idx], ...updated }
      }
    },

    deleteBookmarks(ids: string[]) {
      const idSet = new Set(ids)
      this.bookmarks = this.bookmarks.filter((b) => !idSet.has(b.id))
    },

    addGroup(group: BookmarkGroup) {
      this.groups.push({ ...group })
    },

    updateGroup(updated: Partial<BookmarkGroup> & { id: string }) {
      const idx = this.groups.findIndex((g) => g.id === updated.id)
      if (idx !== -1) {
        this.groups[idx] = { ...this.groups[idx], ...updated }
      }
    },

    deleteGroup(id: string) {
      this.groups = this.groups.filter((g) => g.id !== id)
      // move orphaned bookmarks to ungrouped
      this.bookmarks = this.bookmarks.map((b) => (b.groupId === id ? { ...b, groupId: '' } : b))
    },

    // Optimistic toggle so the collapse animation is instant without a refetch
    toggleGroupCollapsed(id: string) {
      const group = this.groups.find((g) => g.id === id)
      if (group) group.collapsed = !group.collapsed
    },
  },
})
