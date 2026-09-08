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
import { get as prefsGet, set as prefsSet } from '@/lib/prefs'
import { debounce } from '@/lib/array'
import { updateLoginPeerName } from '@/lib/device/login-peers'
import { getRemoteClientId } from '@/lib/device/client-id'
import { isLocalMode } from '@/lib/device/local-mode'
import { openModal } from '@/components/modal'
import type { PairingRequest } from '@/lib/pairing-types'
import PairingRequestModal from '@/views/chat/PairingRequestModal.vue'

export function useMainView() {
  const store = useMainStore()
  const router = useRouter()
  const tempStore = useTempStore()
  const { app, urlTokenKey } = storeToRefs(tempStore)

  const appReady = ref(false)
  const errorMessage = ref('')
  let playAudio = false

  const localMode = isLocalMode()

  const hiddenHeaderSearchRoutes = new Set(['/files/recent', '/screen-mirror'])
  const hiddenHeaderSearchPatterns = [/^\/chat(?:\/|$)/, /^\/developer(?:\/|$)/, /^\/image-editor(?:\/|$)/]

  function toggleSidebar() { store.miniSidebar = !store.miniSidebar }

  const hasTasks = computed(() => tempStore.uploads.length > 0)

  const hasActiveUploads = computed(() => {
    return tempStore.uploads.some((u) => ['uploading', 'saving', 'pending'].includes(u.status))
  })

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
        // Keep the session name in sync with the device's reported name.
        if (data.app.deviceName) {
          void updateLoginPeerName(getRemoteClientId(), data.app.deviceName)
        }
        if (playAudio) { playAudio = false; emitter.emit('do_play_audio') }
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

  const refetchAppHandler = () => refetchApp()
  const playAudioHandler = () => { playAudio = true; refetchApp() }
  const mediaItemsActionedHandler = (event: IMediaItemsActionedEvent) => {
    if (event.type === 'AUDIO') refetchApp()
  }
  const deviceNameUpdatedHandler = (name: string) => {
    if (app.value) app.value.deviceName = name
    if (name) {
      void updateLoginPeerName(getRemoteClientId(), name)
    }
  }

  const pairingRequestHandler = (request: PairingRequest) => {
    openModal(PairingRequestModal, { request })
  }

  onMounted(() => {
    emitter.on('refetch_app', refetchAppHandler)
    emitter.on('play_audio', playAudioHandler)
    emitter.on('media_items_actioned', mediaItemsActionedHandler)
    emitter.on('device_name_updated', deviceNameUpdatedHandler)
    emitter.on('pairing_request_received', pairingRequestHandler)
  })

  onUnmounted(() => {
    emitter.off('refetch_app', refetchAppHandler)
    emitter.off('play_audio', playAudioHandler)
    emitter.off('media_items_actioned', mediaItemsActionedHandler)
    emitter.off('device_name_updated', deviceNameUpdatedHandler)
    emitter.off('pairing_request_received', pairingRequestHandler)
  })

  // Restore persisted state from prefs
  const localState = prefsGet<Record<string, unknown> | null>(getMainStateKey(), null)
  if (localState) {
    store.$state = { ...store.$state, ...localState }
  }

  // High-frequency mutations (e.g. chatTexts keystrokes) must not serialize
  // the whole store and hit the prefs backend on every change — coalesce them.
  const persistState = debounce((state: typeof store.$state) => {
    prefsSet(getMainStateKey(), state)
  }, 500)

  watch(store.$state, (state) => {
    persistState(state)
    currentPath.value = router.currentRoute.value.fullPath
  }, { deep: true })

  return {
    store, app, appReady, errorMessage,
    hasTasks, hasActiveUploads, hasLeftSidebar, showHeaderSearch, localMode,
    toggleSidebar, toggleQuick, getSidebar2CacheKey, resizeWidth,
  }
}
