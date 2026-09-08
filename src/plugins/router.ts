import { createRouter, createWebHistory } from 'vue-router'
import MainView from '@/views/MainView.vue'
import type { MainState } from '@/stores/main'
import i18n from '@/plugins/i18n'
import { getCurrentAuthToken } from '@/lib/device/current'
import { isLocalMode, isLocalModeAllowed, isLocalRouteGroup } from '@/lib/device/local-mode'
import { useMainStore } from '@/stores/main'
import { useTempStore } from '@/stores/temp'
import { findLoginPeer } from '@/lib/device/login-peers'
import { getRemoteClientId } from '@/lib/device/client-id'

const router = createRouter({
  strict: true,
  history: createWebHistory(),
  scrollBehavior(_to, _from, savedPosition) {
    if (savedPosition) {
      return savedPosition
    } else {
      return { top: 0 }
    }
  },
  routes: [
    {
      path: '/',
      component: MainView,
      meta: { requiresAuth: true },
      children: [
        {
          name: 'home',
          path: '',
          components: {
            default: () => import('@/views/home/HomeView.vue'),
          },
          meta: { group: 'home' },
        },
        {
          path: 'messages',
          components: {
            LeftSidebar: () => import('@/views/messages/MessagesSidebar.vue'),
            LeftSidebar2: () => import('@/views/messages/MessagesSidebar2.vue'),
          },
          meta: { group: 'messages', className: 'messages' },
        },
        {
          path: 'messages/archived',
          components: {
            LeftSidebar: () => import('@/views/messages/MessagesSidebar.vue'),
            LeftSidebar2: () => import('@/views/messages/MessagesSidebar2.vue'),
          },
          meta: { group: 'messages', className: 'messages' },
        },
        {
          path: 'messages/archived/:threadId',
          components: {
            default: () => import('@/views/messages/MessagesView.vue'),
            LeftSidebar: () => import('@/views/messages/MessagesSidebar.vue'),
            LeftSidebar2: () => import('@/views/messages/MessagesSidebar2.vue'),
          },
          meta: { group: 'messages', className: 'messages' },
        },
        {
          path: 'messages/:threadId',
          components: {
            default: () => import('@/views/messages/MessagesView.vue'),
            LeftSidebar: () => import('@/views/messages/MessagesSidebar.vue'),
            LeftSidebar2: () => import('@/views/messages/MessagesSidebar2.vue'),
          },
          meta: { group: 'messages', className: 'messages' },
        },
        {
          path: 'calls',
          components: {
            default: () => import('@/views/calls/CallsView.vue'),
            LeftSidebar: () => import('@/views/calls/CallsSidebar.vue'),
          },
          meta: { group: 'calls' },
        },
        {
          path: 'apps',
          components: {
            default: () => import('@/views/apps/AppsView.vue'),
            LeftSidebar: () => import('@/views/apps/AppsSidebar.vue'),
          },
          meta: { group: 'apps' },
        },
        {
          path: 'contacts',
          components: {
            default: () => import('@/views/contacts/ContactsView.vue'),
            LeftSidebar: () => import('@/views/contacts/ContactsSidebar.vue'),
          },
          meta: { group: 'contacts' },
        },
        {
          path: 'images',
          components: {
            default: () => import('@/views/images/ImagesView.vue'),
            LeftSidebar: () => import('@/views/images/ImagesSidebar.vue'),
          },
          meta: { group: 'images' },
        },
        {
          path: 'videos',
          components: {
            default: () => import('@/views/videos/VideosView.vue'),
            LeftSidebar: () => import('@/views/videos/VideosSidebar.vue'),
          },
          meta: { group: 'videos' },
        },
        {
          path: 'audios',
          components: {
            default: () => import('@/views/audios/AudiosView.vue'),
            LeftSidebar: () => import('@/views/audios/AudiosSidebar.vue'),
          },
          meta: { group: 'audios' },
        },
        {
          path: 'docs',
          components: {
            default: () => import('@/views/docs/DocsView.vue'),
            LeftSidebar: () => import('@/views/docs/DocsSidebar.vue'),
          },
          meta: { group: 'docs' },
        },
        {
          path: 'notes/:id',
          components: {
            default: () => import('@/views/notes/NoteEditView.vue'),
            LeftSidebar: () => import('@/views/notes/NotesSidebar.vue'),
            LeftSidebar2: () => import('@/views/notes/NotesSidebar2.vue'),
          },
          meta: { group: 'notes', className: 'note-entry' },
        },
        {
          path: 'notes',
          components: {
            LeftSidebar: () => import('../views/notes/NotesSidebar.vue'),
            LeftSidebar2: () => import('@/views/notes/NotesSidebar2.vue'),
          },
          meta: { group: 'notes', className: 'notes' },
        },
        {
          path: 'files',
          components: {
            default: () => import('@/views/files/FilesView.vue'),
            LeftSidebar: () => import('@/views/files/FilesSidebar.vue'),
          },
          meta: { group: 'files', className: 'files' },
        },
        {
          path: 'files/recent',
          components: {
            default: () => import('@/views/files/FilesRecentView.vue'),
            LeftSidebar: () => import('@/views/files/FilesSidebar.vue'),
          },
          meta: { group: 'files', className: 'files' },
        },
        {
          path: 'screen-mirror',
          component: () => import('@/views/screen-mirror/ScreenMirrorView.vue'),
          meta: { group: 'screen_mirror' },
        },
        {
          path: 'feeds',
          components: {
            LeftSidebar: () => import('@/views/feeds/FeedsSidebar.vue'),
            LeftSidebar2: () => import('@/views/feeds/FeedsSidebar2.vue'),
          },
          meta: { group: 'feeds', className: 'feeds' },
        },
        {
          path: 'feeds/:feedId/entries/:id',
          components: {
            default: () => import('@/views/feeds/FeedEntryView.vue'),
            LeftSidebar: () => import('@/views/feeds/FeedsSidebar.vue'),
            LeftSidebar2: () => import('@/views/feeds/FeedsSidebar2.vue'),
          },
          meta: { group: 'feeds', className: 'feed-entry' },
        },
        {
          path: 'chat',
          components: {
            default: () => import('@/views/chat/ChatView.vue'),
            LeftSidebar: () => import('@/views/chat/ChatSidebar.vue'),
          },
          meta: { group: 'chat', className: 'chat' },
        },
        {
          path: 'chat/app-files',
          components: {
            default: () => import('@/views/app-files/AppFilesView.vue'),
            LeftSidebar: () => import('@/views/chat/ChatSidebar.vue'),
          },
          meta: { group: 'chat', className: 'chat' },
        },
        {
          path: 'developer',
          redirect: '/developer/device-info',
        },
        {
          path: 'developer/datastore',
          components: {
            default: () => import('@/views/developer/DeveloperDataStoreView.vue'),
            LeftSidebar: () => import('@/views/developer/DeveloperSidebar.vue'),
          },
          meta: { group: 'developer' },
        },
        {
          path: 'developer/database',
          components: {
            default: () => import('@/views/developer/DeveloperDatabaseView.vue'),
            LeftSidebar: () => import('@/views/developer/DeveloperSidebar.vue'),
          },
          meta: { group: 'developer' },
        },
        {
          path: 'developer/logs',
          components: {
            default: () => import('@/views/developer/DeveloperLogsView.vue'),
            LeftSidebar: () => import('@/views/developer/DeveloperSidebar.vue'),
          },
          meta: { group: 'developer' },
        },
        {
          path: 'developer/device-info',
          components: {
            default: () => import('@/views/device-info/DeviceInfoView.vue'),
            LeftSidebar: () => import('@/views/developer/DeveloperSidebar.vue'),
          },
          meta: { group: 'developer' },
        },
        {
          path: 'image-editor',
          components: {
            default: () => import('@/views/image-editor/ImageEditorListView.vue'),
          },
          meta: { group: 'image_editor' },
        },
      ],
    },
    {
      name: 'about',
      path: '/about',
      component: () => import('@/views/about/AboutView.vue'),
      meta: { requiresAuth: false },
    },
    {
      name: 'login',
      path: '/login',
      component: () => import('@/views/login/LoginView.vue'),
      meta: { requiresAuth: false },
    },
    {
      name: 'setup',
      path: '/setup',
      component: () => import('@/views/setup/SetupView.vue'),
      meta: { requiresAuth: false },
    },
    {
      // Shared file link. The private `shared_token` travels in the URL
      // fragment (`/s/<shared_id>#<shared_token>`) and is read by the page JS.
      name: 'share',
      path: '/s/:sharedId',
      component: () => import('@/views/share/ShareView.vue'),
      meta: { requiresAuth: false },
    },
    {
      name: 'text-file',
      path: '/text-file',
      component: () => import('@/views/text-file/TextFileView.vue'),
      meta: { requiresAuth: false },
    },
    {
      name: 'media-preview',
      path: '/media-preview',
      component: () => import('@/views/media-preview/MediaPreviewView.vue'),
      meta: { requiresAuth: false, group: 'chat' },
    },
    {
      name: 'ux',
      path: '/ux',
      component: () => import('@/views/ux/UxView.vue'),
      meta: { requiresAuth: false },
    },
    {
      name: 'markdown-preview',
      path: '/markdown-preview',
      component: () => import('@/views/markdown-preview/MarkdownPreviewView.vue'),
      meta: { requiresAuth: false },
    },
    {
      name: 'text-edit',
      path: '/text-edit',
      component: () => import('@/views/text-file/TextFileView.vue'),
      meta: { requiresAuth: true },
    },
    {
      name: 'image-editor-new',
      path: '/image-editor/new',
      component: () => import('@/views/image-editor/ImageEditorView.vue'),
      meta: { requiresAuth: true, group: 'image_editor' },
    },
    {
      name: 'image-editor-by-id',
      path: '/image-editor/:id',
      component: () => import('@/views/image-editor/ImageEditorView.vue'),
      meta: { requiresAuth: true, group: 'image_editor' },
    },
  ],
})

const scrollTops = new Map<string, number>()
router.beforeEach(async (to, from) => {
  const scrollTop = document.getElementsByClassName('main')[0]?.scrollTop
  if (scrollTop !== undefined) {
    scrollTops.set(from.fullPath, scrollTop)
  }
  if (isLocalModeAllowed() && (to.path === '/' || to.meta.group === 'home')) {
    return { path: '/chat' }
  }
  const canAccess = getCurrentAuthToken() || (isLocalModeAllowed() && isLocalRouteGroup(to.meta.group))

  if (to.meta.requiresAuth && !canAccess) {
    return {
      path: '/login',
      query: { redirect: to.fullPath },
    }
  }

  // /media-preview is a self-contained popup window — it carries its source
  // entirely via the `?src=...` query string, so it doesn't need auth.
  // Guard it against being loaded bare (e.g. user typing the URL) by
  // bouncing back to home when no source is present.
  if (to.name === 'media-preview' && !to.query.src) {
    return { path: '/' }
  }

  // clean up tooltip
  clearTimeout(globalThis.showTooltipTimeout)
  setTimeout(() => {
    const tooltips = Array.from(document.getElementsByClassName('tooltip'))
    for (const tooltip of tooltips) {
      if (tooltip.parentNode === document.body) {
        document.body.removeChild(tooltip)
      }
    }
  }, 100)
})

export function updateDocumentTitle(): void {
  const group = (router.currentRoute.value.meta.group as string) || ''
  const titleKey = `page_title.${group}`
  const title = group ? String((i18n.global as any).t(titleKey)) : ''
  const base = title && title !== titleKey ? `${title} - PlainApp` : 'PlainApp'
  const deviceName = useTempStore().app?.deviceName
    || findLoginPeer(getRemoteClientId())?.name
    || ''
  document.title = deviceName ? `${deviceName} - ${base}` : base
}

router.afterEach((to, from) => {
  // Dynamic page title
  updateDocumentTitle()

  // Sync tabs in Tauri mode
  if (__IS_TAURI__) {
    const mainStore = useMainStore()
    const group = (to.meta.group as string) || ''
    const titleKey = `page_title.${group}`
    const title = group ? String((i18n.global as any).t(titleKey)) : ''
    if (group && group !== 'home') {
      mainStore.syncRouteTab(group, title && title !== titleKey ? title : group, to.fullPath)
    } else {
      mainStore.setActiveHome()
    }
  }

  setTimeout(() => {
    const a = document.getElementsByClassName('main')[0]
    if (a) {
      const top = scrollTops.get(to.fullPath)
      a.scrollTop = top || 0
    }
  }, 0)
})

export default router

export const replacePathNoReload = (store: MainState, fullPath: string) => {
  router.currentRoute.value.fullPath = fullPath
  window.history.replaceState({}, document.title, fullPath)
}

export const replacePath = (store: MainState, fullPath: string) => {
  router.push(fullPath)
}

export const getRouteName = (fullPath: string) => {
  return router.resolve(fullPath).meta.group
}
