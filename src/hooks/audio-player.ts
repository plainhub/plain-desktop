import { computed, nextTick, onMounted, onUnmounted, ref, watch, type Ref } from 'vue'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import type { IAudioItem } from '@/lib/interfaces'
import { getFileUrlByPath } from '@/lib/api/file'
import { initMutation, selectAudioGQL, updateAudioPlayModeGQL, removeAudioFromQueueGQL, clearAudioQueueGQL, reorderAudioQueueGQL, enqueueAndSelectAudioGQL } from '@/lib/api/mutation'
import { audioPlayMode, audioPlayback, useAudioPlaylistStore } from '@/hooks/audio-playlist-store'
import emitter from '@/plugins/eventbus'
import { useMediaPlayHistory } from './media-play-history'
import { getGraphqlContextKey } from '@/lib/api/gql-client'

let playbackIntent = 0
let pendingSelection = Promise.resolve()

function beginSelection() {
  const intent = ++playbackIntent
  const context = getGraphqlContextKey()
  return () => intent === playbackIntent && context === getGraphqlContextKey()
}

export function usePlayAudio(onApplied: (track: IAudioItem) => void, enqueue = false) {
  const store = useAudioPlaylistStore()
  const { mutate, loading } = initMutation({ document: enqueue ? enqueueAndSelectAudioGQL : selectAudioGQL })
  function play(variables: { mediaId: string; query?: string }, active = beginSelection()) {
    const command = pendingSelection.then(async () => {
      if (!active()) return
      const result = await mutate(variables)
      const track = result?.data?.selectAudio as IAudioItem | undefined
      if (!active() || !track) return
      audioPlayback.value = { mediaId: track.mediaId, path: track.path }
      onApplied(track)
      if (enqueue) await store.refetch()
    })
    pendingSelection = command.catch((error) => emitter.emit('toast', error instanceof Error ? error.message : 'network_error'))
    return command
  }
  return { play, loading }
}

export function useAudioPlaylist(audioRef: Ref<HTMLAudioElement | undefined>) {
  const { app, urlTokenKey, audioPlaying } = storeToRefs(useTempStore())
  const store = useAudioPlaylistStore()
  const audios = computed<IAudioItem[]>(() => store.items.value)
  const playlistAudios = computed<IAudioItem[]>({
    get: () => audios.value,
    set: (value) => {
      store.items.value = value
    },
  })

  const current = ref<IAudioItem | undefined>()
  const src = ref('')

  function updateMediaSessionPlaybackState() {
    if (!('mediaSession' in navigator)) return
    navigator.mediaSession.playbackState = audioPlaying.value ? 'playing' : 'paused'
  }

  function updateMediaSessionMetadata() {
    if (!('mediaSession' in navigator) || typeof MediaMetadata === 'undefined') return
    const item = current.value
    if (!item) {
      navigator.mediaSession.metadata = null
      return
    }
    navigator.mediaSession.metadata = new MediaMetadata({
      title: item.title || item.path.split('/').pop() || 'PlainApp',
      artist: item.artist || app.value?.deviceName || 'PlainApp',
      album: 'PlainApp',
      artwork: [
        { src: '/icons/192.png', sizes: '192x192', type: 'image/png' },
        { src: '/icons/512.png', sizes: '512x512', type: 'image/png' },
      ],
    })
    updateMediaSessionPlaybackState()
  }

  function setupMediaSessionActions() {
    if (!('mediaSession' in navigator)) return
    navigator.mediaSession.setActionHandler('play', () => _play())
    navigator.mediaSession.setActionHandler('pause', () => audioRef.value?.pause())
    navigator.mediaSession.setActionHandler('previoustrack', () => playPrev())
    navigator.mediaSession.setActionHandler('nexttrack', () => playNext())
  }

  function clearMediaSessionActions() {
    if (!('mediaSession' in navigator)) return
    navigator.mediaSession.setActionHandler('play', null)
    navigator.mediaSession.setActionHandler('pause', null)
    navigator.mediaSession.setActionHandler('previoustrack', null)
    navigator.mediaSession.setActionHandler('nexttrack', null)
    navigator.mediaSession.metadata = null
  }

  async function setCurrent() {
    const c = audioPlayback.value.path
    src.value = c ? getFileUrlByPath(urlTokenKey.value, c) : ''
    const item = audios.value.find((it) => it.mediaId === audioPlayback.value.mediaId)
    if (item) current.value = item
    else if (current.value?.path !== c) current.value = undefined
    updateMediaSessionMetadata()
  }
  setCurrent()
  watch(() => audioPlayback.value.path, setCurrent)
  watch(audios, setCurrent)
  const { play } = usePlayAudio((track) => {
    const intent = playbackIntent
    const context = getGraphqlContextKey()
    current.value = track
    void nextTick(() => {
      if (intent === playbackIntent && context === getGraphqlContextKey()) _play(0)
    })
  })
  const { mutate: clear, loading: clearLoading, onDone: onClearDone } = initMutation({ document: clearAudioQueueGQL })
  const { mutate: updateMode } = initMutation({ document: updateAudioPlayModeGQL })
  const history = useMediaPlayHistory()
  const { mutate: reorderAudioQueue } = initMutation({ document: reorderAudioQueueGQL })
  const { mutate: deleteAudio } = initMutation({ document: removeAudioFromQueueGQL })

  onClearDone(() => {
    audioPlayback.value = { mediaId: null, path: null }
    store.reset()
  })

  let restoreInitialPosition = true
  let disposed = false
  let pendingCommand = Promise.resolve()
  let releasePlayback: (() => void) | undefined
  let binding: { element: HTMLAudioElement; context: string; mediaId: string | null; path: string; intent: number } | undefined

  function bindPlayback() {
    const element = audioRef.value
    if (!element) return
    const context = getGraphqlContextKey()
    const expected = { element, context, mediaId: audioPlayback.value.mediaId,
      path: audioPlayback.value.path || '', intent: playbackIntent }
    if (binding?.element === element && binding.context === context && binding.mediaId === expected.mediaId &&
      binding.path === expected.path && binding.intent === expected.intent) return
    releasePlayback?.()
    binding = expected
    const active = () => !disposed && binding === expected && playbackIntent === expected.intent && context === getGraphqlContextKey() &&
      audioPlayback.value.mediaId === expected.mediaId && audioPlayback.value.path === expected.path
    const release = history.bind(element, { mediaId: expected.mediaId, type: 'AUDIO' }, active, onEnded)
    releasePlayback = () => {
      release()
      if (binding === expected) binding = undefined
    }
  }

  function _play(positionMs?: number) {
    if (disposed) return
    restoreInitialPosition = false
    if (audioRef.value && positionMs !== undefined) audioRef.value.currentTime = positionMs / 1000
    bindPlayback()
    void audioRef.value?.play().catch((error: Error) => emitter.emit('toast', error.message))
  }

  async function loadMore(active: () => boolean) {
    const before = audios.value.length
    await store.loadMore()
    return active() && audios.value.length > before
  }

  async function move(next: boolean) {
    const selection = beginSelection()
    const active = () => !disposed && selection()
    releasePlayback?.()
    try {
      if (!store.total.value) await store.ensureLoaded()
      if (!active() || !store.total.value) return
      const id = audioPlayback.value.mediaId
      let index = audios.value.findIndex((item) => item.mediaId === id)
      while (id && index < 0 && audios.value.length < store.total.value) {
        if (!(await loadMore(active))) return
        index = audios.value.findIndex((item) => item.mediaId === id)
      }
      const total = store.total.value
      if (!total) return
      let target: number
      if (audioPlayMode.value === 'SHUFFLE') {
        const exclude = total > 1 && index >= 0
        target = Math.floor(Math.random() * (total - Number(exclude)))
        if (exclude && target >= index) target++
      } else {
        target = next ? (index + 1) % total : index <= 0 ? total - 1 : index - 1
      }
      while (audios.value.length <= target && audios.value.length < store.total.value) {
        if (!(await loadMore(active))) return
      }
      const item = audios.value[target]
      if (active() && item?.mediaId) await play({ mediaId: item.mediaId }, active)
    } finally {
      if (active() && audioPlayback.value.path) bindPlayback()
    }
  }

  function onEnded() {
    releasePlayback?.()
    if (audioPlayMode.value === 'REPEAT_ONE') _play(0)
    else return move(true)
  }

  function playPrev() { return move(false) }
  function playNext() { return move(true) }
  function changeMode() {
    const context = getGraphqlContextKey()
    const command = pendingCommand.then(async () => {
      if (disposed || context !== getGraphqlContextKey()) return
      const modes = ['REPEAT', 'REPEAT_ONE', 'SHUFFLE'] as const
      const mode = modes[(modes.findIndex((mode) => mode === audioPlayMode.value) + 1) % modes.length]
      const result = await updateMode({ mode })
      if (!disposed && context === getGraphqlContextKey() && result?.data?.updateAudioPlayMode) {
        audioPlayMode.value = mode!
      }
    })
    pendingCommand = command.catch((error) => emitter.emit('toast', error instanceof Error ? error.message : 'network_error'))
    return command
  }

  function playItem(item: IAudioItem) {
    if (!item.mediaId) return
    const selection = beginSelection()
    releasePlayback?.()
    void play({ mediaId: item.mediaId }, () => !disposed && selection())
      .then(() => { if (!disposed && selection()) bindPlayback() })
  }

  async function deleteItem(item: IAudioItem) {
    if (!item.mediaId) return
    if (audioPlayback.value.mediaId === item.mediaId) {
      beginSelection()
      releasePlayback?.()
    }
    const result = await deleteAudio({ mediaId: item.mediaId })
    if (!result) return
    store.removeLocal(item.mediaId)
    if (audioPlayback.value.mediaId === item.mediaId) {
      audioRef.value?.pause()
      audioPlayback.value = { mediaId: null, path: null }
    }
    await store.refetch()
  }

  async function onReorder() {
    const result = await reorderAudioQueue({ mediaIds: playlistAudios.value.map((item) => item.mediaId) })
    if (!result) await store.refetch()
  }

  function clearPlaylist() {
    beginSelection()
    releasePlayback?.()
    void clear()
  }

  const onLoadedMetadata = async () => {
    const element = audioRef.value
    if (!element || !restoreInitialPosition) return
    const { mediaId, path } = audioPlayback.value
    const intent = playbackIntent
    try {
      await history.restorePosition(element, { mediaId, type: 'AUDIO' }, () => !disposed && restoreInitialPosition &&
        audioPlayback.value.mediaId === mediaId && audioPlayback.value.path === path && playbackIntent === intent)
    } catch (error) {
      emitter.emit('toast', error instanceof Error ? error.message : 'network_error')
    }
  }
  const onPlay = () => {
    restoreInitialPosition = false
    bindPlayback()
    audioPlaying.value = true
    updateMediaSessionPlaybackState()
  }
  const onPause = () => {
    audioPlaying.value = false
    updateMediaSessionPlaybackState()
  }
  const doPlayAudio = () => {
    const intent = playbackIntent
    const context = getGraphqlContextKey()
    void nextTick(() => {
      if (intent === playbackIntent && context === getGraphqlContextKey()) _play(0)
    })
  }
  const pauseAudio = () => audioRef.value?.pause()

  onMounted(() => {
    store.ensureLoaded()
    setupMediaSessionActions()
    emitter.on('do_play_audio', doPlayAudio)
    emitter.on('pause_audio', pauseAudio)
    audioRef.value?.addEventListener('loadedmetadata', onLoadedMetadata)
    audioRef.value?.addEventListener('pause', onPause)
    audioRef.value?.addEventListener('play', onPlay)
  })

  onUnmounted(() => {
    disposed = true
    releasePlayback?.()
    emitter.off('do_play_audio', doPlayAudio)
    emitter.off('pause_audio', pauseAudio)
    audioRef.value?.removeEventListener('loadedmetadata', onLoadedMetadata)
    audioRef.value?.removeEventListener('pause', onPause)
    audioRef.value?.removeEventListener('play', onPlay)
    clearMediaSessionActions()
  })

  return {
    app,
    mode: audioPlayMode,
    audios,
    playlistAudios,
    queueTotal: store.total,
    queueLoading: store.loading,
    loadMore: store.loadMore,
    current,
    src,
    clearLoading,
    playPrev,
    playNext,
    changeMode,
    playItem,
    deleteItem,
    clearPlaylist,
    onReorder,
  }
}
