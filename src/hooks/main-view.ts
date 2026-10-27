import { isLocalMode } from '@/lib/device/local-mode'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'
import { onMounted, ref, watch, computed, onUnmounted } from 'vue'
import { useMainStore } from '@/stores/main'
import { useRouter } from 'vue-router'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import { initQuery, appGQL } from '@/lib/api/query'
import emitter from '@/plugins/eventbus'
import { tokenToKey } from '@/lib/api/file'
import type { IApp, IMediaItemsActionedEvent } from '@/lib/interfaces'
import { useRightSidebarResize } from '@/hooks/sidebar'
import { getMainStateKey } from '@/lib/device/current'
import { get as prefsGet, set as prefsSet } from '@/lib/prefs/browser'
import { debounce } from '@/lib/array'
import { hasActiveUploadBatches } from '@/lib/upload/batch-progress'
import { useAudioPlaylistStore } from '@/hooks/audio-playlist-store'
import { openModal } from '@/components/modal'
import type { PairingRequest } from '@/lib/pairing-types'
import PairingRequestModal from '@/views/chat/PairingRequestModal.vue'
import { DataType } from '@/lib/data'
import { subscribeMediaLibraryUpdates } from '@/lib/media/library-updates'
import { useRemotePrefs } from '@/hooks/remote-prefs'

export function useMainView() {
  const store = useMainStore()
  const router = useRouter()
  const tempStore = useTempStore()
  const { app, urlTokenKey } = storeToRefs(tempStore)

  const appReady = ref(false)
  const errorMessage = ref('')

  const localMode = isLocalMode()
  const audioPlaylist = useAudioPlaylistStore()

  const hiddenHeaderSearchRoutes = new Set(['/files/recent', '/screen-mirror'])
  const hiddenHeaderSearchPatterns = [/^\/chat(?:\/|$)/, /^\/developer(?:\/|$)/, /^\/image-editor(?:\/|$)/]

  function toggleSidebar() { store.miniSidebar = !store.miniSidebar }

  const hasTasks = computed(() => tempStore.uploads.length > 0)

  const hasActiveUploads = computed(() => hasActiveUploadBatches())

  const hasLeftSidebar = computed(() => {
    const route = router.currentRoute.value
    const matchedRoute = route.matched[route.matched.length - 1]
    return matchedRoute?.components?.LeftSidebar !== undefined
  })

  function getSidebar2CacheKey() {
    const route = router.currentRoute.value
    const matchedPath = route.matched[route.matched.length - 1]?.path ?? ''
    return (route.meta.group ?? '') + matchedPath + (route.query.q ?? '')
  }

  function toggleQuick(name: string) {
    store.quick = store.quick === name ? '' : name
  }

  const showHeaderSearch = computed(() => {
    const route = router.currentRoute.value
    return !hiddenHeaderSearchRoutes.has(route.path) && !hiddenHeaderSearchPatterns.some((p) => p.test(route.path))
  })

  const refetchApp = initQuery({
    handle: (data: { app: IApp }, error: string) => {
      if (error) {
        errorMessage.value = error
      } else if (data) {
        const oldToken = app.value?.urlToken
        const newToken = data.app.urlToken
        urlTokenKey.value = tokenToKey(newToken)
        if (oldToken !== newToken) window.fileIdMap = new Map<string, string>()
        app.value = data.app
      }
      appReady.value = true
    },
    document: appGQL,
  }).refetch

  const { resizeWidth } = useRightSidebarResize(
    300,
    () => store.quickContentWidth,
    (width: number) => { store.quickContentWidth = width },
  )

  const currentPath = ref(router.currentRoute.value.fullPath)
  watch(() => router.currentRoute.value.fullPath, (v: string) => { currentPath.value = v })

  let stopLibraryUpdates: (() => void) | undefined
  const refetchAppHandler = () => refetchApp()
  const mediaItemsActionedHandler = (event: IMediaItemsActionedEvent) => {
    if (event.type === 'AUDIO') void audioPlaylist.refetch()
  }
  const deviceNameUpdatedHandler = (name: string) => {
    if (app.value) app.value.deviceName = name
  }

  const pairingRequestHandler = (request: PairingRequest) => {
    openModal(PairingRequestModal, { request })
  }
  let stopDeviceNameUpdated: (() => void) | undefined
  let stopPermissionsUpdated: (() => void) | undefined

  onMounted(() => {
    stopLibraryUpdates = subscribeMediaLibraryUpdates(DataType.AUDIO, () => { void audioPlaylist.refetch() })
    emitter.on('refetch_app', refetchAppHandler)
    stopPermissionsUpdated = graphqlSubscriptions.subscribe('permissionsUpdated', refetchAppHandler)
    emitter.on('media_items_actioned', mediaItemsActionedHandler)
    stopDeviceNameUpdated = graphqlSubscriptions.subscribe('deviceNameUpdated', deviceNameUpdatedHandler)
    emitter.on('PAIRING_REQUEST_RECEIVED', pairingRequestHandler)
  })

  onUnmounted(() => {
    stopLibraryUpdates?.()
    emitter.off('refetch_app', refetchAppHandler)
    stopPermissionsUpdated?.()
    emitter.off('media_items_actioned', mediaItemsActionedHandler)
    stopDeviceNameUpdated?.()
    emitter.off('PAIRING_REQUEST_RECEIVED', pairingRequestHandler)
  })

  // Restore persisted state from prefs
  const localState = prefsGet<Record<string, unknown> | null>(getMainStateKey(), null)
  if (localState) {
    const { chatTexts, searchHistory, ...uiState } = localState
    store.$state = { ...store.$state, ...uiState }
  }

  const remotePrefsEnabled = useRemotePrefs(store)

  // High-frequency mutations (e.g. chatTexts keystrokes) must not serialize
  // the whole store and hit the prefs backend on every change — coalesce them.
  const persistState = debounce((state: typeof store.$state) => {
    if (remotePrefsEnabled.value) {
      const { homeFeatures, chatTexts, searchHistory, ...localFields } = state
      prefsSet(getMainStateKey(), localFields)
    } else {
      const { chatTexts, searchHistory, ...localFields } = state
      prefsSet(getMainStateKey(), localFields)
    }
  }, 500)

  watch(store.$state, (state) => {
    persistState(state)
    currentPath.value = router.currentRoute.value.fullPath
  }, { deep: true })
  watch(remotePrefsEnabled, (enabled) => {
    if (enabled) persistState(store.$state)
  })

  return {
    store, app, appReady, errorMessage,
    hasTasks, hasActiveUploads, hasLeftSidebar, showHeaderSearch, localMode,
    toggleSidebar, toggleQuick, getSidebar2CacheKey, resizeWidth,
  }
}
