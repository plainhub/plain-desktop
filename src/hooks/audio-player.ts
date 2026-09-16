import { computed, onMounted, onUnmounted, ref, watch, type Ref } from 'vue'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import type { IPlaylistAudio } from '@/lib/interfaces'
import { getFileUrlByPath } from '@/lib/api/file'
import { initMutation, playAudioGQL, updateAudioPlayModeGQL, deletePlaylistAudioGQL, clearAudioPlaylistGQL, reorderPlaylistAudiosGQL } from '@/lib/api/mutation'
import { sample } from '@/lib/array'
import { useAudioPlaylistStore } from '@/hooks/audio-playlist-store'
import emitter from '@/plugins/eventbus'

export function useAudioPlaylist(audioRef: Ref<HTMLAudioElement | undefined>) {
  const { app, urlTokenKey, audioPlaying } = storeToRefs(useTempStore())
  const store = useAudioPlaylistStore()

  // A view of the phone playback queue; mutations go through GraphQL, then refetch.
  const audios = computed<IPlaylistAudio[]>(() => store.items.value)
  const playlistAudios = computed<IPlaylistAudio[]>({
    get: () => audios.value,
    set: (value) => { store.items.value = value },
  })

  const current = ref<IPlaylistAudio | undefined>()
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
    const { audioCurrent: c } = app.value
    src.value = getFileUrlByPath(urlTokenKey.value, c)
    current.value = audios.value.find((it) => it.path == c)
    updateMediaSessionMetadata()
  }
  setCurrent()
  watch(() => app.value.audioCurrent, setCurrent)
  watch(audios, setCurrent)

  // Mutations
  const { mutate: play, onDone: playDone } = initMutation({ document: playAudioGQL })
  const { mutate: clear, loading: clearLoading } = initMutation({ document: clearAudioPlaylistGQL })
  const { mutate: updatePlayMode } = initMutation({ document: updateAudioPlayModeGQL })
  const { mutate: reorderPlaylistAudios } = initMutation({ document: reorderPlaylistAudiosGQL })
  const { mutate: deleteAudio } = initMutation({ document: deletePlaylistAudioGQL })

  // Clear handler
  const clearDone = () => {
    app.value = { ...app.value, audioCurrent: '' }
    store.reset()
  }

  function _play() { audioRef.value?.play() }

  playDone(() => _play())

  // Navigation
  function playRandom() {
    const c = sample(audios.value)
    if (!c) return
    play({ path: c.path })
    app.value = { ...app.value, audioCurrent: c.path }
  }

  function _playPrev() {
    const index = audios.value.findIndex((it) => it.path === current.value?.path)
    const c = index <= 0 ? audios.value[audios.value.length - 1] : audios.value[index - 1]
    play({ path: c.path })
    app.value = { ...app.value, audioCurrent: c.path }
  }

  function _playNext() {
    const index = audios.value.findIndex((it) => it.path === current.value?.path)
    const c = index + 1 >= audios.value.length ? audios.value[0] : audios.value[index + 1]
    play({ path: c.path })
    app.value = { ...app.value, audioCurrent: c.path }
  }

  function playPrev() {
    if (!audios.value.length) return
    app.value.audioMode === 'SHUFFLE' ? playRandom() : _playPrev()
  }

  function playNext() {
    if (!audios.value.length) return
    app.value.audioMode === 'SHUFFLE' ? playRandom() : _playNext()
  }

  function onEnded() {
    if (!audios.value.length) return
    const mode = app.value.audioMode
    if (mode === 'REPEAT') _playNext()
    else if (mode === 'REPEAT_ONE') audioRef.value?.play()
    else playRandom()
  }

  function changeMode() {
    const modeOrder = { REPEAT: 'REPEAT_ONE', REPEAT_ONE: 'SHUFFLE', SHUFFLE: 'REPEAT' } as const
    const mode = modeOrder[app.value.audioMode as keyof typeof modeOrder] || 'REPEAT'
    updatePlayMode({ mode })
    app.value = { ...app.value, audioMode: mode }
  }

  function playItem(item: IPlaylistAudio) {
    play({ path: item.path })
    app.value = { ...app.value, audioCurrent: item.path }
  }

  function deleteItem(item: IPlaylistAudio) {
    deleteAudio({ path: item.path })
    store.removeLocal(item.path)
  }

  function onReorder() {
    reorderPlaylistAudios({ paths: playlistAudios.value.map((item) => item.path) })
  }

  function clearPlaylist() {
    clear()
    clearDone()
  }

  const onPlay = () => {
    audioPlaying.value = true
    updateMediaSessionPlaybackState()
  }
  const onPause = () => {
    audioPlaying.value = false
    updateMediaSessionPlaybackState()
  }
  const doPlayAudio = () => setTimeout(_play, 500)
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
    audios,
    playlistAudios,
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
