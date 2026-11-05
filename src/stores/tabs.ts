import { defineStore } from 'pinia'
import { generateRecordId } from '@/lib/record-id.generated'

export interface AppTab {
  id: string
  title: string
  path: string
  /** meta.group for feature tabs; null for path-based file tabs */
  group: string | null
}

export const useTabsStore = defineStore('tabs', {
  state: (): { tabs: AppTab[]; activeTabId: string } => ({
    tabs: [],
    activeTabId: '',
  }),
  actions: {
    /**
     * Called from router afterEach for every feature navigation.
     * Deduplicates by meta.group: one tab per feature group.
     */
    syncRoute(group: string, title: string, path: string) {
      const existing = this.tabs.find((t) => t.group === group)
      if (existing) {
        existing.path = path
        existing.title = title
        this.activeTabId = existing.id
      } else {
        const id = generateRecordId()
        this.tabs.push({ id, title, path, group })
        this.activeTabId = id
      }
    },
    /** Called when navigating to home or a route with no meta.group. */
    setActiveHome() {
      this.activeTabId = ''
    },
    /**
     * Path-based tab for file viewers (text-file, etc.).
     * Multiple file tabs can coexist, each with a unique path.
     */
    openTab(title: string, path: string) {
      const existing = this.tabs.find((t) => t.path === path)
      if (existing) {
        this.activeTabId = existing.id
        return
      }
      const id = generateRecordId()
      this.tabs.push({ id, title, path, group: null })
      this.activeTabId = id
    },
    /** Returns the path to navigate to after closing, or null if tab was not active. */
    closeTab(id: string): string | null {
      const idx = this.tabs.findIndex((t) => t.id === id)
      if (idx === -1) return null
      const wasActive = this.activeTabId === id
      this.tabs.splice(idx, 1)
      if (wasActive) {
        const newActive = this.tabs[Math.max(0, idx - 1)]
        this.activeTabId = newActive?.id ?? ''
        return newActive?.path ?? '/'
      }
      return null
    },
    setActive(id: string) {
      this.activeTabId = id
    },
  },
})
