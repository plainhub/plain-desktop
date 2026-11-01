import { defineStore } from 'pinia'
import { DEFAULT_HOME_FEATURES } from '@/views/home/features'
import { shortUUID } from '@/lib/strutil'

const DEFAULT_RAIL_FEATURES = ['files', 'audios', 'images', 'videos', 'chat']

export interface AppTab {
  id: string
  title: string
  path: string
  /** meta.group for feature tabs; null for path-based file tabs */
  group: string | null
  closeable: boolean
}

const HOME_TAB: AppTab = { id: 'home', title: 'Home', path: '/', group: 'home', closeable: false }

// data will be stored to local storage
export type MainState = {
  railFeatures: string[]
  homeFeatures: string[]
  uploadDirs: Record<string, string>
  excludedDirs: string[]
  fileShowHidden: boolean
  chatTexts: Record<string, string>
  quick: string
  quickContentWidth: number
  sidebarWidth: number
  sidebar2Width: number
  miniSidebar: boolean
  lastRoutes: Record<string, string>
  lightboxInfoVisible: boolean
  imageViewQuality: 'fast' | 'original'
  videosCardView: boolean
  imagesCardView: boolean
  pageUIMode: Record<string, 'view' | 'edit'>
  bucketFilterCollapsed: Record<string, boolean>
  appSortBy: string
  fileSortBy: string
  imageSortBy: string
  imagesGroupBy: string // '' | 'TAKEN_AT'
  imagesScrollPaging: boolean
  videosGroupBy: string // '' | 'TAKEN_AT'
  videosScrollPaging: boolean
  audiosScrollPaging: boolean
  docsScrollPaging: boolean
  pageSize: number
  videoSortBy: string
  audioSortBy: string
  docSortBy: string
  conversationSortBy: string
  callNumber: string
  feedEntryFontSize: number // font size for feed entry content
  searchHistory: Record<string, string[]>
  notificationVolume: number
  selectedSimSubscriptionId: number
  tabs: AppTab[]
  activeTabId: string
}

export const useMainStore = defineStore('main', {
  state: () =>
    ({
      railFeatures: [...DEFAULT_RAIL_FEATURES],
      homeFeatures: [...DEFAULT_HOME_FEATURES],
      uploadDirs: {} as Record<string, string>,
      excludedDirs: [] as string[],
      fileShowHidden: false,
      chatTexts: {},
      quick: '',
      quickContentWidth: 400,
      sidebarWidth: 240,
      sidebar2Width: 360,
      miniSidebar: false,
      noteExpand: true,
      lastRoutes: {},
      audios: [],
      audioPlaying: null,
      lightboxInfoVisible: false,
      imageViewQuality: 'fast',
      videosCardView: false,
      imagesCardView: false,
      pageUIMode: {},
      bucketFilterCollapsed: {},
      appSortBy: 'NAME_ASC',
      fileSortBy: 'NAME_ASC',
      imageSortBy: 'DATE_DESC',
      imagesGroupBy: '',
      imagesScrollPaging: false,
      videosGroupBy: '',
      videosScrollPaging: false,
      audiosScrollPaging: false,
      docsScrollPaging: false,
      pageSize: 50,
      videoSortBy: 'DATE_DESC',
      audioSortBy: 'DATE_DESC',
      docSortBy: 'DATE_DESC',
      conversationSortBy: 'DATE_DESC',
      callNumber: '',
      feedEntryFontSize: 16, // default font size
      searchHistory: {},
      notificationVolume: 0.5,
      selectedSimSubscriptionId: -1,
      tabs: [{ ...HOME_TAB }],
      activeTabId: 'home',
    }) as MainState,
  actions: {
    increaseFeedEntryFontSize() {
      if (this.feedEntryFontSize < 32) {
        this.feedEntryFontSize += 2
      }
    },
    decreaseFeedEntryFontSize() {
      if (this.feedEntryFontSize > 10) {
        this.feedEntryFontSize -= 2
      }
    },
    resetFeedEntryFontSize() {
      this.feedEntryFontSize = 16
    },
    syncRouteTab(group: string, title: string, path: string) {
      const existing = this.tabs.find((t) => t.group === group)
      if (existing) {
        existing.path = path
        existing.title = title
        this.activeTabId = existing.id
      } else {
        const id = shortUUID()
        this.tabs.push({ id, title, path, group, closeable: true })
        this.activeTabId = id
      }
    },
    setActiveHome() {
      // Ensure home tab always exists
      if (!this.tabs.find((t) => t.id === 'home')) {
        this.tabs.unshift({ ...HOME_TAB })
      }
      this.activeTabId = 'home'
    },
    openFileTab(title: string, path: string) {
      const existing = this.tabs.find((t) => t.path === path)
      if (existing) {
        this.activeTabId = existing.id
        return
      }
      const id = shortUUID()
      this.tabs.push({ id, title, path, group: null, closeable: true })
      this.activeTabId = id
    },
    closeTab(id: string): string | null {
      const idx = this.tabs.findIndex((t) => t.id === id && t.closeable)
      if (idx === -1) return null
      const wasActive = this.activeTabId === id
      this.tabs.splice(idx, 1)
      if (wasActive) {
        const newActive = this.tabs[Math.max(0, idx - 1)]
        this.activeTabId = newActive?.id ?? 'home'
        return newActive?.path ?? '/'
      }
      return null
    },
    closeTabsToRight(id: string): string | null {
      const idx = this.tabs.findIndex((t) => t.id === id)
      if (idx === -1) return null
      const removed = this.tabs.splice(idx + 1)
      if (removed.some((t) => t.id === this.activeTabId)) {
        const newActive = this.tabs[idx] ?? { ...HOME_TAB }
        this.activeTabId = newActive.id
        return newActive.path
      }
      return null
    },
    setActiveTab(id: string) {
      this.activeTabId = id
    },
  },
})
