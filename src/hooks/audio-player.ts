import { computed, nextTick, onMounted, onUnmounted, ref, watch, type Ref } from 'vue'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import type { IAudioItem } from '@/lib/interfaces'
import { getFileUrlByPath } from '@/lib/api/file'
import { initMutation, playAudioGQL, reportAudioPlaybackGQL, advanceAudioSelectionGQL, updateAudioPlayModeGQL, removeAudioFromQueueGQL, clearAudioQueueGQL, reorderAudioQueueGQL, enqueueAndPlayAudioGQL } from '@/lib/api/mutation'
import { audioPlayback, useAudioPlaylistStore } from '@/hooks/audio-playlist-store'
import emitter from '@/plugins/eventbus'
import { getGraphqlContextKey } from '@/lib/api/gql-client'

export function usePlayAudio(onApplied: () => void, enqueue = false) {
  const store = useAudioPlaylistStore()
  const { mutate: play, loading, onDone } = initMutation({ document: enqueue ? enqueueAndPlayAudioGQL : playAudioGQL })
  onDone(async (r: any) => {
    const context = getGraphqlContextKey()
    const path = r?.data?.playAudio?.path
    if (path) audioPlayback.value = { ...audioPlayback.value, mediaId: r.data.playAudio.mediaId, path: path }
    await store.refetch()
    if (context === getGraphqlContextKey()) onApplied()
  })
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
  const { play } = usePlayAudio(() => {
    void nextTick(() => _play())
  })
  const { mutate: clear, loading: clearLoading, onDone: onClearDone } = initMutation({ document: clearAudioQueueGQL })
  const { mutate: updateMode } = initMutation({ document: updateAudioPlayModeGQL })
  const { mutate: advanceSelection } = initMutation({ document: advanceAudioSelectionGQL })
  const { mutate: reportPlayback } = initMutation({ document: reportAudioPlaybackGQL }, false)
  const { mutate: reorderAudioQueue } = initMutation({ document: reorderAudioQueueGQL })
  const { mutate: deleteAudio } = initMutation({ document: removeAudioFromQueueGQL })

  onClearDone(() => {
    audioPlayback.value = { ...audioPlayback.value, mediaId: null, path: null, isPlaying: false, positionMs: 0 }
    store.reset()
  })

  let disposed = false
  let pendingCommand = Promise.resolve()
  let releasePlayback: (() => void) | undefined
  let playingToken: object | undefined

  function bindPlayback() {
    releasePlayback?.()
    releasePlayback = undefined
    const element = audioRef.value
    if (!element) return
    const context = getGraphqlContextKey()
    const expected = { path: audioPlayback.value.path || '', revision: audioPlayback.value.revision }
    const token = {}
    playingToken = token
    let timer: ReturnType<typeof setInterval> | undefined
    let reporting: Promise<void> | undefined
    let rejected = false
    let samplePending = false
    let startPending = false
    const active = () => !disposed && !rejected && playingToken === token && context === getGraphqlContextKey() &&
      audioPlayback.value.path === expected.path && audioPlayback.value.revision === expected.revision &&
      (!element.currentSrc || element.currentSrc === element.src)
    const sample = (started = false): Promise<void> => {
      startPending ||= started
      samplePending = true
      if (!active() || !expected.path) return Promise.resolve()
      if (!reporting) {
        reporting = (async () => {
          while (samplePending && active()) {
            samplePending = false
            const started = startPending
            const positionMs = Number.isFinite(element.currentTime) ? Math.max(0, Math.trunc(element.currentTime * 1000)) : 0
            const result = await reportPlayback({ path: expected.path, revision: expected.revision, positionMs, started })
            if (result?.data?.reportAudioPlayback === false) { rejected = true; stopTimer() }
            if (started && result?.data?.reportAudioPlayback === true) startPending = false
          }
        })().finally(() => { reporting = undefined })
      }
      return reporting
    }
    const stopTimer = () => { if (timer !== undefined) clearInterval(timer); timer = undefined }
    const playing = () => {
      if (!active()) return
      const started = sample(true)
      pendingCommand = Promise.all([pendingCommand, started]).then(() => {}).catch(() => {})
      stopTimer()
      timer = setInterval(() => { if (active()) void sample(); else stopTimer() }, 1000)
    }
    const pause = () => { stopTimer(); void sample() }
    const ended = () => {
      stopTimer()
      if (active() && element.ended) return advance('COMPLETED', expected)
    }
    element.addEventListener('playing', playing)
    element.addEventListener('pause', pause)
    element.addEventListener('ended', ended, { once: true })
    releasePlayback = () => {
      stopTimer()
      if (playingToken === token) playingToken = undefined
      element.removeEventListener('playing', playing)
      element.removeEventListener('pause', pause)
      element.removeEventListener('ended', ended)
    }
  }

  function _play() {
    if (disposed) return
    bindPlayback()
    void audioRef.value?.play().catch((error: Error) => emitter.emit('toast', error.message))
  }

  function advance(action: 'NEXT' | 'PREVIOUS' | 'COMPLETED', expected?: { path: string; revision: number }) {
    const context = getGraphqlContextKey()
    const command = pendingCommand.then(async () => {
      if (disposed || context !== getGraphqlContextKey()) return
      const snapshot = expected || { path: audioPlayback.value.path || '', revision: audioPlayback.value.revision }
      const result = await advanceSelection({ action, expectedPath: snapshot.path, expectedRevision: snapshot.revision })
      if (!result || disposed || context !== getGraphqlContextKey()) return
      const selection = result.data?.advanceAudioSelection
      if (!selection) { await store.refetch(); return }
      const track = selection.track as IAudioItem | null
      audioPlayback.value = { ...audioPlayback.value, mediaId: track?.mediaId ?? null,
        path: track?.path ?? null, revision: selection.revision, positionMs: selection.positionMs }
      current.value = track || undefined
      if (!track) { audioRef.value?.pause(); return }
      await nextTick()
      if (disposed || context !== getGraphqlContextKey()) return
      if (audioRef.value) audioRef.value.currentTime = selection.positionMs / 1000
      _play()
    })
    pendingCommand = command.catch(() => {})
    return command
  }

  function playPrev() { return advance('PREVIOUS') }
  function playNext() { return advance('NEXT') }
  function changeMode() {
    const context = getGraphqlContextKey()
    const command = pendingCommand.then(async () => {
      if (disposed || context !== getGraphqlContextKey()) return
      const modes = ['REPEAT', 'REPEAT_ONE', 'SHUFFLE'] as const
      const mode = modes[(modes.findIndex((mode) => mode === audioPlayback.value.mode) + 1) % modes.length]
      const result = await updateMode({ mode })
      if (!disposed && context === getGraphqlContextKey() && result?.data?.updateAudioPlayMode) {
        audioPlayback.value = { ...audioPlayback.value, mode }
      }
    })
    pendingCommand = command.catch(() => {})
    return command
  }

  function playItem(item: IAudioItem) {
    play({ mediaId: item.mediaId })
  }

  async function deleteItem(item: IAudioItem) {
    if (!item.mediaId) return
    const result = await deleteAudio({ mediaId: item.mediaId })
    if (!result) return
    store.removeLocal(item.mediaId)
    if (audioPlayback.value.mediaId === item.mediaId) {
      audioRef.value?.pause()
      audioPlayback.value = { ...audioPlayback.value, mediaId: null, path: null, isPlaying: false, positionMs: 0 }
    }
    await store.refetch()
  }

  async function onReorder() {
    const result = await reorderAudioQueue({ mediaIds: playlistAudios.value.map((item) => item.mediaId) })
    if (!result) await store.refetch()
  }

  function clearPlaylist() {
    void clear()
  }

  const onPlay = () => {
    bindPlayback()
    audioPlaying.value = true
    updateMediaSessionPlaybackState()
  }
  const onPause = () => {
    audioPlaying.value = false
    updateMediaSessionPlaybackState()
  }
  const doPlayAudio = () => {
    void nextTick(() => _play())
  }
  const pauseAudio = () => audioRef.value?.pause()

  onMounted(() => {
    store.ensureLoaded()
    setupMediaSessionActions()
    emitter.on('do_play_audio', doPlayAudio)
    emitter.on('pause_audio', pauseAudio)
    audioRef.value?.addEventListener('pause', onPause)
    audioRef.value?.addEventListener('play', onPlay)
  })

  onUnmounted(() => {
    disposed = true
    playingToken = undefined
    releasePlayback?.()
    emitter.off('do_play_audio', doPlayAudio)
    emitter.off('pause_audio', pauseAudio)
    audioRef.value?.removeEventListener('pause', onPause)
    audioRef.value?.removeEventListener('play', onPlay)
    clearMediaSessionActions()
  })

  return {
    app,
    playback: audioPlayback,
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
