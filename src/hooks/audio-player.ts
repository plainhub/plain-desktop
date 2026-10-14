import { computed, nextTick, onMounted, onUnmounted, ref, watch, type Ref } from 'vue'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import type { IAudioItem } from '@/lib/interfaces'
import { getFileUrlByPath } from '@/lib/api/file'
import { initMutation, playAudioGQL, updateAudioPlayModeGQL, removeAudioFromQueueGQL, clearAudioQueueGQL, reorderAudioQueueGQL, enqueueAndPlayAudioGQL } from '@/lib/api/mutation'
import { audioPlayback, useAudioPlaylistStore } from '@/hooks/audio-playlist-store'
import emitter from '@/plugins/eventbus'

export function usePlayAudio(onApplied: () => void, enqueue = false) {
  const store = useAudioPlaylistStore()
  const { mutate: play, loading, onDone } = initMutation({ document: enqueue ? enqueueAndPlayAudioGQL : playAudioGQL })
  onDone((r: any) => {
    const path = r?.data?.playAudio?.path
    if (path) audioPlayback.value = { ...audioPlayback.value, mediaId: r.data.playAudio.mediaId, path: path }
    void store.refetch()
    onApplied()
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
    current.value = audios.value.find((it) => it.mediaId === audioPlayback.value.mediaId)
    updateMediaSessionMetadata()
  }
  setCurrent()
  watch(() => audioPlayback.value.path, setCurrent)
  watch(audios, setCurrent)
  const { play } = usePlayAudio(() => {
    void nextTick(() => _play())
  })
  const { mutate: clear, loading: clearLoading, onDone: onClearDone } = initMutation({ document: clearAudioQueueGQL })
  const { mutate: updatePlayMode } = initMutation({ document: updateAudioPlayModeGQL })
  const { mutate: reorderAudioQueue } = initMutation({ document: reorderAudioQueueGQL })
  const { mutate: deleteAudio } = initMutation({ document: removeAudioFromQueueGQL })

  onClearDone(() => {
    audioPlayback.value = { ...audioPlayback.value, mediaId: null, path: null, isPlaying: false, positionMs: 0 }
    store.reset()
  })

  function _play() {
    audioRef.value?.play()
  }
  async function loadThrough(index: number) {
    while (audios.value.length <= index && audios.value.length < store.total.value) {
      const before = audios.value.length
      await store.loadMore()
      if (audios.value.length <= before) return false
    }
    return true
  }

  async function currentIndex() {
    const id = audioPlayback.value.mediaId
    if (!id) return -1
    let index = audios.value.findIndex((item) => item.mediaId === id)
    while (index < 0 && audios.value.length < store.total.value) {
      const before = audios.value.length
      await store.loadMore()
      if (audios.value.length <= before) return null
      index = audios.value.findIndex((item) => item.mediaId === id)
    }
    return index
  }

  async function playAt(index: number) {
    if (!(await loadThrough(index))) return
    const item = audios.value[index]
    if (item?.mediaId) play({ mediaId: item.mediaId })
  }

  async function playRandom() {
    if (store.total.value) await playAt(Math.floor(Math.random() * store.total.value))
  }

  async function playSequential(next: boolean) {
    if (!store.total.value) return
    const index = await currentIndex()
    if (index === null) return
    const target = next ? (index + 1) % store.total.value : index <= 0 ? store.total.value - 1 : index - 1
    await playAt(target)
  }

  async function playPrev() {
    if (audioPlayback.value.mode === 'SHUFFLE') await playRandom()
    else await playSequential(false)
  }

  async function playNext() {
    if (audioPlayback.value.mode === 'SHUFFLE') await playRandom()
    else await playSequential(true)
  }

  function onEnded() {
    if (audioPlayback.value.mode === 'REPEAT_ONE') {
      if (audioRef.value) audioRef.value.currentTime = 0
      void audioRef.value?.play()
    } else void playNext()
  }

  function changeMode() {
    const modeOrder = { REPEAT: 'REPEAT_ONE', REPEAT_ONE: 'SHUFFLE', SHUFFLE: 'REPEAT' } as const
    const mode = modeOrder[audioPlayback.value.mode as keyof typeof modeOrder] || 'REPEAT'
    updatePlayMode({ mode })
    audioPlayback.value = { ...audioPlayback.value, mode }
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
    onEnded,
    playPrev,
    playNext,
    changeMode,
    playItem,
    deleteItem,
    clearPlaylist,
    onReorder,
  }
}
